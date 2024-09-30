use {
    borsh::{BorshDeserialize, BorshSerialize},
    solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        rent::Rent,
        system_program,
    },
    solana_program_test::*,
    solana_sdk::{
        account::Account,
        signature::{Keypair, Signer},
        transaction::Transaction,
    },
};

use shrub_hackathon::{process_instruction, UserAccount, ShrubFinanceInstruction, ID as PROGRAM_ID};

async fn setup() -> (BanksClient, Keypair, Hash) {
    let program_id = PROGRAM_ID;
    let mut program_test = ProgramTest::new(
        "shrub_hackathon",
        program_id,
        processor!(process_instruction),
    );
    program_test.start().await
}

#[tokio::test]
async fn test_deposit() {
    let (mut banks_client, payer, recent_blockhash) = setup().await;

    let user_wallet = Keypair::new();
    let user_account = Keypair::new();

    // Airdrop some SOL to the user wallet
    let lamports = 5_000_000_000; // 5 SOL
    let transaction = Transaction::new_signed_with_payer(
        &[solana_sdk::system_instruction::transfer(
            &payer.pubkey(),
            &user_wallet.pubkey(),
            lamports,
        )],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Create user account
    let rent = banks_client.get_rent().await.unwrap();
    let account_rent = rent.minimum_balance(std::mem::size_of::<UserAccount>());
    let transaction = Transaction::new_signed_with_payer(
        &[
            solana_sdk::system_instruction::create_account(
                &user_wallet.pubkey(),
                &user_account.pubkey(),
                account_rent,
                std::mem::size_of::<UserAccount>() as u64,
                &PROGRAM_ID,
            ),
        ],
        Some(&user_wallet.pubkey()),
        &[&user_wallet, &user_account],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Deposit instruction
    let deposit_amount = 1_000_000_000; // 1 SOL
    let instruction = Instruction::new_with_borsh(
        PROGRAM_ID,
        &ShrubFinanceInstruction::Deposit { amount: deposit_amount },
        vec![
            AccountMeta::new(user_account.pubkey(), false),
            AccountMeta::new(user_wallet.pubkey(), true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&user_wallet.pubkey()),
        &[&user_wallet],
        recent_blockhash,
    );

    banks_client.process_transaction(transaction).await.unwrap();

    // Verify deposit
    let user_account_data = banks_client.get_account(user_account.pubkey()).await.unwrap().unwrap();
    let user_data = UserAccount::try_from_slice(&user_account_data.data).unwrap();
    assert_eq!(user_data.balance, deposit_amount);

    // Verify user wallet balance
    let user_wallet_account = banks_client.get_account(user_wallet.pubkey()).await.unwrap().unwrap();
    assert_eq!(user_wallet_account.lamports, lamports - deposit_amount - account_rent);
}

#[tokio::test]
async fn test_withdraw() {
    let (mut banks_client, payer, recent_blockhash) = setup().await;

    let user_wallet = Keypair::new();
    let user_account = Keypair::new();

    // Airdrop some SOL to the user wallet
    let initial_balance = 5_000_000_000; // 5 SOL
    let transaction = Transaction::new_signed_with_payer(
        &[solana_sdk::system_instruction::transfer(
            &payer.pubkey(),
            &user_wallet.pubkey(),
            initial_balance,
        )],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Create user account
    let rent = banks_client.get_rent().await.unwrap();
    let account_rent = rent.minimum_balance(std::mem::size_of::<UserAccount>());
    let transaction = Transaction::new_signed_with_payer(
        &[
            solana_sdk::system_instruction::create_account(
                &user_wallet.pubkey(),
                &user_account.pubkey(),
                account_rent,
                std::mem::size_of::<UserAccount>() as u64,
                &PROGRAM_ID,
            ),
        ],
        Some(&user_wallet.pubkey()),
        &[&user_wallet, &user_account],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Deposit some funds first
    let deposit_amount = 2_000_000_000; // 2 SOL
    let deposit_instruction = Instruction::new_with_borsh(
        PROGRAM_ID,
        &ShrubFinanceInstruction::Deposit { amount: deposit_amount },
        vec![
            AccountMeta::new(user_account.pubkey(), false),
            AccountMeta::new(user_wallet.pubkey(), true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
    );
    let transaction = Transaction::new_signed_with_payer(
        &[deposit_instruction],
        Some(&user_wallet.pubkey()),
        &[&user_wallet],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Withdraw instruction
    let withdraw_amount = 1_000_000_000; // 1 SOL
    let instruction = Instruction::new_with_borsh(
        PROGRAM_ID,
        &ShrubFinanceInstruction::Withdraw { amount: withdraw_amount },
        vec![
            AccountMeta::new(user_account.pubkey(), false),
            AccountMeta::new(user_wallet.pubkey(), true),
        ],
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&user_wallet.pubkey()),
        &[&user_wallet],
        recent_blockhash,
    );

    banks_client.process_transaction(transaction).await.unwrap();

    // Verify withdrawal
    let user_account_data = banks_client.get_account(user_account.pubkey()).await.unwrap().unwrap();
    let user_data = UserAccount::try_from_slice(&user_account_data.data).unwrap();
    assert_eq!(user_data.balance, deposit_amount - withdraw_amount);

    // Verify user wallet balance
    let user_wallet_account = banks_client.get_account(user_wallet.pubkey()).await.unwrap().unwrap();
    let expected_balance = initial_balance - account_rent - deposit_amount + withdraw_amount;
    assert_eq!(user_wallet_account.lamports, expected_balance);
}

#[tokio::test]
async fn test_deposit_zero_amount() {
    let (mut banks_client, payer, recent_blockhash) = setup().await;

    let user_wallet = Keypair::new();
    let user_account = Keypair::new();

    // Airdrop some SOL to the user wallet and create user account (similar to deposit test)
    // ... (Add the same setup code as in the deposit test)

    // Attempt to deposit zero amount
    let deposit_amount = 0;
    let instruction = Instruction::new_with_borsh(
        PROGRAM_ID,
        &ShrubFinanceInstruction::Deposit { amount: deposit_amount },
        vec![
            AccountMeta::new(user_account.pubkey(), false),
            AccountMeta::new(user_wallet.pubkey(), true),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&user_wallet.pubkey()),
        &[&user_wallet],
        recent_blockhash,
    );

    let result = banks_client.process_transaction(transaction).await;
    assert!(result.is_err()); // Expect an error for zero amount deposit
}

#[tokio::test]
async fn test_withdraw_insufficient_funds() {
    let (mut banks_client, payer, recent_blockhash) = setup().await;

    let user_wallet = Keypair::new();
    let user_account = Keypair::new();

    // Airdrop some SOL to the user wallet and create user account (similar to deposit test)
    // ... (Add the same setup code as in the deposit test)

    // Deposit some funds first
    let deposit_amount = 1_000_000_000; // 1 SOL
    // ... (Add deposit instruction similar to the deposit test)

    // Attempt to withdraw more than deposited
    let withdraw_amount = 2_000_000_000; // 2 SOL
    let instruction = Instruction::new_with_borsh(
        PROGRAM_ID,
        &ShrubFinanceInstruction::Withdraw { amount: withdraw_amount },
        vec![
            AccountMeta::new(user_account.pubkey(), false),
            AccountMeta::new(user_wallet.pubkey(), true),
        ],
    );

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&user_wallet.pubkey()),
        &[&user_wallet],
        recent_blockhash,
    );

    let result = banks_client.process_transaction(transaction).await;
    assert!(result.is_err()); // Expect an error for insufficient funds
}
