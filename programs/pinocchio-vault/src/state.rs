//! Vault state account — `docs/vault-design.md` §4.
//!
//! Zero-copy: the 107 on-chain bytes are reinterpreted in place as a
//! `#[repr(C)]` struct of `u8`/`[u8; N]` fields only. Every field is
//! byte-aligned and fixed-width, so the struct's memory layout is exactly the
//! byte layout in §4. No deserialization step, no allocation.

use {crate::error::VaultError, pinocchio::Address};

/// `docs/vault-design.md` §3 — vault state PDA seed prefix.
pub const VAULT_SEED: &[u8] = b"vault";

/// `docs/vault-design.md` §3 — vault token account PDA seed prefix.
pub const VAULT_TOKEN_SEED: &[u8] = b"vault_token";

/// `AccountInitFlag` values (§6). Distinct from `InstructionTag`.
pub const ACCOUNT_INIT_FLAG_UNINITIALIZED: u8 = 0;
pub const ACCOUNT_INIT_FLAG_INITIALIZED: u8 = 1;

#[repr(C)]
pub struct VaultState {
    /// Offset 0..1 — checked before any other logic.
    pub account_init_flag: u8,
    /// Offset 1..33
    pub owner: [u8; 32],
    /// Offset 33..65
    pub mint: [u8; 32],
    /// Offset 65..97
    pub token_account: [u8; 32],
    /// Offset 97..98 — canonical bump for the vault state PDA.
    pub bump: u8,
    /// Offset 98..99 — canonical bump for the vault token account PDA.
    pub token_account_bump: u8,
    /// Offset 99..107 — zeroed at init.
    pub reserved: [u8; 8],
}

impl VaultState {
    pub const LEN: usize = core::mem::size_of::<Self>();

    /// Reinterprets `bytes` as a `VaultState` without copying.
    ///
    /// # Safety
    /// `VaultState` is `#[repr(C)]` over `u8`/`[u8; N]` only, so it has
    /// alignment 1 and no padding or invalid bit patterns. Any byte slice of
    /// exactly `LEN` bytes is therefore a valid `VaultState`.
    pub fn from_bytes_mut(bytes: &mut [u8]) -> Result<&mut Self, VaultError> {
        if bytes.len() != Self::LEN {
            return Err(VaultError::DeserializationError);
        }
        Ok(unsafe { &mut *(bytes.as_mut_ptr() as *mut Self) })
    }

    /// Read-only counterpart of [`Self::from_bytes_mut`], used by
    /// `Deposit`/`Withdraw` (which read state but never write it — see
    /// `docs/vault-design.md` §8's "no-op effects" note).
    pub fn from_bytes(bytes: &[u8]) -> Result<&Self, VaultError> {
        if bytes.len() != Self::LEN {
            return Err(VaultError::DeserializationError);
        }
        Ok(unsafe { &*(bytes.as_ptr() as *const Self) })
    }

    pub fn owner_address(&self) -> Address {
        Address::from(self.owner)
    }

    pub fn token_account_address(&self) -> Address {
        Address::from(self.token_account)
    }

    pub fn write(
        &mut self,
        owner: &Address,
        mint: &Address,
        token_account: &Address,
        bump: u8,
        token_account_bump: u8,
    ) {
        self.owner = *owner.as_array();
        self.mint = *mint.as_array();
        self.token_account = *token_account.as_array();
        self.bump = bump;
        self.token_account_bump = token_account_bump;
        self.reserved = [0u8; 8];
        // Written last: the account only counts as initialized once every other
        // field is committed.
        self.account_init_flag = ACCOUNT_INIT_FLAG_INITIALIZED;
    }
}

/// `docs/vault-design.md` §4 fixes this at 107 bytes — pinned here so a
/// silent layout drift fails the build, not just a test.
const _: () = assert!(VaultState::LEN == 107);
