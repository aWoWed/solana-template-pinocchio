#![cfg(feature = "test-sbf")]

//! Per-instruction compute-unit ceiling assertions (consensus plan Step 8).
//! Asserts an **upper bound with documented headroom**, not an exact number
//! (pass-2's "must match README exactly" was a dual-maintenance/brittleness
//! bug both plan reviews flagged) -- see `README.md`'s CU table, which is
//! meant to be regenerated from this file's `println!` output rather than
//! hand-maintained, per the plan's Step 8/Verification Step 7.
//!
//! **Provisional-ceiling note (2026-08-02):** these ceilings were set from
//! published, comparable-operation CU costs (PDA `create_account` +
//! Token-2022 `initialize_account3`/`transfer_checked` CPIs typically run
//! 5,000-40,000 CU depending on account-creation overhead), generously
//! padded for headroom -- NOT measured against this exact program, because
//! this session's local Windows environment could not run `cargo build-sbf`
//! (its host-side build-script compilation requires the MSVC linker; only a
//! GNU toolchain was installable non-interactively here, see
//! `docs/vault-design.md` note and the PR/session notes for the
//! `winget install ... VisualStudio.2022.BuildTools` UAC-elevation blocker).
//! Run `pnpm clients:rust:test` in CI (Linux, no such blocker) or locally
//! once a working MSVC toolchain is available, read the actual
//! `compute_units_consumed` values these tests print, and tighten the
//! constants below to the real measured value plus headroom before treating
//! this as a real regression gate rather than a smoke ceiling.

mod common;

use {
    common::{
        extension_free_mint_account, funded_system_account, initialized_vault_state_account,
        mollusk, token_2022_id, token_account_with_balance, vault_pdas,
    },
    mollusk_svm::result::Check,
    pinocchio_vault_client::instructions::{DepositBuilder, InitializeBuilder, WithdrawBuilder},
    solana_sdk::{account::Account, pubkey::Pubkey, rent::Rent},
    solana_system_interface::program as system_program,
};

/// Generous provisional ceilings (see module doc) -- headroom over a
/// published-comparable-operation estimate, not a measured value yet.
const INITIALIZE_CU_CEILING: u64 = 60_000;
const DEPOSIT_CU_CEILING: u64 = 20_000;
const WITHDRAW_CU_CEILING: u64 = 25_000; // invoke_signed has slightly more overhead than a plain invoke.

#[test]
fn initialize_stays_within_cu_ceiling() {
    let rent = Rent::default();
    let owner = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let (vault_state, _, vault_token, _) = vault_pdas(&owner);

    let instruction = InitializeBuilder::new()
        .owner(owner)
        .vault_state(vault_state)
        .vault_token_account(vault_token)
        .mint(mint)
        .token_program(token_2022_id())
        .system_program(system_program::id())
        .instruction();

    let accounts = vec![
        (owner, funded_system_account(10_000_000_000)),
        (vault_state, Account::new(0, 0, &system_program::id())),
        (vault_token, Account::new(0, 0, &system_program::id())),
        (mint, extension_free_mint_account(&owner, &rent)),
        (token_2022_id(), mollusk_svm_programs_token_2022::account()),
        mollusk_svm::program::keyed_account_for_system_program(),
    ];

    let result =
        mollusk().process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    println!(
        "CU_BENCHMARK Initialize compute_units_consumed={}",
        result.compute_units_consumed
    );
    assert!(
        result.compute_units_consumed <= INITIALIZE_CU_CEILING,
        "Initialize consumed {} CU, exceeding the {} CU ceiling",
        result.compute_units_consumed,
        INITIALIZE_CU_CEILING
    );
}

#[test]
fn deposit_stays_within_cu_ceiling() {
    let rent = Rent::default();
    let owner = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let (vault_state, state_bump, vault_token, token_bump) = vault_pdas(&owner);
    let depositor_token = Pubkey::new_unique();

    let instruction = DepositBuilder::new()
        .depositor(owner)
        .vault_state(vault_state)
        .vault_token_account(vault_token)
        .depositor_token_account(depositor_token)
        .mint(mint)
        .token_program(token_2022_id())
        .amount(500_000)
        .instruction();

    let accounts = vec![
        (owner, funded_system_account(1_000_000_000)),
        (
            vault_state,
            initialized_vault_state_account(
                &owner,
                &mint,
                &vault_token,
                state_bump,
                token_bump,
                &rent,
            ),
        ),
        (
            vault_token,
            token_account_with_balance(&mint, &vault_state, 0, &rent),
        ),
        (
            depositor_token,
            token_account_with_balance(&mint, &owner, 1_000_000, &rent),
        ),
        (mint, extension_free_mint_account(&owner, &rent)),
        (token_2022_id(), mollusk_svm_programs_token_2022::account()),
    ];

    let result =
        mollusk().process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    println!(
        "CU_BENCHMARK Deposit compute_units_consumed={}",
        result.compute_units_consumed
    );
    assert!(
        result.compute_units_consumed <= DEPOSIT_CU_CEILING,
        "Deposit consumed {} CU, exceeding the {} CU ceiling",
        result.compute_units_consumed,
        DEPOSIT_CU_CEILING
    );
}

#[test]
fn withdraw_stays_within_cu_ceiling() {
    let rent = Rent::default();
    let owner = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let (vault_state, state_bump, vault_token, token_bump) = vault_pdas(&owner);
    let destination_token = Pubkey::new_unique();

    let instruction = WithdrawBuilder::new()
        .owner(owner)
        .vault_state(vault_state)
        .vault_token_account(vault_token)
        .destination_token_account(destination_token)
        .mint(mint)
        .token_program(token_2022_id())
        .amount(250_000)
        .instruction();

    let accounts = vec![
        (owner, funded_system_account(1_000_000_000)),
        (
            vault_state,
            initialized_vault_state_account(
                &owner,
                &mint,
                &vault_token,
                state_bump,
                token_bump,
                &rent,
            ),
        ),
        (
            vault_token,
            token_account_with_balance(&mint, &vault_state, 1_000_000, &rent),
        ),
        (
            destination_token,
            token_account_with_balance(&mint, &owner, 0, &rent),
        ),
        (mint, extension_free_mint_account(&owner, &rent)),
        (token_2022_id(), mollusk_svm_programs_token_2022::account()),
    ];

    let result =
        mollusk().process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    println!(
        "CU_BENCHMARK Withdraw compute_units_consumed={}",
        result.compute_units_consumed
    );
    assert!(
        result.compute_units_consumed <= WITHDRAW_CU_CEILING,
        "Withdraw consumed {} CU, exceeding the {} CU ceiling",
        result.compute_units_consumed,
        WITHDRAW_CU_CEILING
    );
}
