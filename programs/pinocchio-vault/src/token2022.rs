//! Token-2022 mint/account inspection — `docs/vault-design.md` §7.
//!
//! `pinocchio_token::state::Mint` only understands the *base* 82-byte mint and
//! hardcodes the legacy SPL Token program as the required owner, so the
//! extension allow/deny decision is enforced here by walking the Token-2022 TLV
//! region directly instead.

use {
    crate::error::VaultError,
    pinocchio::{AccountView, Address},
    solana_program_log::log,
};

/// The SPL Token-2022 program. Hardcoded, never read from the account list.
pub const TOKEN_2022_ID: Address =
    Address::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Length of a base (extension-free) Token-2022 mint.
pub const BASE_MINT_LEN: usize = 82;

/// Length of a base (extension-free) Token-2022 token account. Token-2022 pads
/// extended *mints* out to this length so the `account_type` byte that follows
/// can disambiguate a mint from a token account.
pub const BASE_TOKEN_ACCOUNT_LEN: usize = 165;

/// Offset of the `account_type` byte in an extended Token-2022 account.
const ACCOUNT_TYPE_OFFSET: usize = BASE_TOKEN_ACCOUNT_LEN;

/// `AccountType::Mint` discriminant.
const ACCOUNT_TYPE_MINT: u8 = 1;

/// First byte of the TLV region in an extended Token-2022 account.
const TLV_OFFSET: usize = ACCOUNT_TYPE_OFFSET + 1;

/// Size of a single TLV entry header: `u16` type + `u16` length, both
/// little-endian.
const TLV_HEADER_LEN: usize = 4;

/// Reject the mint unless it is an extension-free Token-2022 mint.
///
/// `docs/vault-design.md` §7 sets the accepted-extension set to empty: every
/// named extension is rejected in v1 and the default posture for anything
/// unnamed is also rejection, so any initialized TLV entry at all fails.
pub fn assert_supported_mint(mint: &AccountView) -> Result<(), VaultError> {
    if !mint.owned_by(&TOKEN_2022_ID) {
        log!("Mint is not owned by the Token-2022 program");
        return Err(VaultError::InvalidMint);
    }

    let data = mint.try_borrow().map_err(|_| VaultError::InvalidMint)?;

    if data.len() == BASE_MINT_LEN {
        return Ok(());
    }

    if data.len() <= TLV_OFFSET || data[ACCOUNT_TYPE_OFFSET] != ACCOUNT_TYPE_MINT {
        log!("Mint is not a well-formed Token-2022 mint");
        return Err(VaultError::InvalidMint);
    }

    let tlv = &data[TLV_OFFSET..];
    if tlv.len() >= TLV_HEADER_LEN {
        let extension_type = u16::from_le_bytes([tlv[0], tlv[1]]);
        // Type 0 is `Uninitialized`, which terminates the entry list.
        if extension_type != 0 {
            log!(
                "Rejected mint carrying unsupported extension type: {}",
                extension_type
            );
            return Err(VaultError::UnsupportedMintExtension);
        }
    }

    Ok(())
}

/// Offset of the `decimals` field in a base Token-2022 mint (after
/// `mint_authority: COption<Pubkey>` (36 bytes) + `supply: u64` (8 bytes)).
const MINT_DECIMALS_OFFSET: usize = 44;

/// Reads a mint's `decimals` field. Used to build `transfer_checked`
/// instructions, which — unlike a plain `transfer` — verify the caller's
/// claimed decimals against the mint on-chain, closing a class of
/// wrong-mint/wrong-decimals CPI mistakes `transfer` alone would not catch.
///
/// Callers must run [`assert_supported_mint`] first: this function only
/// checks that the account is long enough to contain the field, not that the
/// account is genuinely owned by the Token-2022 program.
pub fn mint_decimals(mint: &AccountView) -> Result<u8, VaultError> {
    let data = mint.try_borrow().map_err(|_| VaultError::InvalidMint)?;
    data.get(MINT_DECIMALS_OFFSET)
        .copied()
        .ok_or(VaultError::InvalidMint)
}

/// Offset of the `amount` field in a Token-2022 token account (after
/// `mint: Pubkey` (32 bytes) + `owner: Pubkey` (32 bytes)).
const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;

/// Reads a Token-2022 token account's `amount` (real on-chain balance),
/// little-endian. `Withdraw`'s checked-arithmetic guard (checklist item 10)
/// compares the requested amount against this, not a ledger duplicated in
/// `VaultState` — the token account itself is the single source of truth for
/// balance.
pub fn token_account_amount(token_account: &AccountView) -> Result<u64, VaultError> {
    let data = token_account
        .try_borrow()
        .map_err(|_| VaultError::TokenAccountMismatch)?;
    let bytes: [u8; 8] = data
        .get(TOKEN_ACCOUNT_AMOUNT_OFFSET..TOKEN_ACCOUNT_AMOUNT_OFFSET + 8)
        .ok_or(VaultError::TokenAccountMismatch)?
        .try_into()
        .map_err(|_| VaultError::TokenAccountMismatch)?;
    Ok(u64::from_le_bytes(bytes))
}
