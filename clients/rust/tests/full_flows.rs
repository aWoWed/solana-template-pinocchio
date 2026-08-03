#![cfg(feature = "test-sbf")]

//! Multi-instruction integration-style flows (`Initialize` -> `Deposit` ->
//! `Withdraw`) against real, evolving on-chain state — as opposed to
//! `initialize.rs`/`deposit.rs`/`withdraw.rs`, which each hand-construct a
//! single instruction's *input* fixtures directly.
//!
//! **Scope note (2026-08-02):** the consensus plan (Step 7) originally named
//! LiteSVM for this layer. `litesvm 0.15.0` was evaluated and rejected: its
//! own internal use of `solana-loader-v3-interface` (wincode 0.5.x) against
//! its own `Serialize`/`SchemaWrite` bounds (wincode 0.6.x) does not compile
//! against this workspace's `solana-address 2.7`-driven wincode resolution —
//! the same upstream wincode-0.5/0.6 split `clients/rust/Cargo.toml` already
//! documents for why `solana-program-test` was rejected in favor of
//! mollusk-svm. This is a `litesvm` bug against the current Agave 4.1.2
//! dependency graph, not something fixable from this workspace (verified via
//! `cargo check -p pinocchio-vault-client --tests` with `litesvm`/
//! `litesvm-token` added: 27 compile errors, all the same trait-bound
//! mismatch). Mollusk's own `Mollusk::process_instruction_chain` provides the
//! same capability this layer actually needs — persisted account state across
//! successive instructions in one VM — without a second, differently-broken
//! test harness, so that is what this file uses instead.

mod common;

use {
    common::{
        extension_free_mint_account, funded_system_account, mollusk, token_2022_id,
        token_account_with_balance, vault_pdas,
    },
    mollusk_svm::Mollusk,
    pinocchio_vault_client::{
        accounts::VaultState,
        instructions::{DepositBuilder, InitializeBuilder, WithdrawBuilder},
    },
    solana_sdk::{
        account::Account, instruction::InstructionError, program_pack::Pack, pubkey::Pubkey,
        rent::Rent,
    },
    solana_system_interface::program as system_program,
    spl_token_2022::state::Account as Token2022Account,
};

const DEPOSIT_AMOUNT: u64 = 700_000;
const WITHDRAW_AMOUNT: u64 = 250_000;

struct FlowFixture {
    mollusk: Mollusk,
    owner: Pubkey,
    mint: Pubkey,
    vault_state: Pubkey,
    vault_token: Pubkey,
    owner_token: Pubkey,
    destination_token: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
}

/// `owner` doubles as the depositor (deposits are permissionless, but a
/// single actor exercising the whole lifecycle is the realistic common case
/// and keeps the fixture to one signer).
fn default_fixture() -> FlowFixture {
    let rent = Rent::default();
    let owner = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let (vault_state, _state_bump, vault_token, _token_bump) = vault_pdas(&owner);
    let owner_token = Pubkey::new_unique();
    let destination_token = Pubkey::new_unique();

    let accounts = vec![
        (owner, funded_system_account(10_000_000_000)),
        (vault_state, Account::new(0, 0, &system_program::id())),
        (vault_token, Account::new(0, 0, &system_program::id())),
        (mint, extension_free_mint_account(&owner, &rent)),
        (token_2022_id(), mollusk_svm_programs_token_2022::account()),
        mollusk_svm::program::keyed_account_for_system_program(),
        (
            owner_token,
            token_account_with_balance(&mint, &owner, 10_000_000, &rent),
        ),
        (
            destination_token,
            token_account_with_balance(&mint, &owner, 0, &rent),
        ),
    ];

    FlowFixture {
        mollusk: mollusk(),
        owner,
        mint,
        vault_state,
        vault_token,
        owner_token,
        destination_token,
        accounts,
    }
}

fn initialize_ix(f: &FlowFixture) -> solana_instruction::Instruction {
    InitializeBuilder::new()
        .owner(f.owner)
        .vault_state(f.vault_state)
        .vault_token_account(f.vault_token)
        .mint(f.mint)
        .token_program(token_2022_id())
        .system_program(system_program::id())
        .instruction()
}

fn deposit_ix(f: &FlowFixture, amount: u64) -> solana_instruction::Instruction {
    DepositBuilder::new()
        .depositor(f.owner)
        .vault_state(f.vault_state)
        .vault_token_account(f.vault_token)
        .depositor_token_account(f.owner_token)
        .mint(f.mint)
        .token_program(token_2022_id())
        .amount(amount)
        .instruction()
}

fn withdraw_ix(f: &FlowFixture, signer: Pubkey, amount: u64) -> solana_instruction::Instruction {
    WithdrawBuilder::new()
        .owner(signer)
        .vault_state(f.vault_state)
        .vault_token_account(f.vault_token)
        .destination_token_account(f.destination_token)
        .mint(f.mint)
        .token_program(token_2022_id())
        .amount(amount)
        .instruction()
}

/// The full lifecycle, chained against real, persisted account state
/// (`Mollusk::process_instruction_chain`) rather than three independently
/// hand-constructed fixtures.
#[test]
fn full_flow_initialize_then_deposit_then_withdraw() {
    let f = default_fixture();
    let instructions = vec![
        initialize_ix(&f),
        deposit_ix(&f, DEPOSIT_AMOUNT),
        withdraw_ix(&f, f.owner, WITHDRAW_AMOUNT),
    ];

    let result = f
        .mollusk
        .process_instruction_chain(&instructions, &f.accounts);
    assert!(
        result.raw_result.is_ok(),
        "chain failed: {:?}",
        result.raw_result
    );

    let state_account = result.get_account(&f.vault_state).unwrap();
    assert_eq!(state_account.owner, pinocchio_vault_client::ID);
    let vault_state = VaultState::from_bytes(&state_account.data).unwrap();
    assert_eq!(vault_state.owner, f.owner);
    assert_eq!(vault_state.account_init_flag, 1);

    let vault_token_account = result.get_account(&f.vault_token).unwrap();
    let vault_token_data = Token2022Account::unpack(&vault_token_account.data).unwrap();
    assert_eq!(vault_token_data.amount, DEPOSIT_AMOUNT - WITHDRAW_AMOUNT);

    let destination_account = result.get_account(&f.destination_token).unwrap();
    let destination_data = Token2022Account::unpack(&destination_account.data).unwrap();
    assert_eq!(destination_data.amount, WITHDRAW_AMOUNT);
}

/// Checklist item 3, exercised against a *real* prior `Initialize` in the
/// same chain rather than a hand-constructed already-initialized fixture.
#[test]
fn full_flow_double_initialize_fails() {
    let f = default_fixture();
    let instructions = vec![initialize_ix(&f), initialize_ix(&f)];

    let result = f
        .mollusk
        .process_instruction_chain(&instructions, &f.accounts);
    assert_eq!(
        result.raw_result,
        Err(InstructionError::Custom(2)), // AlreadyInitialized
        "second Initialize in the chain should fail against the now-initialized vault_state"
    );
}

/// Checklist item 7 (pre-mortem #1), exercised after a real `Deposit` funds
/// the vault, so the attacker has something worth draining.
#[test]
fn full_flow_non_owner_withdraw_after_real_deposit() {
    let f = default_fixture();
    let attacker = Pubkey::new_unique();
    let mut accounts = f.accounts.clone();
    accounts.push((attacker, funded_system_account(1_000_000_000)));

    let instructions = vec![
        initialize_ix(&f),
        deposit_ix(&f, DEPOSIT_AMOUNT),
        withdraw_ix(&f, attacker, WITHDRAW_AMOUNT),
    ];

    let result = f
        .mollusk
        .process_instruction_chain(&instructions, &accounts);
    assert_eq!(
        result.raw_result,
        Err(InstructionError::Custom(7)), // NotVaultOwner
    );
}

/// Checklist item 10, exercised against the vault token account's *real*
/// balance after a real `Deposit`, not a hand-set balance.
#[test]
fn full_flow_overdraw_after_real_deposit() {
    let f = default_fixture();
    let instructions = vec![
        initialize_ix(&f),
        deposit_ix(&f, DEPOSIT_AMOUNT),
        withdraw_ix(&f, f.owner, DEPOSIT_AMOUNT + 1),
    ];

    let result = f
        .mollusk
        .process_instruction_chain(&instructions, &f.accounts);
    assert_eq!(
        result.raw_result,
        Err(InstructionError::Custom(10)), // InsufficientFunds
    );
}
