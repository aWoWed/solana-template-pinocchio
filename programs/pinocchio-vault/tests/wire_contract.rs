//! Pins the frozen wire contract in `docs/vault-design.md` to the actual Rust
//! types, and thereby to `programs/pinocchio-vault/idl.json`, which is
//! hand-authored rather than generated (see `docs/idl-generation-notes.md`).
//!
//! If the IDL and the program ever disagree about state layout or error codes,
//! these assertions are what catches it.

use {
    core::{mem::offset_of, str::FromStr},
    pinocchio::{Address, error::ProgramError},
    pinocchio_vault::{
        error::VaultError,
        instruction::{
            DEPOSIT_ACCOUNT_COUNT, INITIALIZE_ACCOUNT_COUNT, TAG_DEPOSIT, TAG_INITIALIZE,
            TAG_WITHDRAW, WITHDRAW_ACCOUNT_COUNT,
        },
        state::{
            ACCOUNT_INIT_FLAG_INITIALIZED, ACCOUNT_INIT_FLAG_UNINITIALIZED, VAULT_SEED,
            VAULT_TOKEN_SEED, VaultState,
        },
        token2022::{BASE_MINT_LEN, BASE_TOKEN_ACCOUNT_LEN, TOKEN_2022_ID},
    },
};

/// `docs/vault-design.md` §4 — byte-for-byte.
#[test]
fn vault_state_layout_matches_design_section_4() {
    assert_eq!(VaultState::LEN, 107);
    assert_eq!(offset_of!(VaultState, account_init_flag), 0);
    assert_eq!(offset_of!(VaultState, owner), 1);
    assert_eq!(offset_of!(VaultState, mint), 33);
    assert_eq!(offset_of!(VaultState, token_account), 65);
    assert_eq!(offset_of!(VaultState, bump), 97);
    assert_eq!(offset_of!(VaultState, token_account_bump), 98);
    assert_eq!(offset_of!(VaultState, reserved), 99);
    // Alignment 1 is what makes the zero-copy reinterpret in `from_bytes_mut`
    // sound for any byte slice, and what guarantees no interior padding.
    assert_eq!(core::mem::align_of::<VaultState>(), 1);
}

#[test]
fn from_bytes_mut_rejects_wrong_length() {
    let mut short = [0u8; VaultState::LEN - 1];
    let mut long = [0u8; VaultState::LEN + 1];
    assert!(matches!(
        VaultState::from_bytes_mut(&mut short),
        Err(VaultError::DeserializationError)
    ));
    assert!(matches!(
        VaultState::from_bytes_mut(&mut long),
        Err(VaultError::DeserializationError)
    ));
}

/// `write` must land every field at the offsets asserted above, and must leave
/// `account_init_flag` set last (§8 step 8b).
#[test]
fn write_produces_the_documented_byte_layout() {
    let owner = Address::from([1u8; 32]);
    let mint = Address::from([2u8; 32]);
    let token_account = Address::from([3u8; 32]);

    let mut bytes = [0u8; VaultState::LEN];
    VaultState::from_bytes_mut(&mut bytes)
        .unwrap()
        .write(&owner, &mint, &token_account, 254, 253);

    assert_eq!(bytes[0], ACCOUNT_INIT_FLAG_INITIALIZED);
    assert_eq!(&bytes[1..33], owner.as_array());
    assert_eq!(&bytes[33..65], mint.as_array());
    assert_eq!(&bytes[65..97], token_account.as_array());
    assert_eq!(bytes[97], 254);
    assert_eq!(bytes[98], 253);
    assert_eq!(&bytes[99..107], &[0u8; 8]);
}

/// `docs/vault-design.md` §9 — all 14 codes (0-13). The discriminant *is* the
/// numbered error code, so this also pins `idl.json`'s `errors` array.
#[test]
fn error_codes_match_design_section_9() {
    let expected: [(VaultError, u32); 14] = [
        (VaultError::AccountCountMismatch, 0),
        (VaultError::DuplicateAccount, 1),
        (VaultError::AlreadyInitialized, 2),
        (VaultError::InvalidOwner, 3),
        (VaultError::MissingRequiredSignature, 4),
        (VaultError::InvalidInstructionTag, 5),
        (VaultError::UnsupportedMintExtension, 6),
        (VaultError::NotVaultOwner, 7),
        (VaultError::InvalidPda, 8),
        (VaultError::TokenAccountMismatch, 9),
        (VaultError::InsufficientFunds, 10),
        (VaultError::InvalidCpiTarget, 11),
        (VaultError::DeserializationError, 12),
        (VaultError::InvalidMint, 13),
    ];

    for (variant, code) in expected {
        assert_eq!(variant as u32, code);
        assert_eq!(ProgramError::from(variant), ProgramError::Custom(code));
    }
}

/// `docs/vault-design.md` §3, §5, §7 — the remaining frozen constants.
#[test]
fn constants_match_the_design_doc() {
    assert_eq!(VAULT_SEED, b"vault");
    assert_eq!(VAULT_TOKEN_SEED, b"vault_token");
    assert_eq!(TAG_INITIALIZE, 0);
    assert_eq!(TAG_DEPOSIT, 1);
    assert_eq!(TAG_WITHDRAW, 2);
    assert_eq!(INITIALIZE_ACCOUNT_COUNT, 6);
    assert_eq!(DEPOSIT_ACCOUNT_COUNT, 6);
    assert_eq!(WITHDRAW_ACCOUNT_COUNT, 6);
    assert_eq!(ACCOUNT_INIT_FLAG_UNINITIALIZED, 0);
    assert_eq!(ACCOUNT_INIT_FLAG_INITIALIZED, 1);
    assert_eq!(BASE_MINT_LEN, 82);
    assert_eq!(BASE_TOKEN_ACCOUNT_LEN, 165);
    assert_eq!(
        TOKEN_2022_ID,
        Address::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
    );
}

/// The IDL is hand-authored, so nothing generates it from these types — this
/// test is the link that keeps the two honest.
#[test]
fn idl_json_matches_the_program() {
    let idl: serde_json::Value =
        serde_json::from_str(include_str!("../idl.json")).expect("idl.json is valid JSON");

    let idl_address = Address::from_str(idl["metadata"]["address"].as_str().unwrap()).unwrap();
    assert_eq!(idl_address, pinocchio_vault::ID);

    let instructions = idl["instructions"].as_array().unwrap();
    assert_eq!(instructions.len(), 3);

    let expected = [
        ("Initialize", TAG_INITIALIZE, INITIALIZE_ACCOUNT_COUNT),
        ("Deposit", TAG_DEPOSIT, DEPOSIT_ACCOUNT_COUNT),
        ("Withdraw", TAG_WITHDRAW, WITHDRAW_ACCOUNT_COUNT),
    ];
    for (index, (name, tag, account_count)) in expected.iter().enumerate() {
        assert_eq!(instructions[index]["name"], *name);
        assert_eq!(
            instructions[index]["discriminant"]["value"],
            u64::from(*tag)
        );
        assert_eq!(
            instructions[index]["accounts"].as_array().unwrap().len(),
            *account_count
        );
    }

    let errors = idl["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 14);
    for (index, error) in errors.iter().enumerate() {
        assert_eq!(error["code"], index as u64);
    }
}

/// `fixtures/vault-vectors.json` is the shared source of truth for which
/// *scenarios* the Rust and JS/TS test suites must both cover (Step 4 of the
/// consensus plan). This test is what keeps that file from drifting against
/// the actual wire contract: every error name/code it references must be a
/// real, correctly-numbered `VaultError` variant, and `Initialize`'s tag/
/// account-count must match `idl.json` (already pinned to the program by
/// `idl_json_matches_the_program` above).
#[test]
fn conformance_vectors_match_idl_and_error_table() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/vault-vectors.json"))
            .expect("fixtures/vault-vectors.json is valid JSON");

    let expected_instructions = [
        ("Initialize", TAG_INITIALIZE, INITIALIZE_ACCOUNT_COUNT),
        ("Deposit", TAG_DEPOSIT, DEPOSIT_ACCOUNT_COUNT),
        ("Withdraw", TAG_WITHDRAW, WITHDRAW_ACCOUNT_COUNT),
    ];
    for (name, tag, account_count) in expected_instructions {
        let entry = &vectors["instructions"][name];
        assert_eq!(entry["tag"], u64::from(tag), "{name} tag mismatch");
        assert_eq!(
            entry["accountCount"], account_count as u64,
            "{name} accountCount mismatch"
        );
    }

    let known_error_codes: [(u32, &str); 14] = [
        (0, "AccountCountMismatch"),
        (1, "DuplicateAccount"),
        (2, "AlreadyInitialized"),
        (3, "InvalidOwner"),
        (4, "MissingRequiredSignature"),
        (5, "InvalidInstructionTag"),
        (6, "UnsupportedMintExtension"),
        (7, "NotVaultOwner"),
        (8, "InvalidPda"),
        (9, "TokenAccountMismatch"),
        (10, "InsufficientFunds"),
        (11, "InvalidCpiTarget"),
        (12, "DeserializationError"),
        (13, "InvalidMint"),
    ];

    for (name, _tag, _account_count) in expected_instructions {
        let scenarios = vectors["instructions"][name]["scenarios"]
            .as_array()
            .unwrap_or_else(|| panic!("vault-vectors.json declares no scenarios for {name}"));
        assert!(
            !scenarios.is_empty(),
            "vault-vectors.json declares no scenarios for {name}"
        );

        let mut saw_success = false;
        for scenario in scenarios {
            let expect = &scenario["expect"];
            match expect["result"].as_str().unwrap() {
                "success" => saw_success = true,
                "error" => {
                    let code = expect["errorCode"].as_u64().unwrap() as u32;
                    let error_name = expect["errorName"].as_str().unwrap();
                    let matched = known_error_codes.iter().any(|&(known_code, known_name)| {
                        known_code == code && known_name == error_name
                    });
                    assert!(
                        matched,
                        "{name} scenario {:?} references errorCode {} / errorName {:?}, which \
                         does not match any entry in docs/vault-design.md §9's error table",
                        scenario["name"], code, error_name
                    );
                }
                other => panic!(
                    "{name} scenario {:?} has unknown result kind {:?}",
                    scenario["name"], other
                ),
            }
        }
        assert!(
            saw_success,
            "vault-vectors.json's {name} scenarios contain no happy-path case"
        );
    }
}
