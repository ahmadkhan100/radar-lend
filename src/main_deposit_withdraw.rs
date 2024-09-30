use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

// Define the program ID
solana_program::declare_id!("Your_Program_ID_Here");

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct UserAccount {
    pub owner: Pubkey,
    pub balance: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum DepositWithdrawInstruction {
    InitializeAccount,
    Deposit { amount: u64 },
    Withdraw { amount: u64 },
}

#[derive(Error, Debug)]
pub enum DepositWithdrawError {
    #[error("Invalid instruction")]
    InvalidInstruction,

    #[error("Not rent exempt")]
    NotRentExempt,

    #[error("Expected amount to be greater than zero")]
    AmountMustBeGreaterThanZero,

    #[error("Insufficient balance for withdrawal")]
    InsufficientBalance,

    #[error("Arithmetic overflow")]
    Overflow,
}

impl From<DepositWithdrawError> for ProgramError {
    fn from(e: DepositWithdrawError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = DepositWithdrawInstruction::try_from_slice(instruction_data)
        .map_err(|_| DepositWithdrawError::InvalidInstruction)?;

    match instruction {
        DepositWithdrawInstruction::InitializeAccount => initialize_account(program_id, accounts),
        DepositWithdrawInstruction::Deposit { amount } => deposit(accounts, amount),
        DepositWithdrawInstruction::Withdraw { amount } => withdraw(accounts, amount),
    }
}

fn initialize_account(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let user_account = next_account_info(account_info_iter)?;
    let user = next_account_info(account_info_iter)?;
    let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;

    if !rent.is_exempt(user_account.lamports(), user_account.data_len()) {
        return Err(DepositWithdrawError::NotRentExempt.into());
    }

    if user_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let mut account_data = UserAccount::try_from_slice(&user_account.data.borrow())?;
    account_data.owner = *user.key;
    account_data.balance = 0;
    account_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    msg!("Account initialized");
    Ok(())
}

fn deposit(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let user_account = next_account_info(account_info_iter)?;
    let user = next_account_info(account_info_iter)?;

    if amount == 0 {
        return Err(DepositWithdrawError::AmountMustBeGreaterThanZero.into());
    }

    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let mut account_data = UserAccount::try_from_slice(&user_account.data.borrow())?;

    if account_data.owner != *user.key {
        return Err(ProgramError::InvalidAccountData);
    }

    account_data.balance = account_data.balance.checked_add(amount)
        .ok_or(DepositWithdrawError::Overflow)?;

    account_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    **user.try_borrow_mut_lamports()? = user.lamports()
        .checked_sub(amount)
        .ok_or(ProgramError::InsufficientFunds)?;

    **user_account.try_borrow_mut_lamports()? = user_account.lamports()
        .checked_add(amount)
        .ok_or(DepositWithdrawError::Overflow)?;

    msg!("Deposit successful: {} lamports", amount);
    Ok(())
}

fn withdraw(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let account_info_iter = &mut accounts.iter();
    let user_account = next_account_info(account_info_iter)?;
    let user = next_account_info(account_info_iter)?;

    if amount == 0 {
        return Err(DepositWithdrawError::AmountMustBeGreaterThanZero.into());
    }

    if !user.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    let mut account_data = UserAccount::try_from_slice(&user_account.data.borrow())?;

    if account_data.owner != *user.key {
        return Err(ProgramError::InvalidAccountData);
    }

    if account_data.balance < amount {
        return Err(DepositWithdrawError::InsufficientBalance.into());
    }

    account_data.balance = account_data.balance.checked_sub(amount)
        .ok_or(DepositWithdrawError::Overflow)?;

    account_data.serialize(&mut &mut user_account.data.borrow_mut()[..])?;

    **user_account.try_borrow_mut_lamports()? = user_account.lamports()
        .checked_sub(amount)
        .ok_or(ProgramError::InsufficientFunds)?;

    **user.try_borrow_mut_lamports()? = user.lamports()
        .checked_add(amount)
        .ok_or(DepositWithdrawError::Overflow)?;

    msg!("Withdrawal successful: {} lamports", amount);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_program::clock::Epoch;
    use std::mem;

    #[test]
    fn test_initialize_account() {
        let program_id = Pubkey::new_unique();
        let user_key = Pubkey::new_unique();
        let mut lamports = 100000;
        let mut data = vec![0; mem::size_of::<UserAccount>()];
        let owner = program_id;

        let user_account = AccountInfo::new(
            &user_key,
            false,
            true,
            &mut lamports,
            &mut data,
            &owner,
            false,
            Epoch::default(),
        );

        let user = AccountInfo::new(
            &user_key,
            true,
            false,
            &mut lamports,
            &mut [],
            &owner,
            false,
            Epoch::default(),
        );

        let mut rent_lamports = 0;
        let rent_data = vec![0; mem::size_of::<Rent>()];
        let rent = AccountInfo::new(
            &Pubkey::new_unique(),
            false,
            false,
            &mut rent_lamports,
            &rent_data,
            &Pubkey::new_unique(),
            false,
            Epoch::default(),
        );

        let accounts = vec![user_account, user, rent];

        let result = initialize_account(&program_id, &accounts);
        assert!(result.is_ok());

        let account_data = UserAccount::try_from_slice(&accounts[0].data.borrow()).unwrap();
        assert_eq!(account_data.owner, user_key);
        assert_eq!(account_data.balance, 0);
    }

    #[test]
    fn test_deposit_and_withdraw() {
        let program_id = Pubkey::new_unique();
        let user_key = Pubkey::new_unique();
        let mut user_lamports = 100000;
        let mut account_lamports = 0;
        let mut data = vec![0; mem::size_of::<UserAccount>()];
        let owner = program_id;

        let mut user_account = AccountInfo::new(
            &user_key,
            false,
            true,
            &mut account_lamports,
            &mut data,
            &owner,
            false,
            Epoch::default(),
        );

        let mut user = AccountInfo::new(
            &user_key,
            true,
            false,
            &mut user_lamports,
            &mut [],
            &owner,
            false,
            Epoch::default(),
        );

        let mut account_data = UserAccount {
            owner: user_key,
            balance: 0,
        };
        account_data.serialize(&mut &mut user_account.data.borrow_mut()[..]).unwrap();

        let accounts = vec![user_account.clone(), user.clone()];
    }
}
