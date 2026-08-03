//! Instruction set — `docs/vault-design.md` §5.
//!
//! The `InstructionTag` (first byte of instruction data) is the *instruction*
//! discriminator; `AccountInitFlag` in `state.rs` is the *account*-level one
//! (§6).
//!
//! There is no Shank-annotated enum here: Shank's derives require `std` +
//! Borsh, which this `no_std` zero-copy crate does not use. See
//! `docs/idl-generation-notes.md`.

/// `docs/vault-design.md` §5 — `Initialize`.
pub const TAG_INITIALIZE: u8 = 0;
/// `docs/vault-design.md` §5 — `Deposit`.
pub const TAG_DEPOSIT: u8 = 1;
/// `docs/vault-design.md` §5 — `Withdraw`.
pub const TAG_WITHDRAW: u8 = 2;

/// Exact number of accounts `Initialize` expects (§8 step 1).
///
/// | # | Account | Flags |
/// |---|---|---|
/// | 0 | `owner` | signer, writable |
/// | 1 | `vault_state` | writable |
/// | 2 | `vault_token_account` | writable |
/// | 3 | `mint` | readonly |
/// | 4 | `token_program` | readonly |
/// | 5 | `system_program` | readonly |
pub const INITIALIZE_ACCOUNT_COUNT: usize = 6;

/// Exact number of accounts `Deposit` expects.
///
/// | # | Account | Flags |
/// |---|---|---|
/// | 0 | `depositor` | signer |
/// | 1 | `vault_state` | readonly |
/// | 2 | `vault_token_account` | writable |
/// | 3 | `depositor_token_account` | writable |
/// | 4 | `mint` | readonly |
/// | 5 | `token_program` | readonly |
///
/// `depositor` need not be `vault_state.owner` — deposits are permissionless
/// top-ups (`docs/vault-design.md` §1); only `Withdraw` is owner-restricted.
pub const DEPOSIT_ACCOUNT_COUNT: usize = 6;

/// Exact number of accounts `Withdraw` expects.
///
/// | # | Account | Flags |
/// |---|---|---|
/// | 0 | `owner` | signer |
/// | 1 | `vault_state` | readonly |
/// | 2 | `vault_token_account` | writable |
/// | 3 | `destination_token_account` | writable |
/// | 4 | `mint` | readonly |
/// | 5 | `token_program` | readonly |
///
/// `owner` must equal the signer stored in `vault_state.owner` (checklist
/// item 7, `NotVaultOwner`) — the one instruction this checklist item guards.
pub const WITHDRAW_ACCOUNT_COUNT: usize = 6;
