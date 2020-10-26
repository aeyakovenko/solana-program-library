//! Program state processor

use crate::{error::LendingError, instruction::LendingInstruction, state::ReserveInfo};
use num_traits::FromPrimitive;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    decode_error::DecodeError,
    entrypoint::ProgramResult,
    info,
    program::invoke_signed,
    program_error::{PrintProgramError, ProgramError},
    program_option::COption,
    program_pack::Pack,
    pubkey::Pubkey,
    sysvar::{rent::Rent, Sysvar},
};

/// Program state handler.
pub struct Processor {}

impl Processor {
    /// Processes an instruction
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = LendingInstruction::unpack(input)?;
        match instruction {
            LendingInstruction::InitReserve { authority } => {
                info!("Instruction: Init Reserve");
                Self::process_init_reserve(program_id, authority, accounts)
            }
            LendingInstruction::Deposit { amount } => {
                info!("Instruction: Deposit");
                Self::process_deposit(program_id, amount, accounts)
            }
        }
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
        let bump_seed = Self::find_authority_bump_seed(program_id, &reserve_info.key);
        if authority_info.key != &Self::authority_id(program_id, reserve_info.key, bump_seed)? {
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
            authorized: reserve_info.key,
            bump_seed,
            token_program: token_program_id.clone(),
        })?;

        Self::token_mint_to(TokenMintToParams {
            mint: liquidity_token_mint_info.clone(),
            destination: liquidity_token_info.clone(),
            amount,
            authority: authority_info.clone(),
            authorized: reserve_info.key,
            bump_seed,
            token_program: token_program_id.clone(),
        })?;

        Ok(())
    }

    fn process_init_reserve(
        program_id: &Pubkey,
        authority: Pubkey,
        accounts: &[AccountInfo],
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();
        let reserve_info = next_account_info(account_info_iter)?;
        let reserve_token_info = next_account_info(account_info_iter)?;
        let collateral_token_info = next_account_info(account_info_iter)?;
        let liquidity_token_mint_info = next_account_info(account_info_iter)?;
        let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
        let token_program_id = next_account_info(account_info_iter)?;

        let reserve = ReserveInfo::unpack_unchecked(&reserve_info.data.borrow())?;
        if reserve.is_initialized {
            return Err(LendingError::AlreadyInUse.into());
        }

        if !rent.is_exempt(reserve_info.lamports(), reserve_info.data_len()) {
            return Err(LendingError::NotRentExempt.into());
        }

        let bump_seed = Self::find_authority_bump_seed(program_id, &reserve_info.key);
        if authority != Self::authority_id(program_id, reserve_info.key, bump_seed)? {
            return Err(LendingError::InvalidProgramAddress.into());
        }

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

        let info = ReserveInfo {
            is_initialized: true,
            bump_seed,
            reserve: *reserve_token_info.key,
            collateral: *collateral_token_info.key,
            liquidity_token_mint: *liquidity_token_mint_info.key,
        };
        ReserveInfo::pack(info, &mut reserve_info.data.borrow_mut())?;

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
    fn token_transfer<'a>(params: TokenTransferParams<'a>) -> Result<(), ProgramError> {
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
    fn token_mint_to<'a>(params: TokenMintToParams<'a>) -> Result<(), ProgramError> {
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

struct TokenTransferParams<'a> {
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    amount: u64,
    authority: AccountInfo<'a>,
    authorized: &'a Pubkey,
    bump_seed: u8,
    token_program: AccountInfo<'a>,
}

struct TokenMintToParams<'a> {
    mint: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    amount: u64,
    authority: AccountInfo<'a>,
    authorized: &'a Pubkey,
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
