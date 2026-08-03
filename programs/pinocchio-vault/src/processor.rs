//! `Initialize` — implemented in the exact validation order of
//! `docs/vault-design.md` §8. The numbered comments below are that order; they
//! are part of what this template teaches, so keep them in sync with the doc.

use {
    crate::{
        error::VaultError,
        instruction::{
            DEPOSIT_ACCOUNT_COUNT, INITIALIZE_ACCOUNT_COUNT, TAG_DEPOSIT, TAG_INITIALIZE,
            TAG_WITHDRAW, WITHDRAW_ACCOUNT_COUNT,
        },
        state::{ACCOUNT_INIT_FLAG_UNINITIALIZED, VAULT_SEED, VAULT_TOKEN_SEED, VaultState},
        token2022::{
            BASE_TOKEN_ACCOUNT_LEN, TOKEN_2022_ID, assert_supported_mint, mint_decimals,
            token_account_amount,
        },
    },
    pinocchio::{
        AccountView, Address, ProgramResult,
        cpi::{Seed, Signer},
        sysvars::{Sysvar, rent::Rent},
    },
    pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer},
    // The `instructions::InitializeAccount3` alias pre-binds `TokenProgram`
    // (which accepts legacy SPL Token too); the generic form is needed to bind
    // `Token2022Program` instead. Same reasoning for `TransferChecked`.
    pinocchio_token::{
        TokenInterface,
        instructions::{
            initialize_account3::InitializeAccount3, transfer_checked::TransferChecked,
        },
    },
    solana_program_log::log,
};

/// SPL Token-2022, and *only* Token-2022.
///
/// `pinocchio_token::TokenProgram` deliberately accepts both SPL Token and
/// Token-2022; this vault's wire contract (§2) is Token-2022-only, so it uses
/// its own `TokenInterface` whose `verify` accepts nothing else.
struct Token2022Program;

impl TokenInterface for Token2022Program {
    const ID: Address = TOKEN_2022_ID;
}

pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    // Each handler still re-derives and checks its own account-count/tag, but
    // this dispatch must reject any tag outside {0,1,2} itself (§5): routing
    // an unrecognised tag through to `process_initialize` would hit its
    // `AccountInitFlag` guard (step 3, before the tag check at step 5) first
    // whenever `vault_state` is already initialized, misreporting
    // `AlreadyInitialized` instead of `InvalidInstructionTag`.
    match instruction_data.first() {
        Some(&TAG_DEPOSIT) => process_deposit(program_id, accounts, instruction_data),
        Some(&TAG_WITHDRAW) => process_withdraw(program_id, accounts, instruction_data),
        Some(&TAG_INITIALIZE) => process_initialize(program_id, accounts, instruction_data),
        _ => {
            log!("Unrecognised InstructionTag");
            Err(VaultError::InvalidInstructionTag.into())
        }
    }
}

fn process_initialize(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    // 1-2. Account-count guard + duplicate/aliased-account check.
    check_accounts(accounts, INITIALIZE_ACCOUNT_COUNT, "Initialize")?;

    // Steps 3-8a run against shared borrows; the state write in step 8b needs a
    // mutable borrow of `accounts[1]`, so it lives in its own scope below.
    let (owner_key, mint_key, state_key, state_bump, token_key, token_bump) = {
        let owner = &accounts[0];
        let vault_state = &accounts[1];
        let vault_token_account = &accounts[2];
        let mint = &accounts[3];
        let token_program = &accounts[4];
        let system_program = &accounts[5];

        // 3. `AccountInitFlag` guard — reject re-initialization before anything
        //    else.
        {
            let data = vault_state.try_borrow()?;
            if !data.is_empty() && data[0] != ACCOUNT_INIT_FLAG_UNINITIALIZED {
                log!("Vault state account is already initialized");
                return Err(VaultError::AlreadyInitialized.into());
            }
        }

        // 4. Signer-presence check.
        if !owner.is_signer() {
            log!("Vault owner must sign Initialize");
            return Err(VaultError::MissingRequiredSignature.into());
        }

        // 5. `InstructionTag` check — before any deserialization.
        let (&tag, args) = instruction_data
            .split_first()
            .ok_or(VaultError::InvalidInstructionTag)?;
        if tag != TAG_INITIALIZE {
            log!("Unrecognised InstructionTag");
            return Err(VaultError::InvalidInstructionTag.into());
        }

        // 6. Deserialize instruction args — `Initialize` takes none, so any
        //    trailing byte is malformed data (distinct code from an invalid tag).
        if !args.is_empty() {
            log!("Initialize takes no arguments");
            return Err(VaultError::DeserializationError.into());
        }

        // 7. Token-2022 mint extension allow/deny enforcement (§7).
        assert_supported_mint(mint)?;

        // The two CPI targets are verified before either is invoked.
        if token_program.address() != &TOKEN_2022_ID {
            log!("Token program account is not SPL Token-2022");
            return Err(VaultError::InvalidCpiTarget.into());
        }
        if system_program.address() != &pinocchio_system::ID {
            log!("System program account is not the System program");
            return Err(VaultError::InvalidCpiTarget.into());
        }

        // Canonical bumps are derived once, here, and stored; every later
        // `invoke_signed` uses the stored bump, never one from instruction data.
        let owner_key = *owner.address();
        let (expected_state, state_bump) =
            Address::derive_program_address(&[VAULT_SEED, owner_key.as_array()], program_id)
                .ok_or(VaultError::InvalidPda)?;
        if vault_state.address() != &expected_state {
            log!("Vault state account is not the canonical vault PDA");
            return Err(VaultError::InvalidPda.into());
        }
        let (expected_token, token_bump) = Address::derive_program_address(
            &[VAULT_TOKEN_SEED, expected_state.as_array()],
            program_id,
        )
        .ok_or(VaultError::InvalidPda)?;
        if vault_token_account.address() != &expected_token {
            log!("Vault token account is not the canonical vault token PDA");
            return Err(VaultError::TokenAccountMismatch.into());
        }

        // 8a. Create the state PDA with the pre-funding-safe pattern (§10).
        let bump_seed = [state_bump];
        let seeds = [
            Seed::from(VAULT_SEED),
            Seed::from(owner_key.as_array()),
            Seed::from(&bump_seed),
        ];
        create_pda_account(
            vault_state,
            owner,
            VaultState::LEN,
            program_id,
            &[Signer::from(&seeds)],
        )?;

        (
            owner_key,
            *mint.address(),
            expected_state,
            state_bump,
            expected_token,
            token_bump,
        )
    };

    // 8b. Write state (effects).
    {
        let vault_state = &mut accounts[1];
        let mut data = vault_state.try_borrow_mut()?;
        VaultState::from_bytes_mut(&mut data)?
            .write(&owner_key, &mint_key, &token_key, state_bump, token_bump);
    }

    // 9. CPI (interactions, last) — create the program-derived token account and
    //    hand its authority to the vault state PDA (§2), never to the owner.
    let owner = &accounts[0];
    let vault_token_account = &accounts[2];
    let mint = &accounts[3];
    let token_program = &accounts[4];

    let token_bump_seed = [token_bump];
    let token_seeds = [
        Seed::from(VAULT_TOKEN_SEED),
        Seed::from(state_key.as_array()),
        Seed::from(&token_bump_seed),
    ];
    create_pda_account(
        vault_token_account,
        owner,
        BASE_TOKEN_ACCOUNT_LEN,
        &TOKEN_2022_ID,
        &[Signer::from(&token_seeds)],
    )?;

    InitializeAccount3::<Token2022Program>::new(vault_token_account, mint, &state_key)
        .invoke_with_program(token_program.address())?;
    // `invoke_with_program` re-verifies the address against
    // `Token2022Program::ID` before the CPI — the check above is not redundant,
    // it is what makes the failure surface as `InvalidCpiTarget` (11) rather
    // than a generic `IncorrectProgramId`.

    log!("Vault initialized");
    Ok(())
}

/// `Deposit` — permissionless top-up (`docs/vault-design.md` §1: any signer
/// may deposit, only `Withdraw` is owner-restricted). Validation order
/// follows §8's Deposit/Withdraw template exactly; the numbered comments
/// below are that order.
fn process_deposit(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    // 1-2. Account-count guard + duplicate/aliased-account check.
    check_accounts(accounts, DEPOSIT_ACCOUNT_COUNT, "Deposit")?;

    let depositor = &accounts[0];
    let vault_state = &accounts[1];
    let vault_token_account = &accounts[2];
    let depositor_token_account = &accounts[3];
    let mint = &accounts[4];
    let token_program = &accounts[5];

    // 3. Owner check — `vault_state` must be owned by this program, checked
    //    before its data is trusted/deserialized at all.
    if !vault_state.owned_by(program_id) {
        log!("Vault state account is not owned by this program");
        return Err(VaultError::InvalidOwner.into());
    }

    // 4. Signer-presence check.
    if !depositor.is_signer() {
        log!("Depositor must sign Deposit");
        return Err(VaultError::MissingRequiredSignature.into());
    }

    // 5. `InstructionTag` check — before any deserialization.
    let (&tag, args) = instruction_data
        .split_first()
        .ok_or(VaultError::InvalidInstructionTag)?;
    if tag != TAG_DEPOSIT {
        log!("Unrecognised InstructionTag");
        return Err(VaultError::InvalidInstructionTag.into());
    }

    // 6. Deserialize instruction args (`amount: u64`).
    let amount_bytes: [u8; 8] = args
        .try_into()
        .map_err(|_| VaultError::DeserializationError)?;
    let amount = u64::from_le_bytes(amount_bytes);

    // 7. Deserialize account state — safe now that step 3 confirmed ownership.
    let (owner_address, state_bump, token_account_address) = {
        let state_data = vault_state.try_borrow()?;
        let state = VaultState::from_bytes(&state_data)?;
        (
            state.owner_address(),
            state.bump,
            state.token_account_address(),
        )
    };

    // 8. PDA re-derivation using the *stored* bump — never one supplied by
    //    the caller.
    let expected_state = Address::derive_address(
        &[VAULT_SEED, owner_address.as_array()],
        Some(state_bump),
        program_id,
    );
    if vault_state.address() != &expected_state {
        log!("Vault state account is not the canonical vault PDA");
        return Err(VaultError::InvalidPda.into());
    }

    // 9. Token-account address re-derivation — must equal the address stored
    //    at `Initialize`, never a caller-supplied lookup (§2).
    if vault_token_account.address() != &token_account_address {
        log!("Vault token account does not match the stored address");
        return Err(VaultError::TokenAccountMismatch.into());
    }

    // CPI target verification, before the CPI.
    if token_program.address() != &TOKEN_2022_ID {
        log!("Token program account is not SPL Token-2022");
        return Err(VaultError::InvalidCpiTarget.into());
    }

    // 10. Effects: none — the vault's balance lives in the token account
    //     itself, not a duplicated ledger in `VaultState` (§8). Deposit has
    //     no checked-arithmetic checklist item of its own (checklist item
    //     10 is Withdraw-specific); Token-2022's own `transfer_checked`
    //     already enforces the depositor has sufficient balance.

    // 11. CPI (interaction, last) — ordinary transfer, signed by the
    //     depositor (their own tokens, their own authority — not the vault).
    let decimals = mint_decimals(mint)?;
    // Turbofish on the struct (`TransferChecked::<Token2022Program>::new`)
    // would bind `Token2022Program` to the *first* generic slot
    // (`MultisigSigner`, not `Program`) since turbofish fills the struct's
    // own parameter list left-to-right; `new` only exists for
    // `MultisigSigner = &AccountView`. An explicit binding-type annotation
    // lets inference pick the right impl instead.
    let transfer: TransferChecked<'_, '_, &AccountView, Token2022Program> = TransferChecked::new(
        depositor_token_account,
        mint,
        vault_token_account,
        depositor,
        amount,
        decimals,
    );
    transfer.invoke_with_program(token_program.address())?;

    log!("Deposit complete");
    Ok(())
}

/// `Withdraw` — owner-only (`docs/vault-design.md` §1). Validation order
/// follows §8's Deposit/Withdraw template exactly; the numbered comments
/// below are that order.
fn process_withdraw(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    // 1-2. Account-count guard + duplicate/aliased-account check.
    check_accounts(accounts, WITHDRAW_ACCOUNT_COUNT, "Withdraw")?;

    let owner = &accounts[0];
    let vault_state = &accounts[1];
    let vault_token_account = &accounts[2];
    let destination_token_account = &accounts[3];
    let mint = &accounts[4];
    let token_program = &accounts[5];

    // 3. Owner check — `vault_state` must be owned by this program, checked
    //    before its data is trusted/deserialized at all.
    if !vault_state.owned_by(program_id) {
        log!("Vault state account is not owned by this program");
        return Err(VaultError::InvalidOwner.into());
    }

    // 4. Signer-presence check.
    if !owner.is_signer() {
        log!("Owner must sign Withdraw");
        return Err(VaultError::MissingRequiredSignature.into());
    }

    // 5. `InstructionTag` check — before any deserialization.
    let (&tag, args) = instruction_data
        .split_first()
        .ok_or(VaultError::InvalidInstructionTag)?;
    if tag != TAG_WITHDRAW {
        log!("Unrecognised InstructionTag");
        return Err(VaultError::InvalidInstructionTag.into());
    }

    // 6. Deserialize instruction args (`amount: u64`).
    let amount_bytes: [u8; 8] = args
        .try_into()
        .map_err(|_| VaultError::DeserializationError)?;
    let amount = u64::from_le_bytes(amount_bytes);

    // 7. Deserialize account state — safe now that step 3 confirmed ownership.
    let (owner_address, state_bump, token_account_address) = {
        let state_data = vault_state.try_borrow()?;
        let state = VaultState::from_bytes(&state_data)?;
        (
            state.owner_address(),
            state.bump,
            state.token_account_address(),
        )
    };

    // 7b. Signer-identity check — `Withdraw` only (checklist item 7): the
    //     signer must be the vault's actual owner, not merely any signer.
    if owner.address() != &owner_address {
        log!("Signer is not the vault owner");
        return Err(VaultError::NotVaultOwner.into());
    }

    // 8. PDA re-derivation using the *stored* bump — never one supplied by
    //    the caller. This bump also becomes the `invoke_signed` seed below,
    //    so a forged bump cannot produce a valid signature either.
    let bump_seed = [state_bump];
    let state_seeds = [
        Seed::from(VAULT_SEED),
        Seed::from(owner_address.as_array()),
        Seed::from(&bump_seed),
    ];
    let expected_state = Address::derive_address(
        &[VAULT_SEED, owner_address.as_array()],
        Some(state_bump),
        program_id,
    );
    if vault_state.address() != &expected_state {
        log!("Vault state account is not the canonical vault PDA");
        return Err(VaultError::InvalidPda.into());
    }

    // 9. Token-account address re-derivation — must equal the address stored
    //    at `Initialize`, never a caller-supplied lookup (§2).
    if vault_token_account.address() != &token_account_address {
        log!("Vault token account does not match the stored address");
        return Err(VaultError::TokenAccountMismatch.into());
    }

    // CPI target verification, before the CPI.
    if token_program.address() != &TOKEN_2022_ID {
        log!("Token program account is not SPL Token-2022");
        return Err(VaultError::InvalidCpiTarget.into());
    }

    // 10. Checked arithmetic (checklist item 10): the withdrawal must not
    //     exceed the vault token account's real, on-chain balance — the
    //     single source of truth, not a duplicated ledger.
    let available = token_account_amount(vault_token_account)?;
    if amount > available {
        log!("Withdraw amount exceeds vault balance");
        return Err(VaultError::InsufficientFunds.into());
    }

    // 11. Effects: none, same as `Deposit` — balance lives in the token
    //     account itself.

    // 12. CPI (interaction, last) — signed by the vault state PDA via
    //     `invoke_signed`, using the *stored* bump, never the owner's own
    //     signature (the owner is never the token account's authority — §2).
    let decimals = mint_decimals(mint)?;
    let transfer: TransferChecked<'_, '_, &AccountView, Token2022Program> = TransferChecked::new(
        vault_token_account,
        mint,
        destination_token_account,
        vault_state,
        amount,
        decimals,
    );
    transfer.invoke_signed_with_program(&[Signer::from(&state_seeds)], token_program.address())?;

    log!("Withdraw complete");
    Ok(())
}

/// §8 steps 1-2, shared verbatim by all three instructions: reject the wrong
/// number of accounts, then reject any two account slots holding the same
/// address. `name` is only used for the count-mismatch log line, so `solana
/// logs` still says which instruction rejected the call.
fn check_accounts(
    accounts: &[AccountView],
    expected_count: usize,
    name: &str,
) -> Result<(), VaultError> {
    if accounts.len() != expected_count {
        log!("{} expects exactly {} accounts", name, expected_count);
        return Err(VaultError::AccountCountMismatch);
    }

    for i in 0..expected_count {
        for j in (i + 1)..expected_count {
            if accounts[i].address() == accounts[j].address() {
                log!("Duplicate account supplied");
                return Err(VaultError::DuplicateAccount);
            }
        }
    }

    Ok(())
}

/// `docs/vault-design.md` §10 — pre-funding-safe PDA creation.
///
/// A naive `CreateAccount` fails outright once the target address holds any
/// lamports, and a PDA address is computable by anyone, so a single stray
/// lamport would permanently block `initialize`.
fn create_pda_account(
    target: &AccountView,
    payer: &AccountView,
    space: usize,
    new_owner: &Address,
    signers: &[Signer],
) -> ProgramResult {
    let required_lamports = Rent::get()?.try_minimum_balance(space)?;

    if target.lamports() == 0 {
        return CreateAccount {
            from: payer,
            to: target,
            lamports: required_lamports,
            space: space as u64,
            owner: new_owner,
        }
        .invoke_signed(signers);
    }

    let shortfall = required_lamports.saturating_sub(target.lamports());
    if shortfall > 0 {
        Transfer {
            from: payer,
            to: target,
            lamports: shortfall,
        }
        .invoke()?;
    }
    Allocate {
        account: target,
        space: space as u64,
    }
    .invoke_signed(signers)?;
    Assign {
        account: target,
        owner: new_owner,
    }
    .invoke_signed(signers)
}
