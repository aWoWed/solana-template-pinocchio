# IDL Generation Notes

## Why the IDL is hand-authored

`programs/pinocchio-vault/idl.json` is **hand-authored**, not generated. Shank's
`#[derive(ShankInstruction, ShankContext)]` macros need `std` and Borsh; this program is a
`no_std`, zero-copy Pinocchio crate that uses neither, so Shank cannot annotate it.

The IDL is instead written directly against `docs/vault-design.md`'s frozen wire contract
(accounts, instruction discriminants, state layout, error codes) and checked into the repository
like any other source file.

Two things keep it honest instead of a code generator:

1. `cargo test -p pinocchio-vault` includes a byte-offset test asserting the exact 107-byte
   `VaultState` layout from `vault-design.md` §4, and a test pinning all 14 error codes (0-13) to
   §9's table. If the Rust code and the IDL ever disagree on layout or error codes, the Rust-side
   test catches the drift — the IDL itself just needs to be kept in sync by hand when the design
   doc changes.
2. `scripts/generate-idls.mjs` re-reads the program's real keypair-derived `program-id` from
   `Cargo.toml` on every run and rewrites `idl.json`'s `metadata.address` if it doesn't match — so
   the deployed address can never silently drift from what the IDL/client describe, even though
   the instruction/account shape is maintained by hand.

## If a Shank-compatible program is ever added

If a Shank-compatible crate is ever added to this workspace, `scripts/generate-idls.mjs` would
need to be revisited — it currently expects the program folder to carry its own checked-in
`idl.json` and does not attempt Shank extraction at all. Adding a generator-based flow for such a
crate while keeping the hand-authored flow for Pinocchio is possible but not built.
