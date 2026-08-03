# Manual Mutation Checks

Per `docs/security-checklist.md`, blanket mutation-checking across all checklist-derived
negative tests does not scale (each mollusk test run needs a full `cargo build-sbf` cycle per
mutant). Scoped instead to the 3 highest-consequence checks. For each: comment out the guard in
`programs/pinocchio-vault/src/processor.rs`, run the named test, confirm it now **fails**
(proving the test genuinely exercises that guard rather than passing for an unrelated reason),
then revert the change.

| # | Check | Where | Bypass | Confirm-fails test | Run |
|---|---|---|---|---|---|
| 1 | Owner check | `process_deposit`/`process_withdraw`, step 3 (`vault_state.owned_by(program_id)`) | Comment out the `if !vault_state.owned_by(program_id) { ... }` block in one handler | `test_rejects_wrong_state_owner` (in that instruction's test file) | `cargo test -p pinocchio-vault-client --features test-sbf test_rejects_wrong_state_owner` |
| 2 | Stored-bump CPI signer | `process_withdraw`, the `invoke_signed_with_program` call using `state_seeds` | Replace `state_bump` (read from `vault_state`) with a hardcoded/arbitrary bump byte before building `bump_seed`/`state_seeds` | `withdraw_transfers_out_of_the_vault_token_account` should fail (the CPI's derived signer no longer matches the vault token account's real authority PDA, so the token-program CPI itself rejects it) | `cargo test -p pinocchio-vault-client --features test-sbf withdraw_transfers_out_of_the_vault_token_account` |
| 3 | Re-initialization guard | `process_initialize`, step 3 (`AccountInitFlag` check) | Comment out the `if !data.is_empty() && data[0] != ACCOUNT_INIT_FLAG_UNINITIALIZED { ... }` block | `test_rejects_double_initialize` (and `full_flow_double_initialize_fails`) | `cargo test -p pinocchio-vault-client --features test-sbf test_rejects_double_initialize full_flow_double_initialize_fails` |

## Status

**Not yet executed in-session (2026-08-02).** This session's local Windows environment could not
run `cargo build-sbf`/`cargo test-sbf` at all (host-side build-script compilation needs the MSVC
linker; only a GNU toolchain was installable non-interactively here -- see the root README's
Windows build note). The procedure above is fully specified and ready to run; it needs to be
executed once against a working `cargo build-sbf` environment (CI, or a local machine with Visual
Studio Build Tools installed) before this checklist item can be marked verified rather than
merely specified.
