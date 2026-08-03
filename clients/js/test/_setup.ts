import { getCreateAccountInstruction } from '@solana-program/system';
import {
  TOKEN_2022_PROGRAM_ADDRESS,
  findAssociatedTokenPda,
  getCreateAssociatedTokenInstructionAsync,
  getInitializeMint2Instruction,
  getMintSize,
  getMintToInstruction,
} from '@solana-program/token-2022';
import {
  Address,
  Commitment,
  Rpc,
  RpcSubscriptions,
  SolanaRpcApi,
  SolanaRpcSubscriptionsApi,
  TransactionMessage,
  TransactionMessageWithBlockhashLifetime,
  TransactionMessageWithFeePayer,
  TransactionSigner,
  airdropFactory,
  appendTransactionMessageInstructions,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  generateKeyPairSigner,
  getSignatureFromTransaction,
  lamports,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
} from '@solana/kit';

// `CompilableTransactionMessage` was removed from `@solana/kit` 7.x --
// message-plus-fee-payer is now spelled as this intersection directly (see
// e.g. `SingleTransactionPlan` in `@solana/instruction-plans`).
type CompilableTransactionMessage = TransactionMessage &
  TransactionMessageWithFeePayer;

type Client = {
  rpc: Rpc<SolanaRpcApi>;
  rpcSubscriptions: RpcSubscriptions<SolanaRpcSubscriptionsApi>;
};

export const createDefaultSolanaClient = (): Client => {
  const rpc = createSolanaRpc('http://127.0.0.1:8899');
  const rpcSubscriptions = createSolanaRpcSubscriptions('ws://127.0.0.1:8900');
  return { rpc, rpcSubscriptions };
};

export const generateKeyPairSignerWithSol = async (
  client: Client,
  putativeLamports: bigint = 1_000_000_000n
) => {
  const signer = await generateKeyPairSigner();
  await airdropFactory(client)({
    recipientAddress: signer.address,
    lamports: lamports(putativeLamports),
    commitment: 'confirmed',
  });
  return signer;
};

export const createDefaultTransaction = async (
  client: Client,
  feePayer: TransactionSigner
) => {
  const { value: latestBlockhash } = await client.rpc
    .getLatestBlockhash()
    .send();
  return pipe(
    createTransactionMessage({ version: 0 }),
    (tx) => setTransactionMessageFeePayerSigner(feePayer, tx),
    (tx) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, tx)
  );
};

export const signAndSendTransaction = async (
  client: Client,
  transactionMessage: CompilableTransactionMessage &
    TransactionMessageWithBlockhashLifetime,
  commitment: Commitment = 'confirmed'
) => {
  const signedTransaction =
    await signTransactionMessageWithSigners(transactionMessage);
  const signature = getSignatureFromTransaction(signedTransaction);
  // `signTransactionMessageWithSigners`'s return type (@solana/kit 7.0.0) is
  // `TransactionWithLifetime`, the union covering both blockhash and
  // durable-nonce lifetimes, regardless of which lifetime kind the input
  // message actually carries -- an upstream typing gap, not a real
  // ambiguity here: `signAndSendTransaction`'s own parameter type already
  // requires `TransactionMessageWithBlockhashLifetime`, so the signed
  // result is always blockhash-lifetime in practice.
  await sendAndConfirmTransactionFactory(client)(
    signedTransaction as Parameters<
      ReturnType<typeof sendAndConfirmTransactionFactory>
    >[0],
    { commitment }
  );
  return signature;
};

export const getBalance = async (client: Client, address: Address) =>
  (await client.rpc.getBalance(address, { commitment: 'confirmed' }).send())
    .value;

/**
 * An extension-free Token-2022 mint — the only mint shape
 * `docs/vault-design.md` §7 accepts.
 */
export const createToken2022Mint = async (
  client: Client,
  payer: TransactionSigner,
  decimals: number = 6
): Promise<Address> => {
  const mint = await generateKeyPairSigner();
  const space = BigInt(getMintSize());
  const rent = await client.rpc
    .getMinimumBalanceForRentExemption(space)
    .send();

  await pipe(
    await createDefaultTransaction(client, payer),
    (tx) =>
      appendTransactionMessageInstructions(
        [
          getCreateAccountInstruction({
            payer,
            newAccount: mint,
            lamports: rent,
            space,
            programAddress: TOKEN_2022_PROGRAM_ADDRESS,
          }),
          getInitializeMint2Instruction({
            mint: mint.address,
            decimals,
            mintAuthority: payer.address,
            freezeAuthority: null,
          }),
        ],
        tx
      ),
    (tx) => signAndSendTransaction(client, tx)
  );

  return mint.address;
};

/**
 * Creates the given owner's associated Token-2022 account for `mint` and
 * mints `amount` tokens into it, funded/authorized by `mintAuthority`
 * (matches `createToken2022Mint`'s `mintAuthority: payer.address`).
 */
export const createFundedToken2022Account = async (
  client: Client,
  payer: TransactionSigner,
  mint: Address,
  owner: Address,
  mintAuthority: TransactionSigner,
  amount: bigint = 0n
): Promise<Address> => {
  const [ata] = await findAssociatedTokenPda({
    owner,
    mint,
    tokenProgram: TOKEN_2022_PROGRAM_ADDRESS,
  });

  const createAtaIx = await getCreateAssociatedTokenInstructionAsync({
    payer,
    owner,
    mint,
  });

  const instructions =
    amount > 0n
      ? [
          createAtaIx,
          getMintToInstruction({
            mint,
            token: ata,
            mintAuthority,
            amount,
          }),
        ]
      : [createAtaIx];

  await pipe(
    await createDefaultTransaction(client, payer),
    (tx) => appendTransactionMessageInstructions(instructions, tx),
    (tx) => signAndSendTransaction(client, tx)
  );

  return ata;
};
