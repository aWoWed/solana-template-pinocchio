#!/usr/bin/env zx
import 'zx/globals';
import { getCargo, getProgramFolders } from './utils.mjs';

// ---------------------------------------------------------------------------
// HAND-AUTHORED IDL — see docs/idl-generation-notes.md for the full rationale.
//
// Shank's derives require `std` + Borsh; this `no_std` zero-copy Pinocchio
// crate does not use either, so it cannot be Shank-annotated.
//
// The program folder therefore carries its own checked-in `idl.json`,
// authored by hand against `docs/vault-design.md`'s frozen wire contract and
// kept honest by two things: (1) `cargo test` includes a byte-offset test
// pinning the exact state layout the IDL describes, and (2) this script
// re-addresses `metadata.address` from the program's actual on-disk keypair
// on every run, so the IDL can never silently drift from the real deployed
// program ID.
//
// This script fails loudly (not silently) if a program folder is missing its
// idl.json, rather than falling through to Shank or Anchor generation.
// ---------------------------------------------------------------------------

for (const folder of getProgramFolders()) {
  const cargo = getCargo(folder);
  const programDir = path.join(__dirname, '..', folder);
  const idlPath = path.join(programDir, 'idl.json');

  if (!fs.existsSync(idlPath)) {
    throw new Error(
      `${folder}/idl.json is missing. This workspace has no Shank-compatible ` +
        `program crate (see docs/idl-generation-notes.md) — every program ` +
        `folder must carry a hand-authored idl.json checked in alongside it.`
    );
  }

  const idl = JSON.parse(fs.readFileSync(idlPath, 'utf8'));
  const programId = cargo.package.metadata.solana['program-id'];

  if (idl.metadata?.address !== programId) {
    idl.metadata = { ...(idl.metadata ?? {}), address: programId };
    fs.writeFileSync(idlPath, `${JSON.stringify(idl, null, 2)}\n`);
    echo(
      chalk.yellow(`[ RE-ADDRESSED ]`),
      `${folder}/idl.json address updated to match Cargo.toml's program-id (${programId}).`
    );
  } else {
    echo(chalk.green(`[ OK ]`), `${folder}/idl.json is already in sync.`);
  }
}
