//! State types

use arrayref::{array_mut_ref, array_ref, array_refs, mut_array_refs};
use solana_program::{
    sysvar::clock::Clock,
    program_error::ProgramError,
    program_pack::{IsInitialized, Pack, Sealed},
    pubkey::Pubkey,
};
use crate::error::LendingError;

/// Prices are only valid for a few slots before needing to be updated again
const PRICE_EXPIRATION_SLOTS: u64 = 5;

/// Maximum number of pool reserves
pub const MAX_RESERVES: u8 = 10;
const MAX_RESERVES_USIZE: usize = MAX_RESERVES as usize;

/// Lending pool state
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoolInfo {
    /// Initialized state.
    pub is_initialized: bool,
    /// Quote currency token mint.
    pub quote_token_mint: Pubkey,
    /// Number of active reserves.
    pub num_reserves: u8,
    /// List of reserves that belong to this pool.
    pub reserves: Box<[Pubkey; MAX_RESERVES_USIZE]>,
}

/// Pool reserve state
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReserveInfo {
    /// Initialized state.
    pub is_initialized: bool,
    /// Pool address
    pub pool: Pubkey,
    /// Reserve token pool
    pub reserve: Pubkey,
    /// Collateral token pool (liquidity tokens)
    pub collateral: Pubkey,
    /// Liquidity tokens are minted when reserve tokens are deposited.
    /// Liquidity tokens can be withdrawn back to the original reserve token.
    pub liquidity_token_mint: Pubkey,
    /// DEX market state account
    pub dex_market: Pubkey,
    /// Latest market price
    pub market_price: u64,
    /// DEX market state account
    pub market_price_updated_slot: u64,
}

impl ReserveInfo {
    /// Fetch the current market price
    pub fn current_market_price(&self, clock: &Clock) -> Result<u64, ProgramError> {
        if self.market_price_updated_slot == 0 {
            Err(LendingError::ReservePriceUnset.into())
        } else if self.market_price_updated_slot + PRICE_EXPIRATION_SLOTS <= clock.slot {
            Err(LendingError::ReservePriceExpired.into())
        } else {
            Ok(self.market_price)
        }
    }
}

/// Borrow obligation state
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ObligationInfo {
    /// Slot when obligation was created. Used for calculating interest.
    pub created_at_slot: u64,
    /// Address that has the authority to repay this obligation
    pub authority: Pubkey,
    /// Amount of collateral tokens deposited for this obligation
    pub collateral_amount: u64,
    /// Reserve which collateral tokens were deposited into
    pub collateral_reserve: Pubkey,
    /// Amount of tokens borrowed for this obligation
    pub borrow_amount: u64,
    /// Reserve which tokens were borrowed from
    pub borrow_reserve: Pubkey,
}

impl Sealed for ReserveInfo {}
impl IsInitialized for ReserveInfo {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

const RESERVE_LEN: usize = 177;
impl Pack for ReserveInfo {
    const LEN: usize = 177;

    /// Unpacks a byte buffer into a [ReserveInfo](struct.ReserveInfo.html).
    fn unpack_from_slice(input: &[u8]) -> Result<Self, ProgramError> {
        let input = array_ref![input, 0, RESERVE_LEN];
        #[allow(clippy::ptr_offset_with_cast)]
        let (is_initialized, pool, reserve, collateral, liquidity_token_mint, dex_market, market_price, market_price_updated_slot) =
            array_refs![input, 1, 32, 32, 32, 32, 32, 8, 8];
        Ok(Self {
            is_initialized: match is_initialized {
                [0] => false,
                [1] => true,
                _ => return Err(ProgramError::InvalidAccountData),
            },
            pool: Pubkey::new_from_array(*pool),
            reserve: Pubkey::new_from_array(*reserve),
            collateral: Pubkey::new_from_array(*collateral),
            liquidity_token_mint: Pubkey::new_from_array(*liquidity_token_mint),
            dex_market: Pubkey::new_from_array(*dex_market),
            market_price: u64::from_le_bytes(*market_price),
            market_price_updated_slot: u64::from_le_bytes(*market_price_updated_slot),
        })
    }

    fn pack_into_slice(&self, output: &mut [u8]) {
        let output = array_mut_ref![output, 0, RESERVE_LEN];
        let (is_initialized, pool, reserve, collateral, pool_mint, dex_market, market_price, market_price_updated_slot) =
            mut_array_refs![output, 1, 32, 32, 32, 32, 32, 8, 8];
        is_initialized[0] = self.is_initialized as u8;
        pool.copy_from_slice(self.pool.as_ref());
        reserve.copy_from_slice(self.reserve.as_ref());
        collateral.copy_from_slice(self.collateral.as_ref());
        pool_mint.copy_from_slice(self.liquidity_token_mint.as_ref());
        dex_market.copy_from_slice(self.dex_market.as_ref());
        *market_price = self.market_price.to_le_bytes();
        *market_price_updated_slot = self.market_price_updated_slot.to_le_bytes();
    }
}

impl Sealed for PoolInfo {}
impl IsInitialized for PoolInfo {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }
}

const POOL_LEN: usize = 354;
impl Pack for PoolInfo {
    const LEN: usize = 354;

    /// Unpacks a byte buffer into a [PoolInfo](struct.PoolInfo.html).
    fn unpack_from_slice(input: &[u8]) -> Result<Self, ProgramError> {
        let input = array_ref![input, 0, POOL_LEN];
        #[allow(clippy::ptr_offset_with_cast)]
        let (is_initialized, quote_token_mint, num_reserves, reserves_flat) =
            array_refs![input, 1, 32, 1, 32 * MAX_RESERVES_USIZE];
        let mut pool = Self {
            is_initialized: match is_initialized {
                [0] => false,
                [1] => true,
                _ => return Err(ProgramError::InvalidAccountData),
            },
            quote_token_mint: Pubkey::new_from_array(*quote_token_mint),
            num_reserves: num_reserves[0],
            reserves: Box::new([Pubkey::new_from_array([0u8; 32]); MAX_RESERVES_USIZE]),
        };
        for (src, dst) in reserves_flat.chunks(32).zip(pool.reserves.iter_mut()) {
            *dst = Pubkey::new(src);
        }
        Ok(pool)
    }

    fn pack_into_slice(&self, output: &mut [u8]) {
        let output = array_mut_ref![output, 0, POOL_LEN];
        let (is_initialized, quote_token_mint, num_reserves, reserves_flat) =
            mut_array_refs![output, 1, 32, 1, 32 * MAX_RESERVES_USIZE];
        *is_initialized = [self.is_initialized as u8];
        quote_token_mint.copy_from_slice(self.quote_token_mint.as_ref());
        *num_reserves = [self.num_reserves];
        for (i, src) in self.reserves.iter().enumerate() {
            let dst_array = array_mut_ref![reserves_flat, 32 * i, 32];
            dst_array.copy_from_slice(src.as_ref());
        }
    }
}

impl Sealed for ObligationInfo {}
impl IsInitialized for ObligationInfo {
    fn is_initialized(&self) -> bool {
        self.created_at_slot > 0
    }
}

const OBLIGATION_LEN: usize = 120;
impl Pack for ObligationInfo {
    const LEN: usize = 120;

    /// Unpacks a byte buffer into a [ObligationInfo](struct.ObligationInfo.html).
    fn unpack_from_slice(input: &[u8]) -> Result<Self, ProgramError> {
        let input = array_ref![input, 0, OBLIGATION_LEN];
        #[allow(clippy::ptr_offset_with_cast)]
        let (
            created_at_slot,
            authority,
            collateral_amount,
            collateral_reserve,
            borrow_amount,
            borrow_reserve,
        ) = array_refs![input, 8, 32, 8, 32, 8, 32];
        Ok(Self {
            created_at_slot: u64::from_le_bytes(*created_at_slot),
            authority: Pubkey::new_from_array(*authority),
            collateral_amount: u64::from_le_bytes(*collateral_amount),
            collateral_reserve: Pubkey::new_from_array(*collateral_reserve),
            borrow_amount: u64::from_le_bytes(*borrow_amount),
            borrow_reserve: Pubkey::new_from_array(*borrow_reserve),
        })
    }

    fn pack_into_slice(&self, output: &mut [u8]) {
        let output = array_mut_ref![output, 0, OBLIGATION_LEN];
        let (
            created_at_slot,
            authority,
            collateral_amount,
            collateral_reserve,
            borrow_amount,
            borrow_reserve,
        ) = mut_array_refs![output, 8, 32, 8, 32, 8, 32];

        *created_at_slot = self.created_at_slot.to_le_bytes();
        authority.copy_from_slice(self.authority.as_ref());
        *collateral_amount = self.collateral_amount.to_le_bytes();
        collateral_reserve.copy_from_slice(self.collateral_reserve.as_ref());
        *borrow_amount = self.borrow_amount.to_le_bytes();
        borrow_reserve.copy_from_slice(self.borrow_reserve.as_ref());
    }
}
