use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::{clock::Clock, Sysvar},
    program::{invoke, invoke_signed},
    system_instruction,
};
use spl_token::instruction as token_instruction;
use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

// Define the program ID
solana_program::declare_id!("Your_Program_ID_Here");

// Constants
const SOL_PRICE: u64 = 150;  // $150 per SOL
const LTV: u64 = 25;  // 25% LTV
const USDC_DECIMALS: u8 = 6;
const USDC_MINT: Pubkey = solana_program::pubkey!("Your_USDC_Mint_Address_Here");
const PROGRAM_USDC_ACCOUNT: Pubkey = solana_program::pubkey!("Your_Program_USDC_Account_Here");

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct LoanAccount {
    pub borrower: Pubkey,
    pub start_date: i64,
    pub principal: u64,
    pub apy: u64,
    pub collateral: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum LoanInstruction {
    InitializeLoan { amount: u64, apy: u64 },
    RepayLoan { amount: u64 },
    LiquidateLoan,
}

#[derive(Error, Debug)]
pub enum LoanError {
    #[error("Invalid instruction")]
    InvalidInstruction,

    #[error("Not rent exempt")]
    NotRentExempt,

    #[error("Invalid loan amount")]
    InvalidLoanAmount,

    #[error("Insufficient collateral")]
    InsufficientCollateral,

    #[error("Arithmetic overflow")]
    Overflow,

    #[error("Insufficient repayment amount")]
    InsufficientRepaymentAmount,

    #[error("Loan is not underwater")]
    LoanNotUnderwater,
}

impl From<LoanError> for ProgramError {
    fn from(e: LoanError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = LoanInstruction::try_from_slice(instruction_data)
        .map_err(|_| LoanError::InvalidInstruction)?;

    match instruction {
        LoanInstruction::InitializeLoan { amount, apy } => {
            initialize_loan(program_id, accounts, amount, apy)
        }
        LoanInstruction::RepayLoan { amount } => repay_loan(accounts, amount),
        LoanInstruction::LiquidateLoan => liquidate_loan(accounts),
    }
}

fn initialize_loan(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
    apy: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let borrower = next_account_info(account_info_iter)?;
    let loan_account = next_account_info(account_info_iter)?;
    let borrower_usdc_account = next_account_info(account_info_iter)?;
    let program_usdc_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;

    if !borrower.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if amount == 0 {
        return Err(LoanError::InvalidLoanAmount.into());
    }

    // Calculate required collateral
    let required_collateral = (amount * 100) / (SOL_PRICE * LTV);

    // Create loan account
    let (pda, bump_seed) = Pubkey::find_program_address(&[borrower.key.as_ref(), b"loan"], program_id);
    if pda != *loan_account.key {
        return Err(ProgramError::InvalidAccountData);
    }

    if !rent.is_exempt(loan_account.lamports(), loan_account.data_len()) {
        return Err(LoanError::NotRentExempt.into());
    }

    let space = std::mem::size_of::<LoanAccount>();
    let rent_lamports = rent.minimum_balance(space);

    invoke_signed(
        &system_instruction::create_account(
            borrower.key,
            loan_account.key,
            rent_lamports,
            space as u64,
            program_id,
        ),
        &[borrower.clone(), loan_account.clone(), system_program.clone()],
        &[&[borrower.key.as_ref(), b"loan", &[bump_seed]]],
    )?;

    // Transfer SOL collateral
    invoke(
        &system_instruction::transfer(borrower.key, loan_account.key, required_collateral),
        &[borrower.clone(), loan_account.clone(), system_program.clone()],
    )?;

    // Transfer USDC to borrower
    invoke(
        &token_instruction::transfer(
            token_program.key,
            program_usdc_account.key,
            borrower_usdc_account.key,
            &program_id,
            &[],
            amount,
        )?,
        &[program_usdc_account.clone(), borrower_usdc_account.clone(), token_program.clone()],
    )?;

    // Initialize loan account data
    let loan_data = LoanAccount {
        borrower: *borrower.key,
        start_date: clock.unix_timestamp,
        principal: amount,
        apy,
        collateral: required_collateral,
    };
    loan_data.serialize(&mut &mut loan_account.data.borrow_mut()[..])?;

    msg!("Loan initialized: {} USDC borrowed against {} SOL", amount, required_collateral);
    Ok(())
}

fn repay_loan(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let borrower = next_account_info(account_info_iter)?;
    let loan_account = next_account_info(account_info_iter)?;
    let borrower_usdc_account = next_account_info(account_info_iter)?;
    let program_usdc_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;

    if !borrower.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let mut loan_data = LoanAccount::try_from_slice(&loan_account.data.borrow())?;
    if loan_data.borrower != *borrower.key {
        return Err(ProgramError::InvalidAccountData);
    }

    // Calculate interest
    let time_elapsed = (clock.unix_timestamp - loan_data.start_date) as u64;
    let interest = (loan_data.principal * loan_data.apy * time_elapsed) / (365 * 24 * 60 * 60 * 100);
    let total_due = loan_data.principal.checked_add(interest).ok_or(LoanError::Overflow)?;

    if amount < total_due {
        return Err(LoanError::InsufficientRepaymentAmount.into());
    }

    // Transfer USDC from borrower to program
    invoke(
        &token_instruction::transfer(
            token_program.key,
            borrower_usdc_account.key,
            program_usdc_account.key,
            borrower.key,
            &[],
            amount,
        )?,
        &[borrower_usdc_account.clone(), program_usdc_account.clone(), borrower.clone(), token_program.clone()],
    )?;

    // Return collateral to borrower
    **loan_account.try_borrow_mut_lamports()? = loan_account.lamports()
        .checked_sub(loan_data.collateral)
        .ok_or(ProgramError::InsufficientFunds)?;
    **borrower.try_borrow_mut_lamports()? = borrower.lamports()
        .checked_add(loan_data.collateral)
        .ok_or(LoanError::Overflow)?;

    // Close loan account
    loan_account.assign(system_program::id());
    loan_account.realloc(0, false)?;

    msg!("Loan repaid: {} USDC. Collateral returned: {} SOL", amount, loan_data.collateral);
    Ok(())
}

fn liquidate_loan(accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let liquidator = next_account_info(account_info_iter)?;
    let loan_account = next_account_info(account_info_iter)?;
    let liquidator_usdc_account = next_account_info(account_info_iter)?;
    let program_usdc_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let clock = &Clock::from_account_info(next_account_info(account_info_iter)?)?;

    if !liquidator.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let loan_data = LoanAccount::try_from_slice(&loan_account.data.borrow())?;

    // Calculate current loan value
    let time_elapsed = (clock.unix_timestamp - loan_data.start_date) as u64;
    let interest = (loan_data.principal * loan_data.apy * time_elapsed) / (365 * 24 * 60 * 60 * 100);
    let total_due = loan_data.principal.checked_add(interest).ok_or(LoanError::Overflow)?;

    // Check if loan is underwater
    let current_collateral_value = (loan_data.collateral * SOL_PRICE) / 100;
    if current_collateral_value >= total_due {
        return Err(LoanError::LoanNotUnderwater.into());
    }

    // Transfer USDC from liquidator to program
    invoke(
        &token_instruction::transfer(
            token_program.key,
            liquidator_usdc_account.key,
            program_usdc_account.key,
            liquidator.key,
            &[],
            total_due,
        )?,
        &[liquidator_usdc_account.clone(), program_usdc_account.clone(), liquidator.clone(), token_program.clone()],
    )?;

    // Transfer collateral to liquidator
    **loan_account.try_borrow_mut_lamports()? = loan_account.lamports()
        .checked_sub(loan_data.collateral)
        .ok_or(ProgramError::InsufficientFunds)?;
    **liquidator.try_borrow_mut_lamports()? = liquidator.lamports()
        .checked_add(loan_data.collateral)
        .ok_or(LoanError::Overflow)?;

    // Close loan account
    loan_account.assign(system_program::id());
    loan_account.realloc(0, false)?;

    msg!("Loan liquidated. Collateral transferred: {} SOL", loan_data.collateral);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::clock::Epoch;
    use std::mem;

    // Helper function to create AccountInfo for testing
    fn create_account_info<'a>(
        key: &'a Pubkey,
        is_signer: bool,
        lamports: &'a mut u64,
        data: &'a mut [u8],
        owner: &'a Pubkey,
    ) -> AccountInfo<'a> {
        AccountInfo::new(
            key,
            is_signer,
            false,
            lamports,
            data,
            owner,
            false,
            Epoch::default(),
        )
    }

    #[test]
    fn test_initialize_loan() {
        let program_id = Pubkey::new_unique();
        let borrower_key = Pubkey::new_unique();
        let loan_account_key = Pubkey::new_unique();
        let usdc_mint_key = Pubkey::new_unique();
        let borrower_usdc_account_key = Pubkey::new_unique();
        let program_usdc_account_key = Pubkey::new_unique();

        let mut borrower_lamports = 1000000000; // 10 SOL
        let mut loan_account_lamports = 0;
        let mut borrower_usdc_lamports = 1000000; // 1 USDC
        let mut program_usdc_lamports = 1000000000; // 1000 USDC

        let mut loan_account_data = vec![0; mem::size_of::<LoanAccount>()];
        let mut borrower_usdc_data = vec![0; 165]; // Mocked SPL Token account data
        let mut program_usdc_data = vec![0; 165]; // Mocked SPL Token account data

        let borrower_account = create_account_info(&borrower_key, true, &mut borrower_lamports, &mut [], &program_id);
        let loan_account = create_account_info(&loan_account_key, false, &mut loan_account_lamports, &mut loan_account_data, &program_id);
        let borrower_usdc_account = create_account_info(&borrower_usdc_account_key, false, &mut borrower_usdc_lamports, &mut borrower_usdc_data, &usdc_mint_key);
        let program_usdc_account = create_account_info(&program_usdc_account_key, false, &mut program_usdc_lamports, &mut program_usdc_data, &usdc_mint_key);

        let system_program_key = Pubkey::new_unique();
        let token_program_key = Pubkey::new_unique();
        let rent_key = Pubkey::new_unique();
        let clock_key = Pubkey::new_unique();

        let accounts = vec![
            borrower_account, loan_account,
            borrower_usdc_account,
            program_usdc_account,
            create_account_info(&system_program_key, false, &mut 0, &mut [], &program_id),
            create_account_info(&token_program_key, false, &mut 0, &mut [], &program_id),
            create_account_info(&rent_key, false, &mut 0, &mut [], &program_id),
            create_account_info(&clock_key, false, &mut 0, &mut [], &program_id),
        ];

        let amount = 100_000_000; // 100 USDC
        let apy = 500; // 5% APY

        let instruction_data = LoanInstruction::InitializeLoan { amount, apy }.try_to_vec().unwrap();

        // Mock Rent and Clock sysvars
        let rent = Rent {
            lamports_per_byte_year: 1,
            exemption_threshold: 2.0,
            burn_percent: 5,
        };
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: 1625097600, // Example timestamp
        };

        // Override the Rent and Clock account data
        accounts[6].data = rent.try_to_vec().unwrap().into();
        accounts[7].data = clock.try_to_vec().unwrap().into();

        // Process the instruction
        process_instruction(&program_id, &accounts, &instruction_data).unwrap();

        // Verify the loan account was created and initialized correctly
        let loan_data = LoanAccount::try_from_slice(&loan_account.data.borrow()).unwrap();
        assert_eq!(loan_data.borrower, borrower_key);
        assert_eq!(loan_data.principal, amount);
        assert_eq!(loan_data.apy, apy);
        assert_eq!(loan_data.start_date, clock.unix_timestamp);

        // Verify the collateral was transferred
        let expected_collateral = (amount * 100) / (SOL_PRICE * LTV);
        assert_eq!(loan_data.collateral, expected_collateral);
        assert_eq!(borrower_account.lamports(), 1000000000 - expected_collateral);
        assert_eq!(loan_account.lamports(), expected_collateral);

        // In a real test, we would also verify the USDC transfer, but we've mocked the token accounts here
    }

    #[test]
    fn test_repay_loan() {
        // Similar setup to test_initialize_loan
        let program_id = Pubkey::new_unique();
        let borrower_key = Pubkey::new_unique();
        let loan_account_key = Pubkey::new_unique();
        let usdc_mint_key = Pubkey::new_unique();
        let borrower_usdc_account_key = Pubkey::new_unique();
        let program_usdc_account_key = Pubkey::new_unique();

        let mut borrower_lamports = 900000000; // 9 SOL (after collateral deposit)
        let mut loan_account_lamports = 100000000; // 1 SOL collateral
        let mut borrower_usdc_lamports = 1100000000; // 1100 USDC (after loan)
        let mut program_usdc_lamports = 900000000; // 900 USDC (after loan)

        let mut loan_account_data = LoanAccount {
            borrower: borrower_key,
            start_date: 1625097600, // Example start timestamp
            principal: 100000000, // 100 USDC
            apy: 500, // 5% APY
            collateral: 100000000, // 1 SOL
        }.try_to_vec().unwrap();

        let mut borrower_usdc_data = vec![0; 165]; // Mocked SPL Token account data
        let mut program_usdc_data = vec![0; 165]; // Mocked SPL Token account data

        let borrower_account = create_account_info(&borrower_key, true, &mut borrower_lamports, &mut [], &program_id);
        let loan_account = create_account_info(&loan_account_key, false, &mut loan_account_lamports, &mut loan_account_data, &program_id);
        let borrower_usdc_account = create_account_info(&borrower_usdc_account_key, false, &mut borrower_usdc_lamports, &mut borrower_usdc_data, &usdc_mint_key);
        let program_usdc_account = create_account_info(&program_usdc_account_key, false, &mut program_usdc_lamports, &mut program_usdc_data, &usdc_mint_key);

        let token_program_key = Pubkey::new_unique();
        let clock_key = Pubkey::new_unique();

        let accounts = vec![
            borrower_account,
            loan_account,
            borrower_usdc_account,
            program_usdc_account,
            create_account_info(&token_program_key, false, &mut 0, &mut [], &program_id),
            create_account_info(&clock_key, false, &mut 0, &mut [], &program_id),
        ];

        let repay_amount = 105000000; // 105 USDC (principal + interest)

        let instruction_data = LoanInstruction::RepayLoan { amount: repay_amount }.try_to_vec().unwrap();

        // Mock Clock sysvar
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: 1625184000, // 1 day later
        };

        // Override the Clock account data
        accounts[5].data = clock.try_to_vec().unwrap().into();

        // Process the instruction
        process_instruction(&program_id, &accounts, &instruction_data).unwrap();

        // Verify the loan was repaid
        assert_eq!(borrower_account.lamports(), 1000000000); // 10 SOL (collateral returned)
        assert_eq!(loan_account.lamports(), 0);
        
        // In a real test, we would also verify the USDC transfer, but we've mocked the token accounts here
    }

    #[test]
    fn test_liquidate_loan() {
        // Similar setup to previous tests
        let program_id = Pubkey::new_unique();
        let borrower_key = Pubkey::new_unique();
        let liquidator_key = Pubkey::new_unique();
        let loan_account_key = Pubkey::new_unique();
        let usdc_mint_key = Pubkey::new_unique();
        let liquidator_usdc_account_key = Pubkey::new_unique();
        let program_usdc_account_key = Pubkey::new_unique();

        let mut liquidator_lamports = 1000000000; // 10 SOL
        let mut loan_account_lamports = 100000000; // 1 SOL collateral
        let mut liquidator_usdc_lamports = 1000000000; // 1000 USDC
        let mut program_usdc_lamports = 900000000; // 900 USDC

        let mut loan_account_data = LoanAccount {
            borrower: borrower_key,
            start_date: 1625097600, // Example start timestamp
            principal: 100000000, // 100 USDC
            apy: 500, // 5% APY
            collateral: 100000000, // 1 SOL
        }.try_to_vec().unwrap();

        let mut liquidator_usdc_data = vec![0; 165]; // Mocked SPL Token account data
        let mut program_usdc_data = vec![0; 165]; // Mocked SPL Token account data

        let liquidator_account = create_account_info(&liquidator_key, true, &mut liquidator_lamports, &mut [], &program_id);
        let loan_account = create_account_info(&loan_account_key, false, &mut loan_account_lamports, &mut loan_account_data, &program_id);
        let liquidator_usdc_account = create_account_info(&liquidator_usdc_account_key, false, &mut liquidator_usdc_lamports, &mut liquidator_usdc_data, &usdc_mint_key);
        let program_usdc_account = create_account_info(&program_usdc_account_key, false, &mut program_usdc_lamports, &mut program_usdc_data, &usdc_mint_key);

        let token_program_key = Pubkey::new_unique();
        let clock_key = Pubkey::new_unique();

        let accounts = vec![
            liquidator_account,
            loan_account,
            liquidator_usdc_account,
            program_usdc_account,
            create_account_info(&token_program_key, false, &mut 0, &mut [], &program_id),
            create_account_info(&clock_key, false, &mut 0, &mut [], &program_id),
        ];

        let instruction_data = LoanInstruction::LiquidateLoan.try_to_vec().unwrap();

        // Mock Clock sysvar
        let clock = Clock {
            slot: 0,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: 1625270400, // 2 days later
        };

        // Override the Clock account data
        accounts[5].data = clock.try_to_vec().unwrap().into();

        // Simulate price drop
        const SOL_PRICE: u64 = 100;  // $100 per SOL (price dropped)

        // Process the instruction
        process_instruction(&program_id, &accounts, &instruction_data).unwrap();

        // Verify the loan was liquidated
        assert_eq!(liquidator_account.lamports(), 1100000000); // 11 SOL (initial + collateral)
        assert_eq!(loan_account.lamports(), 0);
        
        // In a real test, we would also verify the USDC transfer, but we've mocked the token accounts here
    }
}
