// deposit_program/tests/integration_tests.rs

use borsh::{BorshDeserialize, BorshSerialize}; // Import necessary traits
use solana_program::pubkey::Pubkey;
use solana_program_test::*;
use solana_sdk::{signature::Keypair, transaction::Transaction};
use solana_sdk::instruction::Instruction; // Import Instruction
use deposit_program::instruction::DepositInstruction; // Adjust this import based on your module structure

#[derive(BorshSerialize, BorshDeserialize)] // Ensure this is added to your struct definition
pub enum DepositInstruction {
    Deposit { amount: u64 },
    Withdraw { amount: u64 },
}

#[tokio::test]
async fn test_deposit() {
    let program_id = Pubkey::new_unique();
    let user = Keypair::new();
    
    let program_test = ProgramTest::new(
        "deposit_program", // Adjust this based on your program name
        program_id,
        processor!(process_instruction), // Adjust this based on your processor function
    );

    let (mut banks_client, payer, recent_blockhash) = program_test.start().await.unwrap();

    let deposit_amount = 100;
    let deposit_instruction = Instruction::new_with_borsh(
        program_id,
        &DepositInstruction::Deposit { amount: deposit_amount },
        vec![user.pubkey()],
    );

    let transaction = Transaction::new_signed_with_payer(
        &[deposit_instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // Execute the transaction
    let _result = banks_client.process_transaction(transaction).await.unwrap();

    // Additional assertions can be added here
}

#[tokio::test]
async fn test_withdraw() {
    let program_id = Pubkey::new_unique();
    let user = Keypair::new();
    
    let program_test = ProgramTest::new(
        "deposit_program", // Adjust this based on your program name
        program_id,
        processor!(process_instruction), // Adjust this based on your processor function
    );

    let (mut banks_client, payer, recent_blockhash) = program_test.start().await.unwrap();

    let withdraw_amount = 50;
    let withdraw_instruction = Instruction::new_with_borsh(
        program_id,
        &DepositInstruction::Withdraw { amount: withdraw_amount },
        vec![user.pubkey()],
    );

    let transaction = Transaction::new_signed_with_payer(
        &[withdraw_instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    // Execute the transaction
    let _result = banks_client.process_transaction(transaction).await.unwrap();

    // Additional assertions can be added here
}
