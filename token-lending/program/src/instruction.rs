//! Instruction types

use crate::error::LendingError;
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar,
};
use std::{convert::TryInto, mem::size_of};

/// Instructions supported by the lending program.
#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub enum LendingInstruction {
    /// Initializes a new pool.
    ///
    ///   0. `[writable]` Pool account.
    ///   1. `[]` Quote currency token mint. Must be initialized.
    ///   2. `[]` Rent sysvar
    ///   3. '[]` Token program id
    InitPool, // TODO: liquidation margin threshold

    /// Initializes a new reserve.
    ///
    ///   0. `[writable]` Reserve account.
    ///   1. `[writable]` Pool account.
    ///   2. `[]` Reserve token account. Must be non zero, owned by $authority.
    ///   3. `[]` Collateral token account. Must be empty, owned by $authority, minted by liquidity token mint.
    ///   4. `[]` Liquidity Token Mint. Must be empty, owned by $authority.
    ///   5. `[]` Serum DEX market account. Must be initialized and match quote and base currency.
    ///   6. `[]` Rent sysvar
    ///   7. '[]` Token program id
    InitReserve, // TODO: maintenance margin percent, borrow rate, & lend rate

    /// Deposit tokens into a reserve. The output is a liquidity token representing ownership
    /// of the reserve liquidity pool.
    ///
    ///   0. `[]` Reserve account.
    ///   1. `[]` Authority derived from `create_program_address(&[Reserve account])`
    ///   2. `[writable]` reserve_token account, $authority can transfer $amount,
    ///   3. `[writable]` reserve_token Base account, specified by $reserve_account,
    ///   4. `[writable]` liquidity_token account, to deposit minted liquidity tokens,
    ///   5. `[writable]` Pool MINT account, $authority is the owner.
    ///   6. '[]` Token program id
    Deposit {
        /// Amount to deposit into the reserve
        amount: u64,
    },

    // TODO: Withdraw

    /// Borrow tokens from a reserve by depositing collateral tokens. The number of borrowed tokens
    /// is calculated by market price.
    ///
    ///   0. `[]` Deposit reserve account.
    ///   1. `[]` Withdraw reserve account.
    ///   2. `[]` Authority derived from `create_program_address(&[pool account])`
    ///   3. `[writable]` collateral_token source account, $authority can transfer $amount,
    ///   4. `[writable]` Deposit Reserve - collateral account
    ///   5. `[writable]` Withdraw Reserve - reserve account
    ///   6. `[writable]` borrowed_token destination account
    ///   7. `[writable]` Obligation - uninitialized
    ///   8. `[]` Clock sysvar
    ///   9. `[]` Rent sysvar
    ///   10. '[]` Token program id
    Borrow {
        /// Amount of collateral to deposit
        collateral_amount: u64,
        /// Authority of obligation info account
        obligation_authority: Pubkey,
    },

    /// Set the market price of a reserve pool from DEX market accounts.
    ///
    ///   0. `[writable]` Reserve account.
    ///   1. `[]` Serum DEX market account. Must be initialized and match reserve market account.
    ///   3. `[]` Serum DEX market bids. Must be initialized and match dex market.
    ///   2. `[]` Serum DEX market asks. Must be initialized and match dex market.
    ///   4. `[]` Clock sysvar
    SetPrice,

    // Repay,
    // Liquidate,
}

impl LendingInstruction {
    /// Unpacks a byte buffer into a [LendingInstruction](enum.LendingInstruction.html).
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = input
            .split_first()
            .ok_or(LendingError::InvalidInstruction)?;
        Ok(match tag {
            0 => Self::InitPool,
            1 => Self::InitReserve,
            2 => {
                let (amount, _rest) = Self::unpack_u64(rest)?;
                Self::Deposit { amount }
            }
            3 => {
                let (collateral_amount, rest) = Self::unpack_u64(rest)?;
                let (obligation_authority, _rest) = Self::unpack_pubkey(rest)?;
                Self::Borrow {
                    collateral_amount,
                    obligation_authority,
                }
            }
            4 => Self::SetPrice,
            _ => return Err(LendingError::InvalidInstruction.into()),
        })
    }

    fn unpack_u64(input: &[u8]) -> Result<(u64, &[u8]), ProgramError> {
        if input.len() >= 8 {
            let (amount, rest) = input.split_at(8);
            let amount = amount
                .get(..8)
                .and_then(|slice| slice.try_into().ok())
                .map(u64::from_le_bytes)
                .ok_or(LendingError::InvalidInstruction)?;
            Ok((amount, rest))
        } else {
            Err(LendingError::InvalidInstruction.into())
        }
    }

    fn unpack_pubkey(input: &[u8]) -> Result<(Pubkey, &[u8]), ProgramError> {
        if input.len() >= 32 {
            let (key, rest) = input.split_at(32);
            let pk = Pubkey::new(key);
            Ok((pk, rest))
        } else {
            Err(LendingError::InvalidInstruction.into())
        }
    }

    /// Packs a [LendingInstruction](enum.LendingInstruction.html) into a byte buffer.
    pub fn pack(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(size_of::<Self>());
        match *self {
            Self::InitPool => {
                buf.push(0);
            }
            Self::InitReserve => {
                buf.push(1);
            }
            Self::Deposit { amount } => {
                buf.push(2);
                buf.extend_from_slice(&amount.to_le_bytes());
            }
            Self::Borrow {
                collateral_amount,
                obligation_authority,
            } => {
                buf.push(3);
                buf.extend_from_slice(&collateral_amount.to_le_bytes());
                buf.extend_from_slice(obligation_authority.as_ref());
            }
            Self::SetPrice => {
                buf.push(4);
            }
        }
        buf
    }
}

/// Creates an 'InitPool' instruction.
pub fn init_pool(program_id: &Pubkey, pool_pubkey: &Pubkey) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(*pool_pubkey, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
        ],
        data: LendingInstruction::InitPool.pack(),
    })
}

/// Creates an 'InitReserve' instruction.
pub fn init_reserve(
    program_id: &Pubkey,
    reserve_pubkey: &Pubkey,
    pool_pubkey: &Pubkey,
    reserve_token_pubkey: &Pubkey,
    collateral_token_pubkey: &Pubkey,
    liquidity_token_mint_pubkey: &Pubkey,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(*reserve_pubkey, false),
            AccountMeta::new(*pool_pubkey, false),
            AccountMeta::new_readonly(*reserve_token_pubkey, false),
            AccountMeta::new_readonly(*collateral_token_pubkey, false),
            AccountMeta::new_readonly(*liquidity_token_mint_pubkey, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::InitReserve.pack(),
    })
}

/// Creates a 'Deposit' instruction.
pub fn deposit(
    program_id: &Pubkey,
    reserve_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    amount: u64,
    reserve_token_pubkey: &Pubkey,
    base_reserve_token_pubkey: &Pubkey,
    liquidity_token_pubkey: &Pubkey,
    liquidity_token_mint_pubkey: &Pubkey,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new_readonly(*reserve_pubkey, false),
            AccountMeta::new_readonly(*authority_pubkey, false),
            AccountMeta::new(*reserve_token_pubkey, false),
            AccountMeta::new(*base_reserve_token_pubkey, false),
            AccountMeta::new(*liquidity_token_pubkey, false),
            AccountMeta::new(*liquidity_token_mint_pubkey, false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::Deposit { amount }.pack(),
    })
}
