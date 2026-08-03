# Vault Design — Wire Contract

This document is the frozen, byte-level contract for `programs/pinocchio-vault/`. It is
validated (not merely assumed) by the Step 3 tracer-bullet slice before being treated as
frozen — see the plan's Decision 3/4 sequencing note.

Source consensus plan: `.omc/plans/ralplan-create-solana-template-smart-contracts.md` (pass 3).

## 1. Authorization Model — Decision 1

**Single-owner vault.** The vault is created and controlled by exactly one owner.

- `initialize`: caller becomes `vault.owner`. No one else may ever change it.
- `deposit`: **any** signer may deposit into the vault (permissionless top-up).
- `withdraw`: requires `signer == vault.owner`. Any other signer is rejected.

Rejected alternative: multi-depositor pool (share accounting). Out of scope — see ADR in
the consensus plan.

## 2. Token-Account Topology — Decision 4

**Program-derived.** The vault's Token-2022 token account is created *by the program*,
inside `initialize`, at a deterministic address — never supplied by the caller.

- Token account address: Program Derived Address (PDA) with seeds
  `[b"vault_token", vault_state_pda.as_ref()]` — i.e. it is derived from the vault state
  PDA's own address, not from the owner directly. This keeps a 1:1, unambiguous mapping
  from vault state → vault token account that can be re-derived from on-chain state alone.
- Token-account authority: the vault **state PDA** (not the human owner). All CPIs moving
  tokens out of this account are signed by the program via `invoke_signed` using the vault
  state PDA's stored canonical bump — never the owner's own signature, since the owner is
  never the token account's authority.
- This closes the token-account/mint substitution attack class (pre-mortem #3) by
  construction: every instruction re-derives this address from the vault state PDA and
  compares it byte-for-byte; there is no caller-supplied token account to substitute.

## 3. PDA Seed Scheme

| Account | Seeds | Bump storage |
|---|---|---|
| Vault state PDA | `[b"vault", owner_pubkey.as_ref()]` | Canonical bump computed once in `initialize` via `derive_program_address`, stored in `VaultState.bump`. All later signing uses this **stored** bump — never a bump passed in instruction data (pre-mortem #2). |
| Vault token account PDA | `[b"vault_token", vault_state_pda.as_ref()]` | Canonical bump computed once in `initialize`, stored in `VaultState.token_account_bump`. |

Both PDAs are derived from `program_id` implicitly (standard `derive_program_address` behavior).

## 4. State Layout

Fixed-width little-endian scalar fields only — no `Vec`, `Option`, or `String` — so the account's
raw bytes can be reinterpreted directly as `VaultState` (zero-copy), with no serialization step
and no risk of interior padding.

```
VaultState (packed, little-endian, total 107 bytes):
  offset  0..1    account_init_flag : u8       // 0 = uninitialized, 1 = initialized (checked FIRST, before any other logic)
  offset  1..33   owner              : [u8; 32] // Pubkey
  offset 33..65   mint               : [u8; 32] // Pubkey (Token-2022 mint this vault holds)
  offset 65..97   token_account      : [u8; 32] // Pubkey (the program-derived token account address, cached for cheap comparison)
  offset 97..98   bump               : u8       // canonical bump for the vault state PDA itself
  offset 98..99   token_account_bump : u8       // canonical bump for the vault token account PDA
  offset 99..107  reserved           : [u8; 8]  // reserved for future fields; must be zeroed at init
```

`account_init_flag` doubles as this account's "discriminator" in the account-level sense
(distinct from the *instruction*-level discriminator below — see naming note in §6).

## 5. Instruction Set & Discriminators

1-byte instruction tag (`InstructionTag`), first byte of instruction data:

| Tag value | Instruction | Args (Borsh-encoded after the tag byte) |
|---|---|---|
| `0` | `Initialize` | *(none — owner is `accounts[0]`, mint is `accounts[N]`)* |
| `1` | `Deposit` | `amount: u64` (little-endian) |
| `2` | `Withdraw` | `amount: u64` (little-endian) |

No other tag values are valid; an unrecognized tag is `ErrorCode::InvalidInstructionTag` (5).

## 6. Naming Note — Two Different "Discriminators"

These are named distinctly, and code/comments MUST use these exact names to avoid confusing them:

- **`InstructionTag`** (§5 above): the first byte of instruction *data*, identifying which
  instruction is being called. Checked before Borsh-deserializing the remaining args.
- **`AccountInitFlag`** (§4 above): a field *inside* the vault state account, identifying
  whether that account has already been initialized. Checked at the very start of
  `Initialize` (position 3 in the validation order, §8) — before any other logic — to
  reject re-initialization (pre-mortem #5).

## 7. Token-2022 Extension Allow/Deny Decision

Default posture: **reject at `initialize`** any mint carrying an extension not explicitly
handled below. This is the safe default for a teaching template — an unhandled extension is
a silent vulnerability, not a missing feature.

| Extension | Posture | Rationale |
|---|---|---|
| `TransferFee` | **Rejected** (v1 scope) | Amount sent ≠ amount received; would require `transfer_checked_with_fee`-aware accounting throughout deposit/withdraw. Out of scope for v1; documented as a known future extension in the README. |
| `PermanentDelegate` | **Rejected** | The delegate could move tokens out of the vault's token account with zero vault involvement, silently invalidating `VaultState`'s implied balance guarantee. No mitigation possible from the vault program's side — must reject the mint outright. |
| `TransferHook` | **Rejected** (v1 scope) | Requires resolving extra accounts via `ExtraAccountMetaList` at every transfer; Shank cannot express variadic remaining-accounts in a static IDL. Out of scope for v1. |
| `DefaultAccountState` (frozen-by-default) | **Rejected** | Deposits would succeed but withdrawals could brick if the account defaults to frozen. |
| `Pausable` | **Rejected** | Same class of risk as `DefaultAccountState` — an external actor can halt vault operations. |
| `NonTransferable` | **Rejected** | A vault that can accept a token but never move it out is a trap, not a vault. |
| Token-account `CloseAuthority` | **Rejected** on the mint's default token-account state | The vault's own program-derived token account is created by the program itself (§2), so this only matters if the *mint* forces a close authority other than the vault program on newly created accounts — reject any such mint. |

Each rejected extension gets one dedicated negative test, asserting
`ErrorCode::UnsupportedMintExtension` (see error table, §9).

## 8. Validation Ordering (per instruction)

**`Initialize`:**
1. Account-count guard (reject if the account slice doesn't have the exact expected length)
2. Duplicate/aliased-account check (no two account slots may hold the same pubkey)
3. **`AccountInitFlag` guard** — reject if the vault state account is already initialized (pre-mortem #5). This is first, not last — the pass-2 defect this document exists to fix.
4. Signer-presence check (owner-to-be must be a signer)
5. `InstructionTag` check (must be `0`)
6. Deserialize instruction args *(operation, not a checklist item — no args for `Initialize`)*
7. Token-2022 mint extension allow/deny enforcement (§7) — reject unsupported extensions before creating anything
8. **Write state**: populate `VaultState` (owner, mint, token_account, bumps), using the robust PDA-creation pattern (§10)
9. **CPI** (interaction, last): create + initialize the program-derived Token-2022 token account (§2), signed via `invoke_signed` with the vault state PDA's freshly-derived bump (not yet "stored" at this exact sub-step, since this is the step storing it — bump is computed once here and used both for storage and for this CPI's signer seeds)

**`Deposit` / `Withdraw`:**
1. Account-count guard
2. Duplicate/aliased-account check
3. Owner check — vault state account must be owned by this program
4. Signer-presence check
5. `InstructionTag` check (must be `1` or `2`)
6. Deserialize instruction args (`amount: u64`)
7. Signer-identity check — **`Withdraw` only**: `signer == vault.owner` (pre-mortem #1)
8. PDA re-derivation of the vault state PDA using the **stored** bump (`vault.bump`) — never a bump from instruction data (pre-mortem #2)
9. Token-account address re-derivation: recompute the vault token account PDA from the vault state PDA and compare against `vault.token_account` — this is the mint/token-account relationship check, now a re-derivation rather than a lookup, per Decision 4
10. Checked arithmetic — `checked_add`/`checked_sub` on the conceptual balance; the actual balance is the token account's real balance, so this is really a check that the *requested amount* doesn't over/underflow the *token account's actual balance*, i.e. `withdraw` fails if `amount > token_account.amount`
11. **Write state** (effects) — no mutable `VaultState` fields change on deposit/withdraw in this v1 design (balance lives in the token account itself, not duplicated in `VaultState`), so this step is a no-op by design; documented here so the checks→effects→interactions rule is explicit even when effects is empty
12. **CPI** (interaction, last): `transfer_checked` to/from the vault's program-derived token account. `Deposit` is signed by the depositing user (ordinary CPI). `Withdraw` is signed by the **program**, via `invoke_signed` with the vault state PDA's **stored** bump as the signer seeds — this is the CPI-target-verification checklist item: the token program ID invoked must equal the expected, hardcoded Token-2022 program ID, never an attacker-supplied program ID from the account list.

## 9. Error Codes

One distinct numbered code per checklist item, so tests can assert *which* check fired,
not merely that *some* error occurred (required for the error-code-assertion test strategy —
see consensus plan Principle 2).

| Code | Name | Fires when |
|---|---|---|
| 0 | `AccountCountMismatch` | Wrong number of accounts passed |
| 1 | `DuplicateAccount` | Two account slots hold the same pubkey where they must differ |
| 2 | `AlreadyInitialized` | `Initialize` called on an account with `account_init_flag == 1` |
| 3 | `InvalidOwner` | Vault state account not owned by this program |
| 4 | `MissingRequiredSignature` | A required signer is not marked as a signer |
| 5 | `InvalidInstructionTag` | Instruction tag byte is not `0`, `1`, or `2` |
| 6 | `UnsupportedMintExtension` | Mint carries an extension not in the accepted set (§7) |
| 7 | `NotVaultOwner` | `Withdraw` signer does not match `vault.owner` |
| 8 | `InvalidPda` | Vault state PDA re-derivation using the stored bump does not match the supplied account |
| 9 | `TokenAccountMismatch` | Supplied token account does not match the re-derived program-derived address |
| 10 | `InsufficientFunds` | `Withdraw` amount exceeds the vault token account's actual balance (checked-arithmetic failure) |
| 11 | `InvalidCpiTarget` | The token-program account passed does not match the expected, hardcoded Token-2022 program ID |
| 12 | `DeserializationError` | Instruction data present but malformed for the given `InstructionTag` |
| 13 | `InvalidMint` | Supplied mint account is not a genuine Token-2022 mint (wrong owner program / not a mint at all), checked before the extension allow/deny enforcement in §7 |

> **Implementation note (2026-08-01):** code 13 was added during Step 3 implementation —
> the original 13-code table didn't distinguish "not a Token-2022 mint at all" from
> "is a Token-2022 mint but carries an unsupported extension" (§7). Both are real, distinct
> failure modes worth a distinct error code, so the table was extended rather than the
> code trimmed to match a stale doc. This is the kind of doc↔code drift Step 12's final
> re-audit step exists to catch — noted here explicitly instead of silently diverging.

A custom error enum implementing `Into<ProgramError>` (or `From`/`#[from]` via `thiserror`)
is required, mapping 1:1 to this table.

## 10. Robust PDA Account-Creation Pattern

Both the vault state PDA and the vault token account PDA must be created using this pattern,
**not** a naive `invoke(system_instruction::create_account(...))`, which fails outright if the
target address already holds any lamports (a trivial griefing/DoS vector — anyone can send
1 lamport to a computable PDA address before `initialize` runs):

1. Check the account's current lamport balance.
2. If it is less than the rent-exempt minimum for the target size, `invoke` a `system_program::transfer` (from the payer) for the shortfall.
3. `invoke_signed` `system_program::allocate` (set the account's data size) with the PDA's seeds.
4. `invoke_signed` `system_program::assign` (set the account's owner) with the PDA's seeds.
5. Only then write the account's data.

Steps 3-4 are combined into a single `create_account`-equivalent call ONLY when step 1 finds a
zero balance; otherwise steps 2-4 run as separate instructions as described.

## 11a. Scope Note — Integration Test Harness (2026-08-02)

The consensus plan named LiteSVM for multi-instruction integration flows
(Step 7). `litesvm 0.15.0` was evaluated and found to not compile against
this workspace's dependency graph: its own internal use of
`solana-loader-v3-interface` (which pulls `wincode 0.5.x`) against its own
`Serialize`/`SchemaWrite` trait bounds (`wincode 0.6.x`, pulled transitively
via `solana-address 2.7`) fails with 27 trait-bound errors — the same
upstream wincode-0.5/0.6 split already documented in `clients/rust/Cargo.toml`
as the reason `solana-program-test` was rejected in favor of mollusk-svm.
This is a `litesvm` bug against the current Agave 4.1.2 dependency graph, not
fixable from this workspace. `Mollusk::process_instruction_chain` (persisted
account state across successive instructions in one VM invocation) covers the
same need — see `clients/rust/tests/full_flows.rs`.

## 11. Open Follow-Ups

- SBPF target version and the granular `solana-*` crate import surface are verified live
  against the actually-installed toolchain, not assumed from this document's authoring time.
