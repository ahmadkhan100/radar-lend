// File: deposit_program/src/lib.rs

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    declare_id,
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program::invoke,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{rent::Rent, Sysvar},
};

// Define the program ID (Replace with your actual program ID)
declare_id!("CkqWjTWzRMAtYN3CSs8Gp4K9H891htmaN1ysNXqcULc8");

// Error definitions
#[derive(Error, Debug, Copy, Clone)]
pub enum DepositError {
    /// Invalid instruction
    #[error("Invalid Instruction")]
    InvalidInstruction,

    /// Not rent exempt
    #[error("Not Rent Exempt")]
    NotRentExempt,

    /// Insufficient funds
    #[error("Insufficient Funds")]
    InsufficientFunds,

    /// Amount overflow
    #[error("Amount Overflow")]
    AmountOverflow,

    /// Unauthorized access
    #[error("Unauthorized Access")]
    Unauthorized,
}

impl From<DepositError> for ProgramError {
    fn from(e: DepositError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

// Instruction definitions
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum DepositInstruction {
    /// Initializes a new user account
    InitializeAccount,

    /// Deposits lamports into the user account
    Deposit { amount: u64 },

    /// Withdraws lamports from the user account
    Withdraw { amount: u64 },
}

// Account data structure
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct UserAccount {
    /// The owner of the account
    pub owner: Pubkey,

    /// The balance of lamports in the account
    pub balance: u64,
}

// Program entrypoint
entrypoint!(process_instruction);

// Program entrypoint's implementation
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    // Deserialize instruction data
    let instruction = DepositInstruction::try_from_slice(instruction_data)
        .map_err(|_| DepositError::InvalidInstruction)?;

    match instruction {
        DepositInstruction::InitializeAccount => {
            initialize_account(program_id, accounts)
        }
        DepositInstruction::Deposit { amount } => {
            deposit(program_id, accounts, amount)
        }
        DepositInstruction::Withdraw { amount } => {
            withdraw(program_id, accounts, amount)
        }
    }
}

// Instruction handlers

/// Handles InitializeAccount instruction
fn initialize_account(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // Get accounts
    let user = next_account_info(account_info_iter)?;
    let user_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;
    let rent_sysvar = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(rent_sysvar)?;

    // Check that the user signed the transaction
    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Calculate required space and lamports
    let space = std::mem::size_of::<UserAccount>();
    let lamports = rent.minimum_balance(space);

    // Create the user account (program-owned account)
    invoke(
        &solana_program::system_instruction::create_account(
            user.key,
            user_account.key,
            lamports,
            space as u64,
            program_id,
        ),
        &[
            user.clone(),
            user_account.clone(),
            system_program.clone(),
        ],
    )?;

    // Initialize UserAccount data
    let user_account_data = UserAccount {
        owner: *user.key,
        balance: 0,
    };

    // Serialize the user account data into the account's data field
    user_account_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    msg!("User account initialized for {}", user.key);

    Ok(())
}

/// Handles Deposit instruction
fn deposit(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // Get accounts
    let user = next_account_info(account_info_iter)?;
    let user_account = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    // Check that the user signed the transaction
    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Check that the user_account is owned by the program
    if user_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Deserialize UserAccount data
    let mut user_account_data =
        UserAccount::try_from_slice(&user_account.data.borrow())?;

    // Verify the account owner
    if user_account_data.owner != *user.key {
        return Err(DepositError::Unauthorized.into());
    }

    // Transfer lamports from user to user_account
    invoke(
        &solana_program::system_instruction::transfer(
            user.key,
            user_account.key,
            amount,
        ),
        &[
            user.clone(),
            user_account.clone(),
            system_program.clone(),
        ],
    )?;

    // Update the user's balance
    user_account_data.balance = user_account_data.balance.checked_add(amount)
        .ok_or(DepositError::AmountOverflow)?;

    // Serialize the updated data back into the account
    user_account_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    msg!(
        "{} deposited {} lamports",
        user.key,
        amount
    );

    Ok(())
}

/// Handles Withdraw instruction
fn withdraw(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    // Get accounts
    let user = next_account_info(account_info_iter)?;
    let user_account = next_account_info(account_info_iter)?;
    let _system_program = next_account_info(account_info_iter)?;

    // Check that the user signed the transaction
    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Check that the user_account is owned by the program
    if user_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Deserialize UserAccount data
    let mut user_account_data =
        UserAccount::try_from_slice(&user_account.data.borrow())?;

    // Verify the account owner
    if user_account_data.owner != *user.key {
        return Err(DepositError::Unauthorized.into());
    }

    // Check if the user has sufficient balance
    if user_account_data.balance < amount {
        return Err(DepositError::InsufficientFunds.into());
    }

    // Transfer lamports from user_account back to user
    **user_account.try_borrow_mut_lamports()? = user_account
        .lamports()
        .checked_sub(amount)
        .ok_or(DepositError::AmountOverflow)?;

    **user.try_borrow_mut_lamports()? = user
        .lamports()
        .checked_add(amount)
        .ok_or(DepositError::AmountOverflow)?;

    // Update the user's balance
    user_account_data.balance = user_account_data.balance.checked_sub(amount)
        .ok_or(DepositError::AmountOverflow)?;

    // Serialize the updated data back into the account
    user_account_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    msg!(
        "{} withdrew {} lamports",
        user.key,
        amount
    );

    Ok(())
}
