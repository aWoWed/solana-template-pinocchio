//! Single-owner Token-2022 vault, implemented against `pinocchio` (no_std,
//! zero-copy) per `docs/vault-design.md`'s frozen wire contract.

#![no_std]

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;
pub mod error;
pub mod instruction;
pub mod processor;
pub mod state;
pub mod token2022;

pinocchio::address::declare_id!("C5PLDoGR9PXmSc7XZap8Sbp3HCALViQAkQmEVG9HfYQ2");
