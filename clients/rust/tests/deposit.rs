#![cfg(feature = "test-sbf")]

mod common;

use {
    common::{
        extension_free_mint_account, funded_system_account, initialized_vault_state_account,
        mollusk, token_2022_id, token_account_with_balance, vault_pdas,
    },
    mollusk_svm::result::Check,
    pinocchio_vault_client::{errors::PinocchioVaultError, instructions::DepositBuilder},
    solana_sdk::{account::Account, program_pack::Pack, pubkey::Pubkey, rent::Rent},
    solana_system_interface::program as system_program,
    spl_token_2022::state::Account as Token2022Account,
};

struct DepositFixture {
    mollusk: mollusk_svm::Mollusk,
    depositor: Pubkey,
    depositor_token: Pubkey,
    vault_state: Pubkey,
    vault_token: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
    instruction: solana_instruction::Instruction,
}

const DEPOSIT_AMOUNT: u64 = 500_000;
const DEPOSITOR_BALANCE: u64 = 1_000_000;

/// Deposits are permissionless top-ups (`docs/vault-design.md` §1): the
/// depositor is deliberately *not* `owner`, proving the vault never checks
/// depositor identity.
fn default_fixture() -> DepositFixture {
    let rent = Rent::default();
    let owner = Pubkey::new_unique();
    let depositor = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let (vault_state, state_bump, vault_token, token_bump) = vault_pdas(&owner);
    let depositor_token = Pubkey::new_unique();

    let instruction = DepositBuilder::new()
        .depositor(depositor)
        .vault_state(vault_state)
        .vault_token_account(vault_token)
        .depositor_token_account(depositor_token)
        .mint(mint)
        .token_program(token_2022_id())
        .amount(DEPOSIT_AMOUNT)
        .instruction();

    let accounts = vec![
        (depositor, funded_system_account(1_000_000_000)),
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
            token_account_with_balance(&mint, &depositor, DEPOSITOR_BALANCE, &rent),
        ),
        (mint, extension_free_mint_account(&owner, &rent)),
        (token_2022_id(), mollusk_svm_programs_token_2022::account()),
    ];

    DepositFixture {
        mollusk: mollusk(),
        depositor,
        depositor_token,
        vault_state,
        vault_token,
        accounts,
        instruction,
    }
}

#[test]
fn deposit_transfers_into_the_vault_token_account() {
    let DepositFixture {
        mollusk,
        depositor_token,
        vault_token,
        accounts,
        instruction,
        ..
    } = default_fixture();

    let result =
        mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    let vault_token_account = result.get_account(&vault_token).unwrap();
    let vault_token_data = Token2022Account::unpack(&vault_token_account.data).unwrap();
    assert_eq!(vault_token_data.amount, DEPOSIT_AMOUNT);

    let depositor_token_account = result.get_account(&depositor_token).unwrap();
    let depositor_token_data = Token2022Account::unpack(&depositor_token_account.data).unwrap();
    assert_eq!(
        depositor_token_data.amount,
        DEPOSITOR_BALANCE - DEPOSIT_AMOUNT
    );
}

/// Boundary: amount == 0 is a valid no-op transfer, not a rejection.
#[test]
fn deposit_accepts_zero_amount() {
    let DepositFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = [1u8].into_iter().chain(0u64.to_le_bytes()).collect();

    mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
}

/// Checklist item 4 (`docs/security-checklist.md`) — `Deposit`/`Withdraw`
/// only: `Initialize` creates the vault_state account, so it cannot itself be
/// "wrong-owner".
#[test]
fn test_rejects_wrong_state_owner() {
    let DepositFixture {
        mollusk,
        vault_state,
        mut accounts,
        instruction,
        ..
    } = default_fixture();
    accounts[1] = (vault_state, Account::new(0, 0, &system_program::id()));

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::InvalidOwner.into())],
    );
}

/// Checklist item 5.
#[test]
fn test_rejects_missing_signer() {
    let DepositFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.accounts[0].is_signer = false;

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(
            PinocchioVaultError::MissingRequiredSignature.into(),
        )],
    );
}

/// Checklist item 6.
#[test]
fn test_rejects_invalid_instruction_tag() {
    let DepositFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = vec![9, 0, 0, 0, 0, 0, 0, 0, 0];

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(
            PinocchioVaultError::InvalidInstructionTag.into(),
        )],
    );
}

/// Tag (1) is valid but the remaining bytes are not a well-formed `u64`.
#[test]
fn test_rejects_deserialization_error() {
    let DepositFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = vec![1, 0, 0, 0]; // 3 bytes after the tag, not 8

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::DeserializationError.into())],
    );
}

/// Checklist item 9 — Decision 4's substitution defense: the supplied vault
/// token account must re-derive from vault_state, not merely be *some* real
/// Token-2022 account for the right mint.
#[test]
fn test_rejects_substituted_token_account() {
    let DepositFixture {
        mollusk,
        depositor,
        mut accounts,
        mut instruction,
        ..
    } = default_fixture();
    let rent = Rent::default();
    let mint = accounts[4].0;
    let substituted = Pubkey::new_unique();
    instruction.accounts[2].pubkey = substituted;
    accounts[2] = (
        substituted,
        token_account_with_balance(&mint, &depositor, 0, &rent),
    );

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::TokenAccountMismatch.into())],
    );
}

/// Checklist item 11.
#[test]
fn test_rejects_forged_cpi_target() {
    let DepositFixture {
        mollusk,
        mut accounts,
        mut instruction,
        ..
    } = default_fixture();
    let forged = Pubkey::new_unique();
    instruction.accounts[5].pubkey = forged;
    accounts[5] = (forged, Account::new(0, 0, &system_program::id()));

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::InvalidCpiTarget.into())],
    );
}
