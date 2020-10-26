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
    /// Initializes a new reserve.
    ///
    /// TBD should reserves have their own authority that is separate from the pool?
    ///
    ///   0. `[writable]` Reserve account.
    ///   1. `[]` Reserve token account. Must be non zero, owned by $authority.
    ///   2. `[]` Collateral token account. Must be empty, owned by $authority, minted by liquidity token mint.
    ///   3. `[]` Liquidity Token Mint. Must be empty, owned by $authority.
    ///   4. `[]` Rent sysvar
    ///   5. '[]` Token program id
    InitReserve {
        /// Authority derived from `create_program_address(&[Reserve account])`
        authority: Pubkey,
        // TODO: maintenance margin percent
        // TODO: borrow rate
        // TODO: lend rate
    },
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
    // Withdraw,
    // Borrow,
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
            0 => {
                let (authority, _rest) = Self::unpack_pubkey(rest)?;
                Self::InitReserve { authority }
            }
            1 => {
                let (amount, _rest) = Self::unpack_u64(rest)?;
                Self::Deposit { amount }
            }
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
            Self::InitReserve { authority } => {
                buf.push(0);
                buf.extend_from_slice(authority.as_ref());
            }
            Self::Deposit { amount } => {
                buf.push(1);
                buf.extend_from_slice(&amount.to_le_bytes());
            }
        }
        buf
    }
}

/// Creates an 'InitReserve' instruction.
pub fn init_reserve(
    program_id: &Pubkey,
    reserve_pubkey: &Pubkey,
    authority_pubkey: &Pubkey,
    reserve_token_pubkey: &Pubkey,
    collateral_token_pubkey: &Pubkey,
    liquidity_token_mint_pubkey: &Pubkey,
) -> Result<Instruction, ProgramError> {
    Ok(Instruction {
        program_id: *program_id,
        accounts: vec![
            AccountMeta::new(*reserve_pubkey, false),
            AccountMeta::new_readonly(*reserve_token_pubkey, false),
            AccountMeta::new_readonly(*collateral_token_pubkey, false),
            AccountMeta::new_readonly(*liquidity_token_mint_pubkey, false),
            AccountMeta::new_readonly(sysvar::rent::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
        data: LendingInstruction::InitReserve {
            authority: *authority_pubkey,
        }
        .pack(),
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
