use {
    solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        system_program,
    },
    solana_program_test::*,
    solana_sdk::{
        signature::{Keypair, Signer},
        transaction::Transaction,
    },
    spl_token::{
        instruction as token_instruction,
        state::{Account as TokenAccount, Mint},
    },
};

use your_crate_name::{
    processor::process_instruction,
    state::{LoanAccount, LoanInstruction},
    id, USDC_MINT, PROGRAM_USDC_ACCOUNT, SOL_PRICE, LTV,
};

async fn setup() -> (BanksClient, Keypair, Hash) {
    let program_id = id();
    let mut program_test = ProgramTest::new(
        "your_program_name",
        program_id,
        processor!(process_instruction),
    );

    // Add USDC mint
    let usdc_mint = Keypair::new();
    program_test.add_packable_account(
        USDC_MINT,
        u32::MAX as u64,
        &Mint {
            mint_authority: COption::Some(usdc_mint.pubkey()),
            supply: 1_000_000_000_000, // 1M USDC
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        &spl_token::id(),
    );

    // Add program USDC account
    program_test.add_packable_account(
        PROGRAM_USDC_ACCOUNT,
        u32::MAX as u64,
        &TokenAccount {
            mint: USDC_MINT,
            owner: program_id,
            amount: 1_000_000_000_000, // 1M USDC
            state: spl_token::state::AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        },
        &spl_token::id(),
    );

    program_test.start().await
}

#[tokio::test]
async fn test_initialize_loan() {
    let (mut banks_client, payer, recent_blockhash) = setup().await;

    let borrower = Keypair::new();
    let loan_amount = 1_000_000_000; // 1000 USDC
    let apy = 500; // 5% APY

    // Airdrop SOL to borrower
    let required_collateral = (loan_amount * 100) / (SOL_PRICE * LTV);
    let airdrop_amount = required_collateral + 1_000_000_000; // Extra for rent and gas
    let transaction = Transaction::new_signed_with_payer(
        &[system_instruction::transfer(
            &payer.pubkey(),
            &borrower.pubkey(),
            airdrop_amount,
        )],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Create borrower's USDC account
    let borrower_usdc_account = Keypair::new();
    let transaction = Transaction::new_signed_with_payer(
        &[
            system_instruction::create_account(
                &payer.pubkey(),
                &borrower_usdc_account.pubkey(),
                Rent::default().minimum_balance(TokenAccount::LEN),
                TokenAccount::LEN as u64,
                &spl_token::id(),
            ),
            token_instruction::initialize_account(
                &spl_token::id(),
                &borrower_usdc_account.pubkey(),
                &USDC_MINT,
                &borrower.pubkey(),
            )
            .unwrap(),
        ],
        Some(&payer.pubkey()),
        &[&payer, &borrower_usdc_account],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Initialize loan
    let (loan_account_pubkey, _) = Pubkey::find_program_address(&[borrower.pubkey().as_ref(), b"loan"], &id());
    let transaction = Transaction::new_signed_with_payer(
        &[Instruction::new_with_borsh(
            id(),
            &LoanInstruction::InitializeLoan {
                amount: loan_amount,
                apy,
            },
            vec![
                AccountMeta::new(borrower.pubkey(), true),
                AccountMeta::new(loan_account_pubkey, false),
                AccountMeta::new_readonly(USDC_MINT, false),
                AccountMeta::new(borrower_usdc_account.pubkey(), false),
                AccountMeta::new(PROGRAM_USDC_ACCOUNT, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new_readonly(spl_token::id(), false),
                AccountMeta::new_readonly(solana_program::sysvar::rent::id(), false),
            ],
        )],
        Some(&borrower.pubkey()),
        &[&borrower],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Verify loan account
    let loan_account = banks_client.get_account(loan_account_pubkey).await.unwrap().unwrap();
    let loan_data = LoanAccount::try_from_slice(&loan_account.data).unwrap();
    assert_eq!(loan_data.borrower, borrower.pubkey());
    assert_eq!(loan_data.principal, loan_amount);
    assert_eq!(loan_data.apy, apy);
    assert_eq!(loan_data.collateral, required_collateral);

    // Verify borrower's USDC balance
    let borrower_usdc_account_data = banks_client.get_account(borrower_usdc_account.pubkey()).await.unwrap().unwrap();
    let borrower_usdc_balance = TokenAccount::unpack(&borrower_usdc_account_data.data).unwrap().amount;
    assert_eq!(borrower_usdc_balance, loan_amount);
}

#[tokio::test]
async fn test_repay_loan() {
    // Similar setup to initialize_loan test
    // ...

    // Initialize loan
    // ...

    // Repay part of the loan
    let repay_amount = 500_000_000; // 500 USDC
    let transaction = Transaction::new_signed_with_payer(
        &[Instruction::new_with_borsh(
            id(),
            &LoanInstruction::RepayLoan {
                amount: repay_amount,
            },
            vec![
                AccountMeta::new(borrower.pubkey(), true),
                AccountMeta::new(loan_account_pubkey, false),
                AccountMeta::new(borrower_usdc_account.pubkey(), false),
                AccountMeta::new(PROGRAM_USDC_ACCOUNT, false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ],
        )],
        Some(&borrower.pubkey()),
        &[&borrower],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Verify loan account
    let loan_account = banks_client.get_account(loan_account_pubkey).await.unwrap().unwrap();
    let loan_data = LoanAccount::try_from_slice(&loan_account.data).unwrap();
    assert!(loan_data.principal < loan_amount && loan_data.principal > 0);

    // Verify borrower's USDC balance
    let borrower_usdc_account_data = banks_client.get_account(borrower_usdc_account.pubkey()).await.unwrap().unwrap();
    let borrower_usdc_balance = TokenAccount::unpack(&borrower_usdc_account_data.data).unwrap().amount;
    assert_eq!(borrower_usdc_balance, loan_amount - repay_amount);
}

#[tokio::test]
async fn test_liquidate_loan() {
    // Similar setup to initialize_loan test
    // ...

    // Initialize loan
    // ...

    // Simulate price drop
    // This would typically be done by updating the SOL_PRICE constant, but for testing purposes,
    // we can create a situation where the loan becomes underwater

    let liquidator = Keypair::new();
    // Airdrop SOL and USDC to liquidator
    // ...

    // Liquidate loan
    let transaction = Transaction::new_signed_with_payer(
        &[Instruction::new_with_borsh(
            id(),
            &LoanInstruction::LiquidateLoan,
            vec![
                AccountMeta::new(liquidator.pubkey(), true),
                AccountMeta::new(loan_account_pubkey, false),
                AccountMeta::new(liquidator_usdc_account.pubkey(), false),
                AccountMeta::new(PROGRAM_USDC_ACCOUNT, false),
                AccountMeta::new_readonly(spl_token::id(), false),
            ],
        )],
        Some(&liquidator.pubkey()),
        &[&liquidator],
        recent_blockhash,
    );
    banks_client.process_transaction(transaction).await.unwrap();

    // Verify loan account is closed
    let loan_account = banks_client.get_account(loan_account_pubkey).await.unwrap();
    assert!(loan_account.is_none());

    // Verify liquidator received collateral
    let liquidator_account = banks_client.get_account(liquidator.pubkey()).await.unwrap().unwrap();
    assert!(liquidator_account.lamports > initial_liquidator_balance);
}
