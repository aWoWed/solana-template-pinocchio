# Security Checklist

This is the source of truth for what "best practices" means for this template, made concrete
and testable. It derives directly from `docs/vault-design.md` §8-9. When the template is
complete, `programs/pinocchio-vault/` implements every item below, and every item has a
dedicated negative unit test asserting the **specific numbered error code** from
`vault-design.md` §9 — not merely "an error occurred."

> **Implementation status (2026-08-02, end of Step 6):** `Initialize`, `Deposit`, and
> `Withdraw` are all implemented in `programs/pinocchio-vault/src/processor.rs`. All 11 fixed
> checklist items and all 6 extension items are enforced and covered by dedicated
> `test_rejects_*` mollusk-svm tests in `clients/rust/tests/` (one canonical test per item,
> on whichever instruction naturally exercises it — see the per-test doc comments for the
> exact mapping). The extension table is still enforced as a single blanket rule (any
> initialized Token-2022 TLV entry is rejected, `token2022.rs`); the 6 per-extension tests
> each construct a mint carrying that *specific* extension's TLV entry and assert the same
> blanket check correctly identifies it — that is what makes them distinct tests rather than
> 6 copies of one test. The 3-item manual mutation-check subset is documented in
> `clients/rust/tests/MUTATION_CHECKS.md`.

Test naming convention (mollusk-svm unit tests):
`test_rejects_<condition>` — e.g. `test_rejects_missing_signer`.

## Checklist (fixed items — 11 total)

| # | Item | Validates against | Error code | Test name |
|---|---|---|---|---|
| 1 | Account-count guard | Instruction receives the exact expected number of accounts | 0 `AccountCountMismatch` | `test_rejects_wrong_account_count` |
| 2 | Duplicate/aliased-account check | No two account slots hold the same pubkey where they must differ (e.g. source == destination) | 1 `DuplicateAccount` | `test_rejects_duplicate_accounts` |
| 3 | Account-init-flag / re-initialization guard | `Initialize` is rejected if the vault state account is already initialized | 2 `AlreadyInitialized` | `test_rejects_double_initialize` |
| 4 | Owner check | Vault state account is owned by this program (Deposit/Withdraw only — Initialize creates it) | 3 `InvalidOwner` | `test_rejects_wrong_state_owner` |
| 5 | Signer-presence check | Required signer(s) are actually marked as signers | 4 `MissingRequiredSignature` | `test_rejects_missing_signer` |
| 6 | Instruction-tag check | `InstructionTag` byte is one of `0`/`1`/`2`, checked **before** deserializing the rest of instruction data | 5 `InvalidInstructionTag` | `test_rejects_invalid_instruction_tag` |
| 7 | Signer-identity check | `Withdraw` signer equals `vault.owner` | 7 `NotVaultOwner` | `test_rejects_non_owner_withdraw` |
| 8 | PDA re-derivation (stored bump) | Vault state PDA re-derived using `vault.bump`, not a caller-supplied bump | 8 `InvalidPda` | `test_rejects_forged_bump` |
| 9 | Token-account address re-derivation | Supplied token account equals the re-derived program-derived address (Decision 4) | 9 `TokenAccountMismatch` | `test_rejects_substituted_token_account` |
| 10 | Checked arithmetic | `Withdraw` amount does not exceed the vault token account's actual balance | 10 `InsufficientFunds` | `test_rejects_overdraw` |
| 11 | CPI target verification | Token-program account passed equals the hardcoded Token-2022 program ID, never attacker-supplied | 11 `InvalidCpiTarget` | `test_rejects_forged_cpi_target` |

## Checklist (extension items — count finalized by `vault-design.md` §7)

One dedicated negative test per **rejected** extension, asserting error code
6 `UnsupportedMintExtension`:

| Extension | Test name |
|---|---|
| `TransferFee` | `test_rejects_transfer_fee_mint` |
| `PermanentDelegate` | `test_rejects_permanent_delegate_mint` |
| `TransferHook` | `test_rejects_transfer_hook_mint` |
| `DefaultAccountState` (frozen) | `test_rejects_default_frozen_mint` |
| `Pausable` | `test_rejects_pausable_mint` |
| `NonTransferable` | `test_rejects_non_transferable_mint` |

= 6 extension items in the current `vault-design.md` §7 decision (all 7 named extensions are
rejected in v1; `CloseAuthority` is folded into mint-level extension rejection rather than a
separate test, since the vault never accepts a caller-supplied token account for it to apply
to — see §2/§7 of the design doc).

## Test Count Formula

`11 fixed items + 6 extension items = 17 dedicated negative tests minimum.`

The number is derived directly from this table, not asserted independently of it.

## Manual Mutation-Check Subset (3 items, not the full 17)

Blanket mutation-checking (verify every negative test fails when its guard is bypassed) does not
scale to 17 tests — mollusk tests require a full `build-sbf` cycle per mutant. Scoped instead to
the three highest-consequence checks, documented in `MUTATION_CHECKS.md`:

1. **Owner check** (item 4) — bypass it, confirm `test_rejects_wrong_state_owner` fails.
2. **Stored-bump CPI signer** (item 8, specifically the `Withdraw` CPI signing step) — sign
   with an arbitrary bump instead of `vault.bump`, confirm the corresponding test fails.
3. **Re-initialization guard** (item 3) — bypass it, confirm `test_rejects_double_initialize`
   fails.

All other 14 negative tests are enforced via their specific-error-code assertion alone —
a test that returns the *wrong* error code (e.g. fails for an unrelated account-not-found
reason rather than the check it claims to test) is itself a bug the error-code assertion
catches, at zero additional tooling cost.

## Validation Ordering

See `docs/vault-design.md` §8 for the full instruction-by-instruction ordering
(checks → effects → interactions) — this is itself part of what the template teaches, not an
implementation detail.

## Non-Checklist Test Coverage (still required, not part of the 17-count)

- Happy-path tests: `Initialize`, `Deposit`, `Withdraw` succeed under valid conditions.
- Boundary tests: zero-amount deposit/withdraw, max-u64 boundary.
- Malformed-data test: truncated/malformed instruction data after a *valid* `InstructionTag`
  must fail with `DeserializationError` (12) — distinct from an *invalid* tag failing with
  `InvalidInstructionTag` (5) before deserialization is even attempted. This is what proves
  the discriminator-before-deserialize ordering is real, not just documented.
- mollusk-svm integration-flow negatives (`clients/rust/tests/full_flows.rs`, beyond the unit
  list above): non-owner withdraw, over-withdraw, double-initialize, one rejected-extension
  mint — run against `fixtures/vault-vectors.json`.
