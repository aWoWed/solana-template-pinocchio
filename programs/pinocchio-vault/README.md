# pinocchio-vault

A single-owner Token-2022 vault written against [Pinocchio](https://github.com/anza-xyz/pinocchio)
(`no_std`, zero-copy, no Anchor). See the repository root [README](../../README.md) for the full
template rationale, and [`docs/vault-design.md`](../../docs/vault-design.md) /
[`docs/security-checklist.md`](../../docs/security-checklist.md) for the frozen wire contract and
the security checklist this program is built against.

## Build & test

Run from this directory (see the `BUILD NOTE` in `Cargo.toml` for why a root-invoked
`cargo build-sbf` breaks under this workspace's virtual manifest):

```sh
cargo build-sbf
cargo test-sbf
```

Or via the repo-root scripts, which handle the external Token-2022 program dependency and the
deterministic deploy keypair automatically:

```sh
pnpm programs:build
pnpm programs:test
```

## Scope

Single-owner authorization, program-derived Token-2022 token account, no accepted mint
extensions in v1 (see `docs/vault-design.md` §7). Only `Initialize`/`Deposit`/`Withdraw` exist —
there is no admin/close/upgrade instruction.
