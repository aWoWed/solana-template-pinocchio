//! Behavioral proof that `overflow-checks = true` (workspace-root
//! `Cargo.toml` `[profile.release]`) actually reaches the built artifact --
//! per the consensus plan, a text-based `grep overflow-checks Cargo.toml`
//! only proves the text exists somewhere, not that it's under
//! `[profile.release]` at the workspace root specifically (Cargo silently
//! ignores profile keys in a member crate's own manifest).
//!
//! **This test only proves anything under `--release`.** A debug build has
//! `overflow-checks = true` by default *regardless* of this workspace's
//! profile setting, so `cargo test` (debug) would pass here even if
//! `overflow-checks` were removed from the root `Cargo.toml` entirely --
//! that would be exactly the kind of "aspirational, not testable" check
//! Principle 2 rejects. Run:
//!
//! ```sh
//! cargo test --release --manifest-path programs/pinocchio-vault/Cargo.toml \
//!   --test overflow_checks
//! ```
//!
//! If `overflow-checks = true` is ever removed from the root profile, this
//! specific test starts failing under `--release` (the addition silently
//! wraps to `0` instead of panicking, so `#[should_panic]` sees no panic).

#[test]
#[should_panic(expected = "attempt to add with overflow")]
fn release_profile_panics_on_overflow_instead_of_wrapping() {
    // Not a real vault code path -- the vault's own arithmetic (checklist
    // item 10) is deliberately structured as a comparison
    // (`amount > available`), never a subtraction/addition that could
    // itself overflow, so there is no genuine unchecked path in production
    // code to exercise here. This directly exercises the workspace's
    // `overflow-checks` profile setting on the same primitive type
    // (`u64`) the vault's `amount`/balance arithmetic uses.
    let max = std::hint::black_box(u64::MAX);
    let one = std::hint::black_box(1u64);
    let _ = max + one;
}
