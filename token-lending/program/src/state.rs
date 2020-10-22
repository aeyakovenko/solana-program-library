//! State types

use arrayref::{array_mut_ref, array_ref, array_refs, mut_array_refs};
use solana_program::{
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack, Sealed},
    pubkey::Pubkey,
};

/// Lending pool state
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PoolState {}

/// Pool reserve state
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReserveInfo {
    /// Initialized state.
    pub is_initialized: bool,

    /// Extra octet used to bump the program address off the curve.
    /// The program address is created deterministically with the bump seed,
    /// lending program id, and reserve account pubkey.  This program address has
    /// authority over the reserve's token accounts and the liquidity token mint.
    pub bump_seed: u8,

    /// Reserve token pool
    pub reserve: Pubkey,
    /// Collateral token pool (liquidity tokens)
    pub collateral: Pubkey,
    /// Liquidity tokens are minted when reserve tokens are deposited.
    /// Liquidity tokens can be withdrawn back to the original reserve token.
    pub liquidity_token_mint: Pubkey,
}

/// Borrow obligation state
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObligationState {}

impl Sealed for ReserveInfo {}
impl IsInitialized for ReserveInfo {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

const RESERVE_LEN: usize = 98;
impl Pack for ReserveInfo {
    const LEN: usize = 98;

    /// Unpacks a byte buffer into a [SwapInfo](struct.SwapInfo.html).
    fn unpack_from_slice(input: &[u8]) -> Result<Self, ProgramError> {
        let input = array_ref![input, 0, RESERVE_LEN];
        #[allow(clippy::ptr_offset_with_cast)]
        let (is_initialized, bump_seed, reserve, collateral, liquidity_token_mint) =
            array_refs![input, 1, 1, 32, 32, 32];
        Ok(Self {
            is_initialized: match is_initialized {
                [0] => false,
                [1] => true,
                _ => return Err(ProgramError::InvalidAccountData),
            },
            bump_seed: bump_seed[0],
            reserve: Pubkey::new_from_array(*reserve),
            collateral: Pubkey::new_from_array(*collateral),
            liquidity_token_mint: Pubkey::new_from_array(*liquidity_token_mint),
        })
    }

    fn pack_into_slice(&self, output: &mut [u8]) {
        let output = array_mut_ref![output, 0, 98];
        let (is_initialized, bump_seed, reserve, collateral, pool_mint) =
            mut_array_refs![output, 1, 1, 32, 32, 32];
        is_initialized[0] = self.is_initialized as u8;
        bump_seed[0] = self.bump_seed;
        reserve.copy_from_slice(self.reserve.as_ref());
        collateral.copy_from_slice(self.collateral.as_ref());
        pool_mint.copy_from_slice(self.liquidity_token_mint.as_ref());
    }
}
