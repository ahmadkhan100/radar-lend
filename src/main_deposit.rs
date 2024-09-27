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
        msg!("Error: User wallet must be a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if user_account.owner != program_id {
        msg!("Error: User account must be owned by the program");
        return Err(ProgramError::IncorrectProgramId);
    }

    if amount == 0 {
        msg!("Error: Deposit amount must be greater than zero");
        return Err(ProgramError::InvalidInstructionData);
    }

    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(std::mem::size_of::<UserAccount>());

    if user_account.lamports() < required_lamports {
        msg!("Initializing user account");
        let lamports_to_transfer = required_lamports - user_account.lamports();
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
        msg!("Error: User wallet must be a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if user_account.owner != program_id {
        msg!("Error: User account must be owned by the program");
        return Err(ProgramError::IncorrectProgramId);
    }

    if amount == 0 {
        msg!("Error: Withdrawal amount must be greater than zero");
        return Err(ProgramError::InvalidInstructionData);
    }

    let mut user_data = UserAccount::try_from_slice(&user_account.data.borrow())?;
    if user_data.balance < amount {
        msg!("Error: Insufficient funds for withdrawal");
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

// This is not necessary for the main.rs file, but if you want to include tests in the same file:
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
    fn test_deposit() {
        let program_id = Pubkey::new_unique();
        let user_wallet_key = Pubkey::new_unique();
        let user_account_key = Pubkey::new_unique();
        let system_program_key = Pubkey::new_unique();

        let mut user_wallet_lamports = 5_000_000_000; // 5 SOL
        let mut user_account_lamports = 0;
        let mut user_account_data = vec![0; mem::size_of::<UserAccount>()];

        let mut user_wallet_account = create_account_info(
            &user_wallet_key, true, &mut user_wallet_lamports, &mut [], &system_program_key
        );
        let mut user_account = create_account_info(
            &user_account_key, false, &mut user_account_lamports, &mut user_account_data, &program_id
        );
        let system_program_account = create_account_info(
            &system_program_key, false, &mut 0, &mut [], &system_program_key
        );

        let accounts = vec![
            user_account,
            user_wallet_account,
            system_program_account,
        ];

        let deposit_amount = 1_000_000_000; // 1 SOL
        let instruction_data = ShrubFinanceInstruction::Deposit { amount: deposit_amount }
            .try_to_vec()
            .unwrap();

        let result = process_instruction(&program_id, &accounts, &instruction_data);
        assert!(result.is_ok());

        let user_data = UserAccount::try_from_slice(&accounts[0].data.borrow()).unwrap();
        assert_eq!(user_data.balance, deposit_amount);
    }

    #[test]
    fn test_withdraw() {
        let program_id = Pubkey::new_unique();
        let user_wallet_key = Pubkey::new_unique();
        let user_account_key = Pubkey::new_unique();

        let mut user_wallet_lamports = 1_000_000_000; // 1 SOL
        let mut user_account_lamports = 2_000_000_000; // 2 SOL
        let mut user_account_data = UserAccount { balance: 2_000_000_000 }.try_to_vec().unwrap();

        let mut user_wallet_account = create_account_info(
            &user_wallet_key, true, &mut user_wallet_lamports, &mut [], &system_program::id()
        );
        let mut user_account = create_account_info(
            &user_account_key, false, &mut user_account_lamports, &mut user_account_data, &program_id
        );

        let accounts = vec![
            user_account,
            user_wallet_account,
        ];

        let withdraw_amount = 1_000_000_000; // 1 SOL
        let instruction_data = ShrubFinanceInstruction::Withdraw { amount: withdraw_amount }
            .try_to_vec()
            .unwrap();

        let result = process_instruction(&program_id, &accounts, &instruction_data);
        assert!(result.is_ok());

        let user_data = UserAccount::try_from_slice(&accounts[0].data.borrow()).unwrap();
        assert_eq!(user_data.balance, 1_000_000_000); // 2 SOL - 1 SOL = 1 SOL
    }
}
