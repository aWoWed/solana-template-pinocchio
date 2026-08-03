import { fetchToken } from '@solana-program/token-2022';
import { appendTransactionMessageInstruction, pipe } from '@solana/kit';
import test from 'ava';
import {
  findVaultStatePda,
  findVaultTokenPda,
  getDepositInstruction,
  getInitializeInstructionAsync,
  getWithdrawInstruction,
  isPinocchioVaultError,
  PINOCCHIO_VAULT_ERROR__NOT_VAULT_OWNER,
} from '../src';
import {
  createDefaultSolanaClient,
  createDefaultTransaction,
  createFundedToken2022Account,
  createToken2022Mint,
  generateKeyPairSignerWithSol,
  signAndSendTransaction,
} from './_setup';

test('it deposits into and withdraws from a vault', async (t) => {
  // Given an initialized vault over an extension-free Token-2022 mint, and
  // the owner's own funded token account for that mint (deposits are
  // permissionless, but a single actor exercising the whole lifecycle is
  // the realistic common case).
  const client = createDefaultSolanaClient();
  const owner = await generateKeyPairSignerWithSol(client);
  const mint = await createToken2022Mint(client, owner);

  const initializeIx = await getInitializeInstructionAsync({ owner, mint });
  await pipe(
    await createDefaultTransaction(client, owner),
    (tx) => appendTransactionMessageInstruction(initializeIx, tx),
    (tx) => signAndSendTransaction(client, tx)
  );

  const [vaultState] = await findVaultStatePda({ owner: owner.address });
  const [vaultTokenAccount] = await findVaultTokenPda({ vaultState });

  const depositAmount = 700_000n;
  const withdrawAmount = 250_000n;
  const ownerTokenAccount = await createFundedToken2022Account(
    client,
    owner,
    mint,
    owner.address,
    owner,
    1_000_000n
  );

  // When the owner deposits into the vault's program-derived token account.
  const depositIx = getDepositInstruction({
    depositor: owner,
    vaultState,
    vaultTokenAccount,
    depositorTokenAccount: ownerTokenAccount,
    mint,
    amount: depositAmount,
  });
  await pipe(
    await createDefaultTransaction(client, owner),
    (tx) => appendTransactionMessageInstruction(depositIx, tx),
    (tx) => signAndSendTransaction(client, tx)
  );

  const afterDeposit = await fetchToken(client.rpc, vaultTokenAccount);
  t.is(afterDeposit.data.amount, depositAmount);

  // And then withdraws part of it back out, signed by the program via
  // invoke_signed -- never the owner's own token-account authority (§2).
  const withdrawIx = getWithdrawInstruction({
    owner,
    vaultState,
    vaultTokenAccount,
    destinationTokenAccount: ownerTokenAccount,
    mint,
    amount: withdrawAmount,
  });
  await pipe(
    await createDefaultTransaction(client, owner),
    (tx) => appendTransactionMessageInstruction(withdrawIx, tx),
    (tx) => signAndSendTransaction(client, tx)
  );

  const afterWithdraw = await fetchToken(client.rpc, vaultTokenAccount);
  t.is(afterWithdraw.data.amount, depositAmount - withdrawAmount);
});

test('rejection demo: a non-owner cannot withdraw', async (t) => {
  // Pre-mortem #1 -- the primary drain-the-vault attack this template
  // exists to demonstrate defeating, exercised end-to-end against a real
  // localnet transaction rather than a mocked instruction.
  const client = createDefaultSolanaClient();
  const owner = await generateKeyPairSignerWithSol(client);
  const attacker = await generateKeyPairSignerWithSol(client);
  const mint = await createToken2022Mint(client, owner);

  const initializeIx = await getInitializeInstructionAsync({ owner, mint });
  await pipe(
    await createDefaultTransaction(client, owner),
    (tx) => appendTransactionMessageInstruction(initializeIx, tx),
    (tx) => signAndSendTransaction(client, tx)
  );

  const [vaultState] = await findVaultStatePda({ owner: owner.address });
  const [vaultTokenAccount] = await findVaultTokenPda({ vaultState });
  const destination = await createFundedToken2022Account(
    client,
    attacker,
    mint,
    attacker.address,
    owner
  );

  const withdrawIx = getWithdrawInstruction({
    owner: attacker,
    vaultState,
    vaultTokenAccount,
    destinationTokenAccount: destination,
    mint,
    amount: 1n,
  });
  const transactionMessage = await pipe(
    await createDefaultTransaction(client, attacker),
    (tx) => appendTransactionMessageInstruction(withdrawIx, tx)
  );

  const error = await t.throwsAsync(() =>
    signAndSendTransaction(client, transactionMessage)
  );
  t.true(
    isPinocchioVaultError(
      error,
      transactionMessage,
      PINOCCHIO_VAULT_ERROR__NOT_VAULT_OWNER
    ),
    `expected NotVaultOwner, got: ${error}`
  );
});
