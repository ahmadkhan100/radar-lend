use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
    program::{invoke, invoke_signed},
    system_instruction,
};
use borsh::{BorshDeserialize, BorshSerialize};

// Define the program ID
solana_program::declare_id!("Your_Program_ID_Here");

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct UserAccount {
    pub balance: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum ShrubFinanceInstruction {
    Deposit { amount: u64 },
    Withdraw { amount: u64 },
}

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = ShrubFinanceInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    match instruction {
        ShrubFinanceInstruction::Deposit { amount } => {
            msg!("Instruction: Deposit");
            deposit(program_id, accounts, amount)
        }
        ShrubFinanceInstruction::Withdraw { amount } => {
            msg!("Instruction: Withdraw");
            withdraw(program_id, accounts, amount)
        }
    }
}

fn deposit(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let user_account = next_account_info(account_info_iter)?;
    let user_wallet = next_account_info(account_info_iter)?;
    let system_program = next_account_info(account_info_iter)?;

    if !user_wallet.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if user_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    if amount == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(std::mem::size_of::<UserAccount>());

    if user_account.lamports() < required_lamports {
        msg!("Initializing user account");
        let lamports_to_transfer = required_lamports.saturating_sub(user_account.lamports());
        invoke(
            &system_instruction::transfer(user_wallet.key, user_account.key, lamports_to_transfer),
            &[user_wallet.clone(), user_account.clone(), system_program.clone()],
        )?;
    }

    let mut user_data = if user_account.data_len() > 0 {
        UserAccount::try_from_slice(&user_account.data.borrow())?
    } else {
        UserAccount { balance: 0 }
    };

    user_data.balance = user_data.balance.checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    user_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    invoke(
        &system_instruction::transfer(user_wallet.key, user_account.key, amount),
        &[user_wallet.clone(), user_account.clone(), system_program.clone()],
    )?;

    msg!("Deposited {} lamports", amount);
    msg!("New balance: {} lamports", user_data.balance);
    Ok(())
}

fn withdraw(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();

    let user_account = next_account_info(account_info_iter)?;
    let user_wallet = next_account_info(account_info_iter)?;

    if !user_wallet.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    if user_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    if amount == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }

    let mut user_data = UserAccount::try_from_slice(&user_account.data.borrow())?;
    if user_data.balance < amount {
        return Err(ProgramError::InsufficientFunds);
    }

    user_data.balance = user_data.balance.checked_sub(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    user_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    **user_account.try_borrow_mut_lamports()? = user_account.lamports()
        .checked_sub(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    **user_wallet.try_borrow_mut_lamports()? = user_wallet.lamports()
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    msg!("Withdrawn {} lamports", amount);
    msg!("New balance: {} lamports", user_data.balance);
    Ok(())
}
