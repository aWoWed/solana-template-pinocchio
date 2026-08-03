#![cfg(feature = "test-sbf")]

mod common;

use {
    common::{
        extension_free_mint_account, funded_system_account, initialized_vault_state_account,
        mollusk, token_2022_id, token_account_with_balance, vault_pdas,
    },
    mollusk_svm::result::Check,
    pinocchio_vault_client::{errors::PinocchioVaultError, instructions::WithdrawBuilder},
    solana_sdk::{account::Account, program_pack::Pack, pubkey::Pubkey, rent::Rent},
    solana_system_interface::program as system_program,
    spl_token_2022::state::Account as Token2022Account,
};

struct WithdrawFixture {
    mollusk: mollusk_svm::Mollusk,
    owner: Pubkey,
    vault_state: Pubkey,
    vault_token: Pubkey,
    destination_token: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
    instruction: solana_instruction::Instruction,
}

const VAULT_BALANCE: u64 = 1_000_000;
const WITHDRAW_AMOUNT: u64 = 400_000;

fn default_fixture() -> WithdrawFixture {
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
        .amount(WITHDRAW_AMOUNT)
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
            token_account_with_balance(&mint, &vault_state, VAULT_BALANCE, &rent),
        ),
        (
            destination_token,
            token_account_with_balance(&mint, &owner, 0, &rent),
        ),
        (mint, extension_free_mint_account(&owner, &rent)),
        (token_2022_id(), mollusk_svm_programs_token_2022::account()),
    ];

    WithdrawFixture {
        mollusk: mollusk(),
        owner,
        vault_state,
        vault_token,
        destination_token,
        accounts,
        instruction,
    }
}

/// The CPI is `invoke_signed` by the vault state PDA using its *stored* bump
/// — never the owner's own signature (`docs/vault-design.md` §2) — so a
/// passing test here is itself evidence that program-signing path works.
#[test]
fn withdraw_transfers_out_of_the_vault_token_account() {
    let WithdrawFixture {
        mollusk,
        vault_token,
        destination_token,
        accounts,
        instruction,
        ..
    } = default_fixture();

    let result =
        mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    let vault_token_account = result.get_account(&vault_token).unwrap();
    let vault_token_data = Token2022Account::unpack(&vault_token_account.data).unwrap();
    assert_eq!(vault_token_data.amount, VAULT_BALANCE - WITHDRAW_AMOUNT);

    let destination_token_account = result.get_account(&destination_token).unwrap();
    let destination_token_data = Token2022Account::unpack(&destination_token_account.data).unwrap();
    assert_eq!(destination_token_data.amount, WITHDRAW_AMOUNT);
}

/// Boundary: amount == 0 succeeds (it is <= any real balance).
#[test]
fn withdraw_accepts_zero_amount() {
    let WithdrawFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = [2u8].into_iter().chain(0u64.to_le_bytes()).collect();

    mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);
}

/// Checklist item 4.
#[test]
fn test_rejects_wrong_state_owner() {
    let WithdrawFixture {
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
    let WithdrawFixture {
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
    let WithdrawFixture {
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

/// Tag (2) is valid but the remaining bytes are not a well-formed `u64`.
#[test]
fn test_rejects_deserialization_error() {
    let WithdrawFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = vec![2, 0, 0]; // 2 bytes after the tag, not 8

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::DeserializationError.into())],
    );
}

/// Checklist item 7 — pre-mortem #1, the primary drain-the-vault attack this
/// template exists to demonstrate defeating.
#[test]
fn test_rejects_non_owner_withdraw() {
    let WithdrawFixture {
        mollusk,
        vault_state,
        mut accounts,
        instruction,
        ..
    } = default_fixture();
    let rent = Rent::default();
    let real_owner = Pubkey::new_unique(); // != the `owner` signer in the fixture's instruction
    let (_, state_bump, vault_token, token_bump) = vault_pdas(&real_owner);
    let mint = accounts[4].0;
    // Rebuild vault_state so its recorded owner is someone other than the
    // signer the instruction actually presents.
    accounts[1] = (
        vault_state,
        initialized_vault_state_account(
            &real_owner,
            &mint,
            &vault_token,
            state_bump,
            token_bump,
            &rent,
        ),
    );

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::NotVaultOwner.into())],
    );
}

/// Checklist item 8 — pre-mortem #2. The program only ever signs with the
/// *stored* bump; there is no separate "forged instruction-data bump" path
/// to test (closed by construction), so this simulates the PDA-substitution
/// half: `vault_state` is not the address its own stored (owner, bump)
/// re-derive to.
#[test]
fn test_rejects_forged_bump() {
    let WithdrawFixture {
        mollusk,
        owner,
        mut accounts,
        mut instruction,
        ..
    } = default_fixture();
    let rent = Rent::default();
    let (_, state_bump, vault_token, token_bump) = vault_pdas(&owner);
    let mint = accounts[4].0;
    let wrong_address = Pubkey::new_unique();

    instruction.accounts[1].pubkey = wrong_address;
    accounts[1] = (
        wrong_address,
        initialized_vault_state_account(&owner, &mint, &vault_token, state_bump, token_bump, &rent),
    );

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::InvalidPda.into())],
    );
}

/// Checklist item 9.
#[test]
fn test_rejects_substituted_token_account() {
    let WithdrawFixture {
        mollusk,
        owner,
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
        token_account_with_balance(&mint, &owner, VAULT_BALANCE, &rent),
    );

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::TokenAccountMismatch.into())],
    );
}

/// Checklist item 10 — the token account's real on-chain balance is the
/// single source of truth, not a duplicated ledger in `VaultState`.
#[test]
fn test_rejects_overdraw() {
    let WithdrawFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = [2u8]
        .into_iter()
        .chain((VAULT_BALANCE + 1).to_le_bytes())
        .collect();

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::InsufficientFunds.into())],
    );
}

/// Boundary: amount == u64::MAX against a small real balance must fail as an
/// ordinary overdraw, not panic/overflow.
#[test]
fn test_rejects_max_u64_overdraw() {
    let WithdrawFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = [2u8].into_iter().chain(u64::MAX.to_le_bytes()).collect();

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::InsufficientFunds.into())],
    );
}

/// Checklist item 11.
#[test]
fn test_rejects_forged_cpi_target() {
    let WithdrawFixture {
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
