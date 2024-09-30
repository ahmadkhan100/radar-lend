use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    pubkey::Pubkey,
    msg,
    program::{invoke, invoke_signed},
    system_instruction,
    sysvar::{rent::Rent, Sysvar},
    clock::Clock,
};
use spl_token::instruction as token_instruction;
use borsh::{BorshDeserialize, BorshSerialize};

// Define the program ID
solana_program::declare_id!("Your_Program_ID_Here");

// Hard-coded values
const SOL_PRICE: u64 = 150;  // $150 per SOL
const LTV: u64 = 25;  // 25% LTV
const USDC_MINT: Pubkey = solana_program::pubkey!("Your_USDC_Mint_Address_Here");
const USDC_DECIMALS: u8 = 6;
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

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = LoanInstruction::try_from_slice(instruction_data)?;

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
    let usdc_mint = next_account_info(account_info_iter)?;
    let borrower_usdc_account = next_account_info(account_info_iter)?;
    let program_usdc_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;
    let rent_sysvar = next_account_info(account_info_iter)?;

    // Verify USDC mint
    if usdc_mint.key != &USDC_MINT {
        return Err(solana_program::program_error::ProgramError::InvalidAccountData);
    }

    // Calculate required collateral
    let required_collateral = (amount * 100) / (SOL_PRICE * LTV);

    // Create loan account
    let (pda, bump_seed) = Pubkey::find_program_address(&[borrower.key.as_ref(), b"loan"], program_id);
    if pda != *loan_account.key {
        return Err(solana_program::program_error::ProgramError::InvalidAccountData);
    }

    let rent = Rent::from_account_info(rent_sysvar)?;
    let space = std::mem::size_of::<LoanAccount>();
    let lamports = rent.minimum_balance(space);

    invoke_signed(
        &system_instruction::create_account(
            borrower.key,
            loan_account.key,
            lamports,
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
        start_date: Clock::get()?.unix_timestamp,
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

    let mut loan_data = LoanAccount::try_from_slice(&loan_account.data.borrow())?;
    if loan_data.borrower != *borrower.key {
        return Err(solana_program::program_error::ProgramError::InvalidAccountData);
    }

    // Calculate interest
    let current_time = Clock::get()?.unix_timestamp;
    let time_elapsed = (current_time - loan_data.start_date) as u64;
    let interest = (loan_data.principal * loan_data.apy * time_elapsed) / (365 * 24 * 60 * 60 * 100);
    let total_due = loan_data.principal + interest;

    if amount > total_due {
        return Err(solana_program::program_error::ProgramError::InvalidArgument);
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

    // Update loan data
    loan_data.principal = total_due - amount;
    if loan_data.principal == 0 {
        // Loan fully repaid, return collateral
        **loan_account.try_borrow_mut_lamports()? -= loan_data.collateral;
        **borrower.try_borrow_mut_lamports()? += loan_data.collateral;
        msg!("Loan fully repaid. Collateral returned: {} SOL", loan_data.collateral);
    } else {
        loan_data.serialize(&mut &mut loan_account.data.borrow_mut()[..])?;
        msg!("Loan partially repaid. Remaining principal: {} USDC", loan_data.principal);
    }

    Ok(())
}

fn liquidate_loan(accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let liquidator = next_account_info(account_info_iter)?;
    let loan_account = next_account_info(account_info_iter)?;
    let liquidator_usdc_account = next_account_info(account_info_iter)?;
    let program_usdc_account = next_account_info(account_info_iter)?;
    let token_program = next_account_info(account_info_iter)?;

    let loan_data = LoanAccount::try_from_slice(&loan_account.data.borrow())?;

    // Check if loan is underwater
    let current_collateral_value = loan_data.collateral * SOL_PRICE / 100;
    if current_collateral_value >= loan_data.principal {
        return Err(solana_program::program_error::ProgramError::InvalidAccountData);
    }

    // Transfer USDC from liquidator to program
    invoke(
        &token_instruction::transfer(
            token_program.key,
            liquidator_usdc_account.key,
            program_usdc_account.key,
            liquidator.key,
            &[],
            loan_data.principal,
        )?,
        &[liquidator_usdc_account.clone(), program_usdc_account.clone(), liquidator.clone(), token_program.clone()],
    )?;

    // Transfer collateral to liquidator
    **loan_account.try_borrow_mut_lamports()? -= loan_data.collateral;
    **liquidator.try_borrow_mut_lamports()? += loan_data.collateral;

    msg!("Loan liquidated. Collateral transferred: {} SOL", loan_data.collateral);
    Ok(())
}
