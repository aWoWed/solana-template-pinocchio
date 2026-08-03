#!/usr/bin/env zx
import 'zx/globals';
import {
  cliArguments,
  getCargo,
  getProgramFolders,
  workingDirectory,
} from '../utils.mjs';

// Save external programs binaries to the output directory.
import './dump.mjs';

// Configure additional arguments here, e.g.:
// ['--arg1', '--arg2', ...cliArguments()]
const buildArgs = cliArguments();

const deployDir = path.join(workingDirectory, 'target', 'deploy');

// Build the programs.
for (const folder of getProgramFolders()) {
  const manifestPath = path.join(workingDirectory, folder, 'Cargo.toml');

  // `cargo-build-sbf` mints a *random* program keypair when
  // `target/deploy/<crate>-keypair.json` is absent, which would deploy the
  // program at an address that matches neither `Cargo.toml`'s `program-id` nor
  // the address baked into `idl.json` and the generated clients. Seed the
  // deploy directory from the checked-in keypair first so the deployed address
  // is deterministic.
  const cargo = getCargo(folder);
  const crateName = cargo.package.name.replace(/-/g, '_');
  const keypairPath = path.join(workingDirectory, folder, 'keypair.json');
  const deployKeypairPath = path.join(deployDir, `${crateName}-keypair.json`);

  await $`mkdir -p ${deployDir}`.quiet();
  fs.copyFileSync(keypairPath, deployKeypairPath);

  const { stdout } = await $`solana-keygen pubkey ${deployKeypairPath}`.quiet();
  const actualProgramId = stdout.trim();
  const declaredProgramId = cargo.package.metadata.solana['program-id'];

  if (actualProgramId !== declaredProgramId) {
    throw new Error(
      `${folder}/keypair.json resolves to ${actualProgramId}, but ` +
        `${folder}/Cargo.toml declares program-id ${declaredProgramId}.`
    );
  }

  await $`cargo-build-sbf --manifest-path ${manifestPath} ${buildArgs}`;
}
