import { fetchToken } from '@solana-program/token-2022';
import { appendTransactionMessageInstruction, pipe } from '@solana/kit';
import test from 'ava';
import {
  fetchVaultState,
  findVaultStatePda,
  findVaultTokenPda,
  getInitializeInstructionAsync,
} from '../src';
import {
  createDefaultSolanaClient,
  createDefaultTransaction,
  createToken2022Mint,
  generateKeyPairSignerWithSol,
  signAndSendTransaction,
} from './_setup';

test('it initializes a vault over a Token-2022 mint', async (t) => {
  // Given an owner with some SOL and an extension-free Token-2022 mint.
  const client = createDefaultSolanaClient();
  const owner = await generateKeyPairSignerWithSol(client);
  const mint = await createToken2022Mint(client, owner);

  // When the owner initializes a vault for that mint.
  const initializeIx = await getInitializeInstructionAsync({ owner, mint });
  await pipe(
    await createDefaultTransaction(client, owner),
    (tx) => appendTransactionMessageInstruction(initializeIx, tx),
    (tx) => signAndSendTransaction(client, tx)
  );

  // Then the vault state PDA holds exactly what vault-design.md §4 specifies.
  const [vaultState, vaultStateBump] = await findVaultStatePda({
    owner: owner.address,
  });
  const [vaultTokenAccount, vaultTokenAccountBump] = await findVaultTokenPda({
    vaultState,
  });

  const account = await fetchVaultState(client.rpc, vaultState);
  t.deepEqual(account.data, {
    accountInitFlag: 1,
    owner: owner.address,
    mint,
    tokenAccount: vaultTokenAccount,
    bump: vaultStateBump,
    tokenAccountBump: vaultTokenAccountBump,
    reserved: new Uint8Array(8),
  });

  // And the token account is a real Token-2022 account for that mint whose
  // authority is the vault state PDA, never the human owner (§2).
  const token = await fetchToken(client.rpc, vaultTokenAccount);
  t.is(token.data.mint, mint);
  t.is(token.data.owner, vaultState);
  t.is(token.data.amount, 0n);
});
