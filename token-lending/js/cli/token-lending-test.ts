/* eslint-disable @typescript-eslint/no-unsafe-assignment */
/* eslint-disable @typescript-eslint/no-unsafe-member-access */

import fs from "mz/fs";
import {
  Account,
  Connection,
  BpfLoader,
  PublicKey,
  BPF_LOADER_PROGRAM_ID,
} from "@solana/web3.js";
import { Token } from "@solana/spl-token";

import { TokenReserve } from "../client";
import { Store } from "../client/util/store";
import { newAccountWithLamports } from "../client/util/new-account-with-lamports";
import { url } from "../client/util/url";

let connection: Connection | undefined;
async function getConnection(): Promise<Connection> {
  if (connection) return connection;

  connection = new Connection(url, "recent");
  const version = await connection.getVersion();

  console.log("Connection to cluster established:", url, version);
  return connection;
}

let tokenProgramId: PublicKey;
let lendingProgramId: PublicKey;

export async function loadPrograms(): Promise<void> {
  const connection = await getConnection();
  [tokenProgramId, lendingProgramId] = await GetPrograms(connection);

  console.log("SPL Token Program ID", tokenProgramId.toString());
  console.log("SPL Token Lending Program ID", lendingProgramId.toString());
}

export async function createLendingReserve(): Promise<void> {
  const connection = await getConnection();

  const payer = await newAccountWithLamports(
    connection,
    100000000000 /* wag */
  );

  const owner = await newAccountWithLamports(
    connection,
    100000000000 /* wag */
  );
  const reserveAccount = new Account();

  const [authority] = await PublicKey.findProgramAddress(
    [reserveAccount.publicKey.toBuffer()],
    lendingProgramId
  );

  console.log("creating liquidity token mint");
  const liquidityTokenMint = await Token.createMint(
    connection,
    payer,
    authority,
    null,
    2,
    tokenProgramId
  );

  console.log("creating collateral token account");
  const collateralToken = await liquidityTokenMint.createAccount(authority);

  console.log("creating reserve token mint");
  const reserveTokenMint = await Token.createMint(
    connection,
    payer,
    owner.publicKey,
    null,
    2,
    tokenProgramId
  );

  console.log("creating reserve token account");
  const reserveToken = await reserveTokenMint.createAccount(authority);

  console.log("creating token reserve");
  await TokenReserve.create({
    connection,
    tokenProgramId,
    reserveAccount,
    reserveToken,
    collateralToken,
    // TODO cleanup after @solana/spl-token v0.0.12 is released
    liquidityTokenMint: (liquidityTokenMint as any).publicKey,
    lendingProgramId,
    payer,
  });
}

async function loadProgram(
  connection: Connection,
  path: string
): Promise<PublicKey> {
  const data = await fs.readFile(path);
  const { feeCalculator } = await connection.getRecentBlockhash();

  const loaderCost =
    feeCalculator.lamportsPerSignature *
    BpfLoader.getMinNumSignatures(data.length);
  const minAccountBalance = await connection.getMinimumBalanceForRentExemption(
    0
  );
  const minExecutableBalance = await connection.getMinimumBalanceForRentExemption(
    data.length
  );
  const balanceNeeded = minAccountBalance + loaderCost + minExecutableBalance;

  const from = await newAccountWithLamports(connection, balanceNeeded);
  const program_account = new Account();
  console.log("Loading program:", path);
  await BpfLoader.load(
    connection,
    from,
    program_account,
    data,
    BPF_LOADER_PROGRAM_ID
  );
  return program_account.publicKey;
}

async function GetPrograms(
  connection: Connection
): Promise<[PublicKey, PublicKey]> {
  const store = new Store();
  let tokenProgramId = null;
  let tokenLendingProgramId = null;
  try {
    const config = await store.load("config.json");
    console.log("Using pre-loaded Token and Token-lending programs");
    console.log(
      "  Note: To reload programs remove client/util/store/config.json"
    );
    if ("tokenProgramId" in config && "tokenLendingProgramId" in config) {
      tokenProgramId = new PublicKey(config["tokenProgramId"]);
      tokenLendingProgramId = new PublicKey(config["tokenLendingProgramId"]);
    } else {
      throw new Error("Program ids not found");
    }
  } catch (err) {
    tokenProgramId = await loadProgram(
      connection,
      "../../target/bpfel-unknown-unknown/release/spl_token.so"
    );
    tokenLendingProgramId = await loadProgram(
      connection,
      "../../target/bpfel-unknown-unknown/release/spl_token_lending.so"
    );
    await store.save("config.json", {
      tokenProgramId: tokenProgramId.toString(),
      tokenLendingProgramId: tokenLendingProgramId.toString(),
    });
  }
  return [tokenProgramId, tokenLendingProgramId];
}
