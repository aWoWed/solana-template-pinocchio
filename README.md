# solana-template — Pinocchio Token-2022 vault

A native-Rust (no-Anchor) Solana program template: a single-owner Token-2022 vault built on
[Pinocchio](https://github.com/anza-xyz/pinocchio) (`no_std`, zero-copy account access, no
macro-magic account validation), plus generated Rust and JavaScript/TypeScript clients.

Everything in this template is built and tested against a written, checked-in security
checklist rather than "best effort" conventions — see [Design & security](#design--security)
below.

## Scope

- **Program:** a single Pinocchio program, `programs/pinocchio-vault/`.
- **Authorization model:** single-owner. `initialize` sets the caller as `vault.owner`
  permanently; `deposit` is a permissionless top-up (any signer); `withdraw` requires
  `signer == vault.owner`.
- **Token-account topology:** program-derived. The vault's Token-2022 token account is created
  *by the program* inside `initialize`, at a deterministic PDA — never supplied by the caller.
  This closes the token-account/mint substitution attack class by construction (every
  instruction re-derives the address and compares it, rather than trusting a caller-supplied
  account).
- **Token-2022 extensions:** default-reject. No extension is accepted in v1 — any mint carrying
  an initialized extension TLV entry is rejected at `initialize`. See
  [`docs/vault-design.md` §7](docs/vault-design.md) for the per-extension rationale (`TransferFee`,
  `PermanentDelegate`, `TransferHook`, `DefaultAccountState`, `Pausable`, `NonTransferable`,
  `CloseAuthority`).
- **No Anchor anywhere.** `! cargo tree --workspace --edges all | grep -qi anchor` must exit 0 —
  enforced in CI.

## Repository layout

```
programs/pinocchio-vault/   The on-chain program (Pinocchio, no_std)
clients/rust/                Generated Rust client + mollusk-svm test suite
clients/js/                  Generated TypeScript client (@solana/kit) + ava e2e tests
fixtures/vault-vectors.json  Shared scenario fixtures (Rust and JS/TS test suites both cover these)
docs/vault-design.md         Frozen wire contract (accounts, seeds, state layout, error codes)
docs/security-checklist.md   Checklist -> dedicated negative test mapping
docs/idl-generation-notes.md Why the IDL is hand-authored instead of Shank-generated
.semgrep/solana-vault.yml    Checked-in security-scanner ruleset (CI fallback gate)
```

## Design & security

- [`docs/vault-design.md`](docs/vault-design.md) — the frozen, byte-level wire contract: PDA
  seeds, state layout, instruction discriminators, error codes, the Token-2022 extension
  allow/deny table, validation ordering, and the pre-funding-safe PDA-creation pattern.
- [`docs/security-checklist.md`](docs/security-checklist.md) — 11 fixed checklist items + 6
  Token-2022 extension-rejection items, each mapped to a dedicated negative test asserting its
  *specific* numbered error code (not merely "an error occurred"). Also documents the 3-item
  manual mutation-check subset (owner check, stored-bump CPI signer, re-initialization guard).

## Test pyramid

| Layer | Where | What |
|---|---|---|
| Wire-contract unit tests | `programs/pinocchio-vault/tests/wire_contract.rs` | Byte-offset/state-layout assertions, error-code table, IDL<->code<->fixtures cross-check. Host-only, no BPF build needed. |
| Overflow-checks behavioral test | `programs/pinocchio-vault/tests/overflow_checks.rs` | Proves `overflow-checks = true` (root `Cargo.toml` `[profile.release]`) actually reaches the built artifact — only meaningful under `cargo test --release` (see the file's module doc for why a debug run doesn't prove anything). |
| mollusk-svm unit tests | `clients/rust/tests/{initialize,deposit,withdraw}.rs` | One dedicated negative test per checklist item (specific numbered error code), plus happy-path and boundary (zero-amount, max-u64) tests. Needs the built program (see below). |
| mollusk-svm CU benchmark | `clients/rust/tests/cu_benchmark.rs` | Per-instruction compute-unit ceiling assertions (upper bound with headroom, not an exact match). |
| mollusk-svm integration flows | `clients/rust/tests/full_flows.rs` | Multi-instruction flows (`initialize` -> `deposit` -> `withdraw`) chained against real, persisted account state via `Mollusk::process_instruction_chain` — see the file's module doc for why this replaces the originally-planned LiteSVM layer. |
| E2E | `clients/js/test/*.test.ts` | Real transactions against `solana-test-validator`, including one rejection demo (non-owner withdraw). Runs on its own CI cadence — see [CI](#ci). |

Every mollusk-svm test needs the real compiled program (`target/deploy/pinocchio_vault.so`),
loaded via `SBF_OUT_DIR` — build it first:

```sh
pnpm programs:build       # cargo build-sbf, plus dumps the real Token-2022 program binary
pnpm programs:test        # cargo test-sbf, for programs/pinocchio-vault/tests/wire_contract.rs
pnpm clients:rust:test    # mollusk-svm unit/integration/benchmark tests
```

### Windows-specific build note

`cargo build-sbf`'s host-side build-script compilation (proc-macros like `proc-macro2`/`quote`)
requires a working host linker. On a fresh Windows machine without Visual Studio Build Tools
installed, both plain `cargo build`/`cargo check` *and* `cargo build-sbf` fail with
`linker 'link.exe' not found`. A GNU toolchain (`rustup toolchain install stable-x86_64-pc-windows-gnu`
+ a MinGW-w64 install such as WinLibs, then `rustup override set stable-x86_64-pc-windows-gnu` in
this repo) unblocks plain `cargo check`/`cargo test` for fast iteration, but `cargo build-sbf`
pins its own bundled toolchain internally and still needs a real MSVC `link.exe` for its host-side
build scripts regardless of the ambient rustup override. Install
[Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
with the "Desktop development with C++" workload to unblock `cargo build-sbf`/`cargo test-sbf`
locally. CI (`ubuntu-latest`) does not hit this at all.

## Client examples

```sh
pnpm generate              # regenerate idl.json's address + both clients from idl.json
pnpm clients:rust:test     # SBF_OUT_DIR=target/deploy cargo test --features test-sbf
pnpm clients:js:test       # against a local validator -- see clients/js/test/_setup.ts
```

The JS client (`clients/js`) is a thin, testable proof the program works end-to-end via real
transactions — see `clients/js/test/initialize.test.ts` (happy path) and
`clients/js/test/deposit-withdraw.test.ts` (deposit/withdraw round trip + one rejection demo,
`isPinocchioVaultError`-typed).

## CI

- **`ci.yml`** — always-blocking on every PR/push to `main`: `cargo build-sbf`/`cargo test-sbf`,
  `cargo clippy -- -D warnings` (both crates), `cargo audit` against `.cargo/audit.toml`,
  the checked-in Semgrep ruleset (`.semgrep/solana-vault.yml`), the JS client lint/build, and the
  "no Anchor anywhere" `cargo tree` check.
- **`e2e.yml`** — PR-triggered only on paths that can affect an on-chain flow
  (`programs/**`, `clients/**`, `fixtures/**`) plus a nightly scheduled run, *not* unconditional
  on every PR (validator-startup flake under CI resource contention is a real, unresolved risk
  that readiness polling alone doesn't eliminate). Starts a local `solana-test-validator` with
  validator-readiness polling (not a fixed sleep — see `scripts/start-validator.mjs`) and runs
  the JS client's e2e suite against it.

**This workflow file alone does not block merges** — that additionally requires GitHub
branch-protection configured to require the `ci.yml` checks (a repository setting, not a
workflow-file property).

## Security scanning

- `cargo audit` against [`.cargo/audit.toml`](.cargo/audit.toml) — currently zero ignores needed
  (see that file's history note for why an earlier 9-entry ignore list was removed as stale).
- [`.semgrep/solana-vault.yml`](.semgrep/solana-vault.yml) — a small, checked-in ruleset targeting
  the omission patterns this checklist cares about: unverified CPI targets
  (`invoke_with_unverified_program`) and placeholder/panic-prone code
  (`todo!`/`unimplemented!`/`panic!`/`.unwrap()`/`.expect()`) reaching program source. Solana-specific
  dataflow scanners (X-Ray, Radar) were evaluated but not wired into CI in this pass, since their
  CI reliability against this exact toolchain wasn't verified in-session; Semgrep is what's
  actually pinned and running today. See the ruleset file's header comment for the full note.

## License

MIT — see [`LICENSE`](LICENSE).
