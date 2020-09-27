//! Instruction types

#![allow(clippy::too_many_arguments)]

use solana_sdk::program_error::ProgramError;
use std::mem::size_of;

/// Fee rate as a ratio
/// Fee is minted on deposit
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fee {
    /// denominator of the fee ratio
    pub denominator: u64,
    /// numerator of the fee ratio
    pub numerator: u64,
}

/// Inital values for the Stake Pool
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InitArgs {
    /// Fee paid to the owner in pool tokens
    pub fee: Fee,
    /// Nonce used for the deposit program address
    pub deposit_nonce: u8,
    /// Nonce used for the withdraw program address
    /// This program address is used as the stake withdraw key as well
    pub withdraw_none: u8,
}

/// Instructions supported by the StakePool program.
#[repr(C)]
#[derive(Clone, Debug, PartialEq)]
pub enum StakePoolInstruction {
    ///   Initializes a new StakePool.
    ///
    ///   0. `[w, s]` New StakePool to create.
    ///   1. `[]` Owner
    ///   2. `[]` pool token Mint. Must be non zero, owned by withdraw authority.
    ///   3. `[w]` Pool Account to deposit the generated fee for owner.
    Initialize(InitArgs),

    ///   Deposit some stake into the pool.  The output is a "pool" token representing ownership
    ///   into the pool. Inputs are converted to the current ratio.
    ///
    ///   0. `[]` StakePool
    ///   1. `[]` deposit authority
    ///   2. `[]` withdraw  authority
    ///   3. `[w]` Stake, deposit authority is set as the withdrawal key
    ///   4. `[w]` Pool MINT account, authority is the owner.
    ///   5. `[w]` Pool Account to deposit the generated tokens.
    ///   6. `[w]` Pool Account to deposit the generated fee for owner.
    Deposit,

    ///   Withdraw the token from the pool at the current ratio.
    ///   The amount withdrawn is the MIN(u64, stake size)
    ///   
    ///   0. `[]` StakePool
    ///   1. `[]` withdraw  authority
    ///   2. `[w]` SOURCE Pool account, amount is transferable by authority
    ///   3. `[w]` Pool MINT account, authority is the owner
    ///   4. `[w]` Stake SOURCE owned by the withdraw authority  
    ///   6. `[w]` Stake destination, uninitialized, for the user stake
    ///   userdata: amount to withdraw
    Withdraw(u64),

    ///   Update the staking pubkey for a stake
    ///
    ///   0. `[w]` StakePool
    ///   1. `[s]` Owner
    ///   2. `[]` withdraw authority
    ///   3. '[]` Staking pubkey.
    ///   4. `[w]` Stake to update the staking pubkey
    UpdateStakingAuthority,

    ///   Update owner
    ///
    ///   0. `[w]` StakePool
    ///   1. `[s]` Owner
    ///   2. '[]` New owner pubkey
    ///   3. '[]` New owner fee account
    UpdateOwner,
}

impl StakePoolInstruction {
    /// Deserializes a byte buffer into an [StakePoolInstruction](enum.StakePoolInstruction.html).
    pub fn deserialize(input: &[u8]) -> Result<Self, ProgramError> {
        if input.len() < size_of::<u8>() {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(match input[0] {
            0 => {
                let val: &InitArgs = unpack(input)?;
                Self::Initialize(*val)
            }
            1 => Self::Deposit,
            2 => {
                let val: &u64 = unpack(input)?;
                Self::Withdraw(*val)
            }
            3 => Self::UpdateStakingAuthority,
            4 => Self::UpdateOwner,
            _ => return Err(ProgramError::InvalidAccountData),
        })
    }

    /// Serializes an [StakePoolInstruction](enum.StakePoolInstruction.html) into a byte buffer.
    pub fn serialize(&self) -> Result<Vec<u8>, ProgramError> {
        let mut output = vec![0u8; size_of::<StakePoolInstruction>()];
        match self {
            Self::Initialize(init) => {
                output[0] = 0;
                #[allow(clippy::cast_ptr_alignment)]
                let value = unsafe { &mut *(&mut output[1] as *mut u8 as *mut InitArgs) };
                *value = *init;
            }
            Self::Deposit => {
                output[0] = 1;
            }
            Self::Withdraw(val) => {
                output[0] = 2;
                #[allow(clippy::cast_ptr_alignment)]
                let value = unsafe { &mut *(&mut output[1] as *mut u8 as *mut u64) };
                *value = *val;
            }
            Self::UpdateStakingAuthority => {
                output[0] = 3;
            }
            Self::UpdateOwner => {
                output[0] = 4;
            }
        }
        Ok(output)
    }
}

/// Unpacks a reference from a bytes buffer.
pub fn unpack<T>(input: &[u8]) -> Result<&T, ProgramError> {
    if input.len() < size_of::<u8>() + size_of::<T>() {
        return Err(ProgramError::InvalidAccountData);
    }
    #[allow(clippy::cast_ptr_alignment)]
    let val: &T = unsafe { &*(&input[1] as *const u8 as *const T) };
    Ok(val)
}