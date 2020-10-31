//! Program state processor

use crate::{
    error::LendingError,
    instruction::LendingInstruction,
    state::{ObligationInfo, PoolInfo, ReserveInfo, MAX_RESERVES},
};
use num_traits::FromPrimitive;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    decode_error::DecodeError,
    entrypoint::ProgramResult,
    info,
    program::invoke_signed,
    program_error::{PrintProgramError, ProgramError},
    program_option::COption,
    program_pack::{IsInitialized, Pack},
    pubkey::Pubkey,
    sysvar::{clock::Clock, rent::Rent, Sysvar},
};
use serum_dex::state::{MarketState, strip_header, AccountFlag};
use serum_dex::critbit::{Slab,SlabView};
use safe_transmute::to_bytes::transmute_to_bytes;
use enumflags2::BitFlags;
use bytemuck::{Pod, Zeroable};
use std::cell::RefMut;

/// Program state handler.
pub struct Processor {}

impl Processor {
    /// Processes an instruction
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = LendingInstruction::unpack(input)?;
        match instruction {
            LendingInstruction::InitPool => {
                info!("Instruction: Init Pool");
                Self::process_init_pool(program_id, accounts)
            }
            LendingInstruction::InitReserve => {
                info!("Instruction: Init Reserve");
                Self::process_init_reserve(program_id, accounts)
            }
            LendingInstruction::Deposit { amount } => {
                info!("Instruction: Deposit");
                Self::process_deposit(program_id, amount, accounts)
            }
            LendingInstruction::Borrow {
                collateral_amount,
                obligation_authority,
            } => {
                info!("Instruction: Borrow");
                Self::process_borrow(
                    program_id,
                    collateral_amount,
                    obligation_authority,
                    accounts,
                )
            }
            LendingInstruction::SetPrice => {
                info!("Instruction: Set price");
                Self::process_set_price(accounts)
            }
        }
    }

    fn process_init_pool(_program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let pool_info = next_account_info(account_info_iter)?;
        let quote_token_mint_info = next_account_info(account_info_iter)?;
        let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
        let token_program_id = next_account_info(account_info_iter)?;

        let pool = PoolInfo::unpack_unchecked(&pool_info.data.borrow())?;
        if pool.is_initialized() {
            return Err(LendingError::AlreadyInUse.into());
        }
        if !rent.is_exempt(pool_info.lamports(), pool_info.data_len()) {
            return Err(LendingError::NotRentExempt.into());
        }

        if quote_token_mint_info.owner != token_program_id.key {
            return Err(LendingError::InvalidTokenProgram.into());
        }
        Self::unpack_mint(&quote_token_mint_info.data.borrow())?;

        let info = PoolInfo {
            is_initialized: true,
            quote_token_mint: *quote_token_mint_info.key,
            ..PoolInfo::default()
        };
        PoolInfo::pack(info, &mut pool_info.data.borrow_mut())?;

        Ok(())
    }

    fn process_init_reserve(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let reserve_info = next_account_info(account_info_iter)?;
        let pool_info = next_account_info(account_info_iter)?;
        let reserve_token_info = next_account_info(account_info_iter)?;
        let collateral_token_info = next_account_info(account_info_iter)?;
        let liquidity_token_mint_info = next_account_info(account_info_iter)?;
        let dex_market_info = next_account_info(account_info_iter)?;
        let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
        let token_program_id = next_account_info(account_info_iter)?;

        let reserve = ReserveInfo::unpack_unchecked(&reserve_info.data.borrow())?;
        if reserve.is_initialized() {
            return Err(LendingError::AlreadyInUse.into());
        }
        if !rent.is_exempt(reserve_info.lamports(), reserve_info.data_len()) {
            return Err(LendingError::NotRentExempt.into());
        }

        let mut pool = PoolInfo::unpack(&pool_info.data.borrow())?;
        if pool.num_reserves >= MAX_RESERVES {
            return Err(LendingError::PoolFull.into());
        }

        let bump_seed = Self::find_authority_bump_seed(program_id, &pool_info.key);
        let authority = Self::authority_id(program_id, pool_info.key, bump_seed)?;

        if reserve_token_info.owner != token_program_id.key {
            return Err(LendingError::InvalidTokenProgram.into());
        }
        if collateral_token_info.owner != token_program_id.key {
            return Err(LendingError::InvalidTokenProgram.into());
        }
        if liquidity_token_mint_info.owner != token_program_id.key {
            return Err(LendingError::InvalidTokenProgram.into());
        }

        let reserve_token = Self::unpack_token_account(&reserve_token_info.data.borrow())?;
        let collateral_token = Self::unpack_token_account(&collateral_token_info.data.borrow())?;
        let liquidity_mint = Self::unpack_mint(&liquidity_token_mint_info.data.borrow())?;

        if authority != reserve_token.owner {
            return Err(LendingError::InvalidOwner.into());
        }
        if authority != collateral_token.owner {
            return Err(LendingError::InvalidOwner.into());
        }
        if COption::Some(authority) != liquidity_mint.mint_authority {
            return Err(LendingError::InvalidOwner.into());
        }

        if &collateral_token.mint != liquidity_token_mint_info.key {
            return Err(LendingError::InvalidCollateral.into());
        }
        if collateral_token.mint == reserve_token.mint {
            return Err(LendingError::InvalidCollateral.into());
        }

        if reserve_token.close_authority.is_some() {
            return Err(LendingError::InvalidCloseAuthority.into());
        }
        if reserve_token.delegate.is_some() {
            return Err(LendingError::InvalidDelegate.into());
        }
        if collateral_token.close_authority.is_some() {
            return Err(LendingError::InvalidCloseAuthority.into());
        }
        if collateral_token.delegate.is_some() {
            return Err(LendingError::InvalidDelegate.into());
        }

        if liquidity_mint.freeze_authority.is_some() {
            return Err(LendingError::InvalidFreezeAuthority.into());
        }

        if !rent.is_exempt(dex_market_info.lamports(), dex_market_info.data_len()) {
            return Err(LendingError::NotRentExempt.into());
        }

        // TODO: check that market state is owned by real serum dex program
        let market = MarketState::load(dex_market_info, dex_market_info.owner)?;
        if &pool.quote_token_mint.to_bytes()[..] != transmute_to_bytes(&market.pc_mint) {
            return Err(LendingError::InvalidInput.into());
        }
        if &reserve_token.mint.to_bytes()[..] != transmute_to_bytes(&market.coin_mint) {
            return Err(LendingError::InvalidInput.into());
        }

        let info = ReserveInfo {
            is_initialized: true,
            pool: *pool_info.key,
            reserve: *reserve_token_info.key,
            collateral: *collateral_token_info.key,
            liquidity_token_mint: *liquidity_token_mint_info.key,
            dex_market: *dex_market_info.key,
            ..ReserveInfo::default()
        };
        ReserveInfo::pack(info, &mut reserve_info.data.borrow_mut())?;

        pool.reserves[pool.num_reserves as usize] = *reserve_info.key;
        pool.num_reserves = pool.num_reserves + 1;
        PoolInfo::pack(pool, &mut pool_info.data.borrow_mut())?;

        Ok(())
    }

    fn process_deposit(
        program_id: &Pubkey,
        amount: u64,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let reserve_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let source_token_info = next_account_info(account_info_iter)?;
        let destination_token_info = next_account_info(account_info_iter)?;
        let liquidity_token_info = next_account_info(account_info_iter)?;
        let liquidity_token_mint_info = next_account_info(account_info_iter)?;
        let token_program_id = next_account_info(account_info_iter)?;

        let reserve = ReserveInfo::unpack(&reserve_info.data.borrow())?;
        let bump_seed = Self::find_authority_bump_seed(program_id, &reserve.pool);
        if authority_info.key != &Self::authority_id(program_id, &reserve.pool, bump_seed)? {
            return Err(LendingError::InvalidProgramAddress.into());
        }

        if destination_token_info.key != &reserve.reserve
            || liquidity_token_mint_info.key != &reserve.liquidity_token_mint
        {
            return Err(LendingError::InvalidInput.into());
        }
        if destination_token_info.key == source_token_info.key {
            return Err(LendingError::InvalidInput.into());
        }
        if liquidity_token_info.key == &reserve.collateral {
            return Err(LendingError::InvalidInput.into());
        }

        Self::token_transfer(TokenTransferParams {
            source: source_token_info.clone(),
            destination: destination_token_info.clone(),
            amount,
            authority: authority_info.clone(),
            authorized: &reserve.pool,
            bump_seed,
            token_program: token_program_id.clone(),
        })?;

        Self::token_mint_to(TokenMintToParams {
            mint: liquidity_token_mint_info.clone(),
            destination: liquidity_token_info.clone(),
            amount,
            authority: authority_info.clone(),
            authorized: &reserve.pool,
            bump_seed,
            token_program: token_program_id.clone(),
        })?;

        Ok(())
    }

    fn process_borrow(
        program_id: &Pubkey,
        collateral_amount: u64,
        obligation_authority: Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let deposit_reserve_info = next_account_info(account_info_iter)?;
        let withdraw_reserve_info = next_account_info(account_info_iter)?;
        let authority_info = next_account_info(account_info_iter)?;
        let collateral_source_token_info = next_account_info(account_info_iter)?;
        let collateral_destination_token_info = next_account_info(account_info_iter)?;
        let borrow_source_token_info = next_account_info(account_info_iter)?;
        let borrow_destination_token_info = next_account_info(account_info_iter)?;
        let obligation_info = next_account_info(account_info_iter)?;
        let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;
        let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
        let token_program_id = next_account_info(account_info_iter)?;

        let obligation = ObligationInfo::unpack_unchecked(&obligation_info.data.borrow())?;
        if obligation.is_initialized() {
            return Err(LendingError::AlreadyInUse.into());
        }
        if !rent.is_exempt(obligation_info.lamports(), obligation_info.data_len()) {
            return Err(LendingError::NotRentExempt.into());
        }

        let deposit_reserve = ReserveInfo::unpack(&deposit_reserve_info.data.borrow())?;
        let withdraw_reserve = ReserveInfo::unpack(&withdraw_reserve_info.data.borrow())?;
        let deposit_reserve_liquidity_token =
            Self::unpack_token_account(&collateral_destination_token_info.data.borrow())?;
        if deposit_reserve.pool != withdraw_reserve.pool {
            return Err(LendingError::PoolMismatch.into());
        }
        if &withdraw_reserve.reserve != borrow_source_token_info.key {
            return Err(LendingError::InvalidInput.into());
        }
        if deposit_reserve.liquidity_token_mint != deposit_reserve_liquidity_token.owner {
            return Err(LendingError::InvalidInput.into());
        }

        let pool_key = &deposit_reserve.pool;
        let bump_seed = Self::find_authority_bump_seed(program_id, pool_key);

        Self::token_transfer(TokenTransferParams {
            source: collateral_source_token_info.clone(),
            destination: deposit_reserve_info.clone(),
            amount: collateral_amount,
            authority: authority_info.clone(),
            authorized: pool_key,
            bump_seed,
            token_program: token_program_id.clone(),
        })?;

        // TODO improve math
        let deposit_token_price = deposit_reserve.current_market_price(clock)? as u128;
        let withdraw_token_price = withdraw_reserve.current_market_price(clock)? as u128;
        let deposit_value = collateral_amount as u128 * deposit_token_price;
        let borrow_amount = (deposit_value / withdraw_token_price) as u64;

        Self::token_transfer(TokenTransferParams {
            source: borrow_source_token_info.clone(),
            destination: borrow_destination_token_info.clone(),
            amount: borrow_amount,
            authority: authority_info.clone(),
            authorized: pool_key,
            bump_seed,
            token_program: token_program_id.clone(),
        })?;

        ObligationInfo::pack(
            ObligationInfo {
                created_at_slot: clock.slot,
                authority: obligation_authority,
                collateral_amount,
                collateral_reserve: *deposit_reserve_info.key,
                borrow_amount,
                borrow_reserve: *withdraw_reserve_info.key,
            },
            &mut obligation_info.data.borrow_mut(),
        )?;

        Ok(())
    }

    fn process_set_price(
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let reserve_info = next_account_info(account_info_iter)?;
        let dex_market_info = next_account_info(account_info_iter)?;
        let dex_market_bids_info = next_account_info(account_info_iter)?;
        let dex_market_asks_info = next_account_info(account_info_iter)?;
        let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;

        let mut reserve = ReserveInfo::unpack(&reserve_info.data.borrow())?;
        if &reserve.dex_market != dex_market_info.key {
            return Err(LendingError::InvalidInput.into());
        }

        let market = MarketState::load(dex_market_info, dex_market_info.owner)?;
        if &dex_market_bids_info.key.to_bytes()[..] != transmute_to_bytes(&market.bids) {
            return Err(LendingError::InvalidInput.into());
        }
        if &dex_market_asks_info.key.to_bytes()[..] != transmute_to_bytes(&market.asks) {
            return Err(LendingError::InvalidInput.into());
        }

        #[derive(Copy, Clone)]
        #[repr(C)]
        struct OrderBookStateHeader {
            account_flags: u64, // Initialized, (Bids or Asks)
        }
        unsafe impl Zeroable for OrderBookStateHeader {}
        unsafe impl Pod for OrderBookStateHeader {}

        #[inline]
        fn load_bids<'a>(bids: &'a AccountInfo) -> Result<RefMut<'a, Slab>, ProgramError> {
            let (header, buf) = strip_header::<OrderBookStateHeader, u8>(bids, false)?;
            let flags = BitFlags::from_bits(header.account_flags).unwrap();
            if &flags != &(AccountFlag::Initialized | AccountFlag::Bids) {
                Err(LendingError::InvalidInput.into())
            } else {
                Ok(RefMut::map(buf, Slab::new))
            }
        }

        #[inline]
        fn load_asks<'a>(bids: &'a AccountInfo) -> Result<RefMut<'a, Slab>, ProgramError> {
            let (header, buf) = strip_header::<OrderBookStateHeader, u8>(bids, false)?;
            let flags = BitFlags::from_bits(header.account_flags).unwrap();
            if &flags != &(AccountFlag::Initialized | AccountFlag::Asks) {
                Err(LendingError::InvalidInput.into())
            } else {
                Ok(RefMut::map(buf, Slab::new))
            }
        }
    
        let mut bids = load_bids(dex_market_bids_info)?;
        let mut asks = load_asks(dex_market_asks_info)?;

        let max_bid = bids.find_max().ok_or_else(|| ProgramError::from(LendingError::InvalidInput))?;
        let min_ask = asks.find_min().ok_or_else(|| ProgramError::from(LendingError::InvalidInput))?;

        let best_bid_ref = bids
        .get_mut(max_bid)
        .unwrap()
        .as_leaf_mut()
        .unwrap();

        let best_ask_ref = asks
        .get_mut(min_ask)
        .unwrap()
        .as_leaf_mut()
        .unwrap();

        let best_bid: u64 = best_bid_ref.price().into();
        let best_ask: u64 = best_ask_ref.price().into();

        reserve.market_price = (best_bid + best_ask) / 2;
        reserve.market_price_updated_slot = clock.slot;
        ReserveInfo::pack(reserve, &mut reserve_info.data.borrow_mut())?;

        Ok(())
    }
    /// Generates seed bump for lending pool authorities
    fn find_authority_bump_seed(program_id: &Pubkey, my_info: &Pubkey) -> u8 {
        let (pubkey, bump_seed) =
            Pubkey::find_program_address(&[&my_info.to_bytes()[..32]], program_id);
        {
            let mut log_message: String = "Found authority ".to_string();
            log_message.push_str(&pubkey.to_string());
            log_message.push_str(" with bump seed ");
            log_message.push_str(&bump_seed.to_string());
            log_message.push_str(" for ");
            log_message.push_str(&my_info.to_string());
            info!(log_message.as_str());
        }
        bump_seed
    }

    /// Unpacks a spl_token `Account`.
    fn unpack_token_account(data: &[u8]) -> Result<spl_token::state::Account, LendingError> {
        spl_token::state::Account::unpack(data).map_err(|_| LendingError::ExpectedTokenAccount)
    }

    /// Unpacks a spl_token `Mint`.
    fn unpack_mint(data: &[u8]) -> Result<spl_token::state::Mint, LendingError> {
        spl_token::state::Mint::unpack(data).map_err(|_| LendingError::ExpectedTokenMint)
    }

    /// Calculates the authority id by generating a program address.
    fn authority_id(
        program_id: &Pubkey,
        my_info: &Pubkey,
        bump_seed: u8,
    ) -> Result<Pubkey, LendingError> {
        Pubkey::create_program_address(&[&my_info.to_bytes()[..32], &[bump_seed]], program_id)
            .or(Err(LendingError::InvalidProgramAddress))
    }

    /// Issue a spl_token `Transfer` instruction.
    fn token_transfer<'a, 'b>(params: TokenTransferParams<'a, 'b>) -> Result<(), ProgramError> {
        let authorized_bytes = params.authorized.to_bytes();
        let authority_signer_seeds = [&authorized_bytes[..32], &[params.bump_seed]];
        let TokenTransferParams {
            source,
            destination,
            authority,
            token_program,
            amount,
            ..
        } = params;
        let ix = spl_token::instruction::transfer(
            token_program.key,
            source.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?;
        invoke_signed(
            &ix,
            &[source, destination, authority, token_program],
            &[&authority_signer_seeds],
        )
    }

    /// Issue a spl_token `MintTo` instruction.
    fn token_mint_to<'a, 'b>(params: TokenMintToParams<'a, 'b>) -> Result<(), ProgramError> {
        let authorized_bytes = params.authorized.to_bytes();
        let authority_signer_seeds = [&authorized_bytes[..32], &[params.bump_seed]];
        let TokenMintToParams {
            mint,
            destination,
            authority,
            token_program,
            amount,
            ..
        } = params;
        let ix = spl_token::instruction::mint_to(
            token_program.key,
            mint.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?;
        invoke_signed(
            &ix,
            &[mint, destination, authority, token_program],
            &[&authority_signer_seeds],
        )
    }
}

struct TokenTransferParams<'a: 'b, 'b> {
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    amount: u64,
    authority: AccountInfo<'a>,
    authorized: &'b Pubkey,
    bump_seed: u8,
    token_program: AccountInfo<'a>,
}

struct TokenMintToParams<'a: 'b, 'b> {
    mint: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    amount: u64,
    authority: AccountInfo<'a>,
    authorized: &'b Pubkey,
    bump_seed: u8,
    token_program: AccountInfo<'a>,
}

impl PrintProgramError for LendingError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        info!(self.to_string().as_str());
    }
}
