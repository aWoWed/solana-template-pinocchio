#![cfg(feature = "test-sbf")]

mod common;

use {
    common::{
        VAULT_TOKEN_SEED, extension_free_mint_account, funded_system_account, mint_with_extension,
        mollusk, token_2022_id, vault_pdas,
    },
    mollusk_svm::{Mollusk, result::Check},
    pinocchio_vault_client::{
        ID, accounts::VaultState, errors::PinocchioVaultError, instructions::InitializeBuilder,
    },
    solana_sdk::{
        account::Account, program_option::COption, program_pack::Pack, pubkey::Pubkey, rent::Rent,
    },
    solana_system_interface::program as system_program,
    spl_token_2022::{
        extension::ExtensionType,
        state::{Account as Token2022Account, AccountState},
    },
};

/// The vault's Token-2022 token account, PDA-derived and empty, exactly as
/// `Initialize` is expected to leave it — used only to assert against, never
/// passed in as an already-created input (§2: the program creates it).
fn expected_token_account(mint: &Pubkey, owner_authority: &Pubkey) -> Token2022Account {
    Token2022Account {
        mint: *mint,
        owner: *owner_authority,
        amount: 0,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    }
}

/// Common six-account happy-path setup, returned so negative tests can mutate
/// exactly the one thing under test.
struct InitializeFixture {
    mollusk: Mollusk,
    owner: Pubkey,
    mint: Pubkey,
    vault_state: Pubkey,
    vault_token: Pubkey,
    accounts: Vec<(Pubkey, Account)>,
    instruction: solana_instruction::Instruction,
}

fn default_fixture() -> InitializeFixture {
    let rent = Rent::default();
    let owner = Pubkey::new_unique();
    let mint = Pubkey::new_unique();
    let (vault_state, _state_bump, vault_token, _token_bump) = vault_pdas(&owner);

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

    InitializeFixture {
        mollusk: mollusk(),
        owner,
        mint,
        vault_state,
        vault_token,
        accounts,
        instruction,
    }
}

#[test]
fn initialize_creates_vault_state_and_program_derived_token_account() {
    let InitializeFixture {
        mollusk,
        owner,
        mint,
        vault_state,
        vault_token,
        accounts,
        instruction,
    } = default_fixture();
    let (_, state_bump) = VaultState::find_pda(&owner);
    let (_, token_bump) =
        Pubkey::find_program_address(&[VAULT_TOKEN_SEED, vault_state.as_ref()], &ID);

    let result =
        mollusk.process_and_validate_instruction(&instruction, &accounts, &[Check::success()]);

    let state_account = result.get_account(&vault_state).unwrap();
    assert_eq!(state_account.owner, ID);
    assert_eq!(state_account.data.len(), VaultState::LEN);

    let vault_state_data = VaultState::from_bytes(&state_account.data).unwrap();
    assert_eq!(vault_state_data.account_init_flag, 1);
    assert_eq!(vault_state_data.owner, owner);
    assert_eq!(vault_state_data.mint, mint);
    assert_eq!(vault_state_data.token_account, vault_token);
    assert_eq!(vault_state_data.bump, state_bump);
    assert_eq!(vault_state_data.token_account_bump, token_bump);
    assert_eq!(vault_state_data.reserved, [0u8; 8]);

    let token_account = result.get_account(&vault_token).unwrap();
    assert_eq!(token_account.owner, token_2022_id());

    let token = Token2022Account::unpack(&token_account.data).unwrap();
    let expected = expected_token_account(&mint, &vault_state);
    assert_eq!(token.mint, expected.mint);
    assert_eq!(token.owner, expected.owner);
    assert_eq!(token.amount, expected.amount);
}

/// Checklist item 1 — `docs/security-checklist.md`.
#[test]
fn test_rejects_wrong_account_count() {
    let InitializeFixture {
        mollusk,
        mut accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.accounts.truncate(5);
    accounts.truncate(5);

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::AccountCountMismatch.into())],
    );
}

/// Checklist item 2.
#[test]
fn test_rejects_duplicate_accounts() {
    let InitializeFixture {
        mollusk,
        vault_state,
        mut accounts,
        mut instruction,
        ..
    } = default_fixture();
    // Alias vault_token_account (slot 2) to vault_state (slot 1).
    instruction.accounts[2].pubkey = vault_state;
    accounts[2].0 = vault_state;

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::DuplicateAccount.into())],
    );
}

/// Checklist item 3 — pre-mortem #5, re-initialization must be rejected
/// before any other logic runs.
#[test]
fn test_rejects_double_initialize() {
    let InitializeFixture {
        mollusk,
        owner,
        mint,
        vault_state,
        vault_token,
        mut accounts,
        instruction,
    } = default_fixture();
    let rent = Rent::default();
    let (_, state_bump, _, token_bump) = vault_pdas(&owner);
    accounts[1] = (
        vault_state,
        common::initialized_vault_state_account(
            &owner,
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
        &[Check::err(PinocchioVaultError::AlreadyInitialized.into())],
    );
}

/// Checklist item 5.
#[test]
fn test_rejects_missing_signer() {
    let InitializeFixture {
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
    let InitializeFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = vec![9];

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(
            PinocchioVaultError::InvalidInstructionTag.into(),
        )],
    );
}

/// Distinct from an invalid tag: the tag (0) is valid, but `Initialize` takes
/// no arguments, so trailing bytes are malformed. Proves the tag check runs
/// before this one.
#[test]
fn test_rejects_deserialization_error() {
    let InitializeFixture {
        mollusk,
        accounts,
        mut instruction,
        ..
    } = default_fixture();
    instruction.data = vec![0, 1, 2, 3];

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::DeserializationError.into())],
    );
}

/// Checklist item 13 — mint account not owned by Token-2022 at all (e.g. a
/// legacy SPL Token mint or arbitrary account), checked before the extension
/// allow/deny enforcement.
#[test]
fn test_rejects_mint_not_owned_by_token_2022() {
    let InitializeFixture {
        mollusk,
        owner,
        mint,
        mut accounts,
        instruction,
        ..
    } = default_fixture();
    let rent = Rent::default();
    let mut bad_mint = extension_free_mint_account(&owner, &rent);
    bad_mint.owner = system_program::id();
    accounts[3] = (mint, bad_mint);

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::InvalidMint.into())],
    );
}

/// Checklist item 11 — token_program forged.
#[test]
fn test_rejects_forged_token_program() {
    let InitializeFixture {
        mollusk,
        mut accounts,
        mut instruction,
        ..
    } = default_fixture();
    let forged = Pubkey::new_unique();
    instruction.accounts[4].pubkey = forged;
    accounts[4] = (forged, Account::new(0, 0, &system_program::id()));

    mollusk.process_and_validate_instruction(
        &instruction,
        &accounts,
        &[Check::err(PinocchioVaultError::InvalidCpiTarget.into())],
    );
}

/// Checklist item 11 — system_program forged.
#[test]
fn test_rejects_forged_system_program() {
    let InitializeFixture {
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

/// Extension items — `docs/vault-design.md` §7 rejects all 7 named
/// extensions; `CloseAuthority` is folded into the blanket rule rather than
/// a separate test (see the design doc). One test per remaining extension,
/// each constructing a mint that carries that extension's real on-wire TLV
/// type ID (see `common::mint_with_extension`).
macro_rules! extension_rejection_test {
    ($test_name:ident, $extension:expr) => {
        #[test]
        fn $test_name() {
            let InitializeFixture {
                mollusk,
                owner,
                mint,
                mut accounts,
                instruction,
                ..
            } = default_fixture();
            let rent = Rent::default();
            accounts[3] = (mint, mint_with_extension(&owner, &rent, $extension));

            mollusk.process_and_validate_instruction(
                &instruction,
                &accounts,
                &[Check::err(
                    PinocchioVaultError::UnsupportedMintExtension.into(),
                )],
            );
        }
    };
}

extension_rejection_test!(
    test_rejects_transfer_fee_mint,
    ExtensionType::TransferFeeConfig
);
extension_rejection_test!(
    test_rejects_permanent_delegate_mint,
    ExtensionType::PermanentDelegate
);
extension_rejection_test!(test_rejects_transfer_hook_mint, ExtensionType::TransferHook);
extension_rejection_test!(
    test_rejects_default_frozen_mint,
    ExtensionType::DefaultAccountState
);
extension_rejection_test!(test_rejects_pausable_mint, ExtensionType::Pausable);
extension_rejection_test!(
    test_rejects_non_transferable_mint,
    ExtensionType::NonTransferable
);
