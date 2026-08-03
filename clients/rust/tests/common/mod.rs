#![cfg(feature = "test-sbf")]
#![allow(dead_code)]

//! Shared mollusk-svm test fixtures for `initialize.rs`/`deposit.rs`/`withdraw.rs`.
//! Kept in one place so the byte-level account shapes (`docs/vault-design.md`
//! §4) and the extension-TLV construction below cannot silently diverge
//! between the three instructions' test files.

use {
    mollusk_svm::Mollusk,
    pinocchio_vault_client::{ID, accounts::VaultState},
    solana_sdk::{
        account::Account, program_option::COption, program_pack::Pack, pubkey::Pubkey, rent::Rent,
    },
    spl_token_2022::{
        extension::ExtensionType,
        state::{Account as Token2022Account, AccountState, Mint},
    },
};

/// `docs/vault-design.md` §3.
pub const VAULT_TOKEN_SEED: &[u8] = b"vault_token";

/// `docs/vault-design.md` §4, offset table.
const ACCOUNT_INIT_FLAG_OFFSET: usize = 0;
const OWNER_OFFSET: usize = 1;
const MINT_OFFSET: usize = 33;
const TOKEN_ACCOUNT_OFFSET: usize = 65;
const BUMP_OFFSET: usize = 97;
const TOKEN_ACCOUNT_BUMP_OFFSET: usize = 98;
const VAULT_STATE_LEN: usize = 107;

/// `programs/pinocchio-vault/src/token2022.rs`'s `BASE_TOKEN_ACCOUNT_LEN` —
/// where an extended Token-2022 mint's `account_type` byte lives, followed by
/// the TLV region.
const BASE_TOKEN_ACCOUNT_LEN: usize = 165;
const ACCOUNT_TYPE_MINT: u8 = 1;
const TLV_HEADER_LEN: usize = 4;

/// mollusk-svm-programs-token-2022 pins the real SPL Token-2022 ELF — the
/// same program `scripts/program/dump.mjs` pulls into `target/deploy`.
pub fn token_2022_id() -> Pubkey {
    mollusk_svm_programs_token_2022::ID
}

pub fn mollusk() -> Mollusk {
    let mut m = Mollusk::new(&ID, "pinocchio_vault");
    mollusk_svm_programs_token_2022::add_program(&mut m);
    m
}

/// Vault state PDA + canonical bump, and the vault token account PDA (derived
/// from the vault state PDA, per Decision 4) + its own canonical bump.
pub fn vault_pdas(owner: &Pubkey) -> (Pubkey, u8, Pubkey, u8) {
    let (vault_state, state_bump) = VaultState::find_pda(owner);
    let (vault_token, token_bump) =
        Pubkey::find_program_address(&[VAULT_TOKEN_SEED, vault_state.as_ref()], &ID);
    (vault_state, state_bump, vault_token, token_bump)
}

/// A funded, System-owned account suitable for a signer input (e.g. `owner`,
/// `depositor`) that never itself gets written to by the instruction under
/// test.
pub fn funded_system_account(lamports: u64) -> Account {
    Account::new(lamports, 0, &solana_system_interface::program::id())
}

/// An extension-free Token-2022 mint (`docs/vault-design.md` §7's only
/// accepted shape), already initialized with `mint_authority = owner`.
pub fn extension_free_mint_account(owner: &Pubkey, rent: &Rent) -> Account {
    let mint = Mint {
        mint_authority: COption::Some(*owner),
        supply: 0,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    let mut data = vec![0u8; Mint::LEN];
    Mint::pack(mint, &mut data).unwrap();

    Account {
        lamports: rent.minimum_balance(Mint::LEN),
        data,
        owner: token_2022_id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// A Token-2022 mint carrying exactly one initialized extension TLV entry,
/// tagged with that extension's *real* on-wire type ID (from
/// `spl_token_2022::extension::ExtensionType`, which is `#[repr(u16)]` and
/// matches the actual protocol numbering) and an all-zero body.
///
/// `programs/pinocchio-vault/src/token2022.rs`'s `assert_supported_mint` only
/// inspects the first TLV entry's *type* field (§7's default-reject posture
/// treats every extension identically) — it never reads the body — so this
/// construction exercises the exact real type ID Token-2022 uses for each
/// named extension without needing each extension's full pod-layout API,
/// which is exactly what makes these six tests distinct from one another
/// rather than six copies of the same "any extension" case.
pub fn mint_with_extension(owner: &Pubkey, rent: &Rent, extension_type: ExtensionType) -> Account {
    let space = ExtensionType::try_calculate_account_len::<Mint>(&[extension_type]).unwrap();
    let mut data = vec![0u8; space];

    let mint = Mint {
        mint_authority: COption::Some(*owner),
        supply: 0,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    Mint::pack(mint, &mut data[0..Mint::LEN]).unwrap();

    data[BASE_TOKEN_ACCOUNT_LEN] = ACCOUNT_TYPE_MINT;

    let tlv_offset = BASE_TOKEN_ACCOUNT_LEN + 1;
    let type_id = extension_type as u16;
    let body_len = (space - tlv_offset - TLV_HEADER_LEN) as u16;
    data[tlv_offset..tlv_offset + 2].copy_from_slice(&type_id.to_le_bytes());
    data[tlv_offset + 2..tlv_offset + 4].copy_from_slice(&body_len.to_le_bytes());
    // Body bytes (if any) are left zeroed -- assert_supported_mint never reads them.

    Account {
        lamports: rent.minimum_balance(space),
        data,
        owner: token_2022_id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// A Token-2022 token account with the given `amount`, owned by
/// `token_authority` (never the human vault owner -- §2).
pub fn token_account_with_balance(
    mint: &Pubkey,
    token_authority: &Pubkey,
    amount: u64,
    rent: &Rent,
) -> Account {
    let token = Token2022Account {
        mint: *mint,
        owner: *token_authority,
        amount,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    let mut data = vec![0u8; Token2022Account::LEN];
    Token2022Account::pack(token, &mut data).unwrap();

    Account {
        lamports: rent.minimum_balance(Token2022Account::LEN),
        data,
        owner: token_2022_id(),
        executable: false,
        rent_epoch: 0,
    }
}

/// Pre-initialized `VaultState` account bytes, matching `docs/vault-design.md`
/// §4 byte-for-byte -- lets `Deposit`/`Withdraw` tests start from an
/// already-initialized vault without running `Initialize` first.
pub fn initialized_vault_state_account(
    owner: &Pubkey,
    mint: &Pubkey,
    token_account: &Pubkey,
    bump: u8,
    token_account_bump: u8,
    rent: &Rent,
) -> Account {
    let mut data = vec![0u8; VAULT_STATE_LEN];
    data[ACCOUNT_INIT_FLAG_OFFSET] = 1;
    data[OWNER_OFFSET..OWNER_OFFSET + 32].copy_from_slice(owner.as_ref());
    data[MINT_OFFSET..MINT_OFFSET + 32].copy_from_slice(mint.as_ref());
    data[TOKEN_ACCOUNT_OFFSET..TOKEN_ACCOUNT_OFFSET + 32].copy_from_slice(token_account.as_ref());
    data[BUMP_OFFSET] = bump;
    data[TOKEN_ACCOUNT_BUMP_OFFSET] = token_account_bump;

    Account {
        lamports: rent.minimum_balance(VAULT_STATE_LEN),
        data,
        owner: ID,
        executable: false,
        rent_epoch: 0,
    }
}
