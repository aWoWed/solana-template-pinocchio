#!/usr/bin/env zx
import 'zx/globals';
import * as c from 'codama';
import { rootNodeFromAnchor } from '@codama/nodes-from-anchor';
import { renderVisitor as renderJavaScriptVisitor } from '@codama/renderers-js';
import { renderVisitor as renderRustVisitor } from '@codama/renderers-rust';
import { getAllProgramIdls } from './utils.mjs';

// Mirrors `programs/pinocchio-vault/src/token2022.rs`.
const TOKEN_2022_ID = 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb';
const SYSTEM_PROGRAM_ID = '11111111111111111111111111111111';
const BASE_TOKEN_ACCOUNT_LEN = 165;

// Mirrors `state::ACCOUNT_INIT_FLAG_INITIALIZED`.
const ACCOUNT_INIT_FLAG_INITIALIZED = 1;

const [idl, ...additionalIdls] = getAllProgramIdls().map((idl) =>
  rootNodeFromAnchor(require(idl))
);
const codama = c.createFromRoot(idl, additionalIdls);

// `docs/vault-design.md` §3 — vault state PDA.
codama.update(
  c.updateAccountsVisitor({
    vaultState: {
      seeds: [
        c.constantPdaSeedNodeFromString('utf8', 'vault'),
        c.variablePdaSeedNode(
          'owner',
          c.publicKeyTypeNode(),
          'The vault owner this vault belongs to'
        ),
      ],
    },
  })
);

// The vault token account is a Token-2022 account, so it is not one of *this*
// program's account types and cannot carry seeds via `updateAccountsVisitor`.
// It is still a PDA of this program, so it is declared as a standalone one.
codama.update(
  c.addPdasVisitor({
    pinocchioVault: [
      c.pdaNode({
        name: 'vaultToken',
        seeds: [
          c.constantPdaSeedNodeFromString('utf8', 'vault_token'),
          c.variablePdaSeedNode(
            'vaultState',
            c.publicKeyTypeNode(),
            'The vault state PDA that is this token account authority'
          ),
        ],
      }),
    ],
  })
);

codama.update(
  c.updateInstructionsVisitor({
    initialize: {
      byteDeltas: [
        c.instructionByteDeltaNode(c.accountLinkNode('vaultState')),
        c.instructionByteDeltaNode(c.numberValueNode(BASE_TOKEN_ACCOUNT_LEN), {
          withHeader: true,
        }),
      ],
      accounts: {
        vaultState: {
          defaultValue: c.pdaValueNode('vaultState', [
            c.pdaSeedValueNode('owner', c.accountValueNode('owner')),
          ]),
        },
        vaultTokenAccount: {
          defaultValue: c.pdaValueNode('vaultToken', [
            c.pdaSeedValueNode('vaultState', c.accountValueNode('vaultState')),
          ]),
        },
        tokenProgram: {
          defaultValue: c.publicKeyValueNode(TOKEN_2022_ID, 'token2022'),
        },
        systemProgram: {
          defaultValue: c.publicKeyValueNode(
            SYSTEM_PROGRAM_ID,
            'systemProgram'
          ),
        },
      },
    },
    // `deposit`/`withdraw` cannot default `vaultState`/`vaultTokenAccount` the
    // way `initialize` does: `vaultState`'s PDA seed is the vault *owner*,
    // which is not knowable from either instruction's own accounts alone
    // (`deposit`'s signer is an arbitrary depositor, not necessarily the
    // owner) — callers must always supply both explicitly. `tokenProgram`
    // has no such ambiguity, so it gets the same default as `initialize`.
    deposit: {
      accounts: {
        tokenProgram: {
          defaultValue: c.publicKeyValueNode(TOKEN_2022_ID, 'token2022'),
        },
      },
    },
    withdraw: {
      accounts: {
        tokenProgram: {
          defaultValue: c.publicKeyValueNode(TOKEN_2022_ID, 'token2022'),
        },
      },
    },
  })
);

// `account_init_flag` is the account-level discriminator (`docs/vault-design.md`
// §6) — an initialized vault state account always carries 1 at offset 0.
codama.update(
  c.setAccountDiscriminatorFromFieldVisitor({
    vaultState: {
      field: 'accountInitFlag',
      value: c.numberValueNode(ACCOUNT_INIT_FLAG_INITIALIZED),
    },
  })
);

// Render JavaScript.
const jsClient = path.join(__dirname, '..', 'clients', 'js');
codama.accept(
  renderJavaScriptVisitor(path.join(jsClient, 'src', 'generated'), {
    prettierOptions: require(path.join(jsClient, '.prettierrc.json')),
  })
);

// Render Rust.
const rustClient = path.join(__dirname, '..', 'clients', 'rust');
codama.accept(
  renderRustVisitor(path.join(rustClient, 'src', 'generated'), {
    formatCode: true,
    crateFolder: rustClient,
  })
);
