/* eslint-disable @typescript-eslint/no-unsafe-assignment */
/* eslint-disable @typescript-eslint/no-unsafe-call */
/* eslint-disable @typescript-eslint/no-unsafe-member-access */

import {
  Account,
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  SYSVAR_RENT_PUBKEY,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import * as BufferLayout from "buffer-layout";
import * as Layout from "./layout";

const TOKEN_PROGRAM_ID = new PublicKey(
  "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
);

/**
 * @private
 */
export const TokenReserveLayout: typeof BufferLayout.Structure = BufferLayout.struct(
  [
    BufferLayout.u8("isInitialized"),
    BufferLayout.u8("bumpSeed"),
    Layout.publicKey("reserveToken"),
    Layout.publicKey("collateralToken"),
    Layout.publicKey("liquidityTokenMint"),
  ]
);

export type TokenLendingPoolParams = {
  connection: Connection;
  tokenProgramId?: PublicKey;
  lendingProgramId?: PublicKey;
  reserves: Array<TokenReserve>;
  payer: Account;
};

export class TokenLendingPool {
  connection: Connection;
  reserves: Array<TokenReserve>;

  constructor(params: TokenLendingPoolParams) {
    this.connection = params.connection;
    this.reserves = params.reserves;
  }
}

export type TokenReserveParams = {
  connection: Connection;
  tokenProgramId?: PublicKey;
  lendingProgramId: PublicKey;
  reserveAccount: Account;
  reserveToken: PublicKey;
  collateralToken: PublicKey;
  liquidityTokenMint: PublicKey;
  payer: Account;
};

export type InitReserveInstructionParams = {
  reserveAccount: PublicKey;
  reserveToken: PublicKey;
  collateralToken: PublicKey;
  liquidityTokenMint: PublicKey;
  tokenProgramId?: PublicKey;
  lendingProgramId: PublicKey;
};

export class TokenReserve {
  connection: Connection;
  tokenProgramId: PublicKey;
  lendingProgramId: PublicKey;
  reserveAccount: Account;
  reserveToken: PublicKey;
  collateralToken: PublicKey;
  liquidityTokenMint: PublicKey;
  payer: Account;

  constructor(params: TokenReserveParams) {
    this.connection = params.connection;
    this.tokenProgramId = params.tokenProgramId || TOKEN_PROGRAM_ID;
    this.lendingProgramId = params.lendingProgramId;
    this.reserveAccount = params.reserveAccount;
    this.reserveToken = params.reserveToken;
    this.collateralToken = params.collateralToken;
    this.liquidityTokenMint = params.liquidityTokenMint;
    this.payer = params.payer;
  }

  static async create(params: TokenReserveParams): Promise<TokenReserve> {
    const tokenReserve = new TokenReserve(params);

    // Allocate memory for the account
    const balanceNeeded = await TokenReserve.getMinBalanceRentForExemptTokenReserve(
      tokenReserve.connection
    );

    const transaction = new Transaction()
      .add(
        SystemProgram.createAccount({
          fromPubkey: tokenReserve.payer.publicKey,
          newAccountPubkey: tokenReserve.reserveAccount.publicKey,
          lamports: balanceNeeded,
          space: TokenReserveLayout.span,
          programId: tokenReserve.lendingProgramId,
        })
      )
      .add(
        await TokenReserve.createInitReserveInstruction({
          ...tokenReserve,
          reserveAccount: tokenReserve.reserveAccount.publicKey,
        })
      );

    await sendAndConfirmTransaction(
      tokenReserve.connection,
      transaction,
      [tokenReserve.payer, tokenReserve.reserveAccount],
      { commitment: "singleGossip", preflightCommitment: "singleGossip" }
    );

    return tokenReserve;
  }

  /**
   * Get the minimum balance for the token reserve account to be rent exempt
   *
   * @return Number of lamports required
   */
  static async getMinBalanceRentForExemptTokenReserve(
    connection: Connection
  ): Promise<number> {
    return await connection.getMinimumBalanceForRentExemption(
      TokenReserveLayout.span
    );
  }

  static async createInitReserveInstruction(
    params: InitReserveInstructionParams
  ): Promise<TransactionInstruction> {
    const tokenProgramId = params.tokenProgramId || TOKEN_PROGRAM_ID;
    const programId = params.lendingProgramId;
    const keys = [
      { pubkey: params.reserveAccount, isSigner: false, isWritable: true },
      { pubkey: params.reserveToken, isSigner: false, isWritable: false },
      { pubkey: params.collateralToken, isSigner: false, isWritable: false },
      { pubkey: params.liquidityTokenMint, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: tokenProgramId, isSigner: false, isWritable: false },
    ];
    const commandDataLayout = BufferLayout.struct([
      BufferLayout.u8("instruction"),
      Layout.publicKey("authority"),
    ]);
    const [authority] = await PublicKey.findProgramAddress(
      [params.reserveAccount.toBuffer()],
      programId
    );
    let data = Buffer.alloc(1024);
    {
      const encodeLength = commandDataLayout.encode(
        {
          instruction: 0, // InitializeReserve instruction
          authority: authority.toBuffer(),
        },
        data
      );
      data = data.slice(0, encodeLength);
    }
    return new TransactionInstruction({
      keys,
      programId,
      data,
    });
  }
}
