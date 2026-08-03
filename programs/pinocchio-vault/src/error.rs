//! Maps 1:1 to `docs/vault-design.md` §9 (14 codes, 0-13 — code 13 was added
//! during implementation; see the doc's 2026-08-01 note).
//!
//! The discriminant *is* the numbered error code, so variants must never be
//! reordered or inserted mid-list.

use pinocchio::error::ProgramError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    /// 0 - Wrong number of accounts passed
    AccountCountMismatch = 0,
    /// 1 - Two account slots hold the same address where they must differ
    DuplicateAccount = 1,
    /// 2 - Initialize called on an account with account_init_flag == 1
    AlreadyInitialized = 2,
    /// 3 - Vault state account not owned by this program
    InvalidOwner = 3,
    /// 4 - A required signer is not marked as a signer
    MissingRequiredSignature = 4,
    /// 5 - Instruction tag byte is not a recognised InstructionTag
    InvalidInstructionTag = 5,
    /// 6 - Mint carries an extension not in the accepted set
    UnsupportedMintExtension = 6,
    /// 7 - Withdraw signer does not match vault.owner
    NotVaultOwner = 7,
    /// 8 - Vault state PDA re-derivation does not match the supplied account
    InvalidPda = 8,
    /// 9 - Supplied token account does not match the re-derived address
    TokenAccountMismatch = 9,
    /// 10 - Withdraw amount exceeds the vault token account's actual balance
    InsufficientFunds = 10,
    /// 11 - Token-program account does not match the expected program ID
    InvalidCpiTarget = 11,
    /// 12 - Instruction data present but malformed for the given InstructionTag
    DeserializationError = 12,
    /// 13 - Supplied mint is not a Token-2022 mint
    InvalidMint = 13,
}

impl From<VaultError> for ProgramError {
    fn from(e: VaultError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
