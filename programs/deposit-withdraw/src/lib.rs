use anchor_lang::prelude::*;
use anchor_client::{
    solana_sdk::{
        signature::{Keypair, Signer},
        transaction::Transaction,
    },
    Client, Cluster,
};
use std::str::FromStr;

declare_id!("27EvSTnpN61RPkypusJM3yk2a9qA1utPsgNfpwhpYFBw");

#[program]
pub mod solana_vault {
    use super::*;
    pub fn initialize(ctx: Context<Initialize>) -> ProgramResult {
        let vault = &mut ctx.accounts.vault;
        vault.authority = ctx.accounts.authority.key();
        vault.balance = 0;
        Ok(())
    }

    pub fn deposit(ctx: Context<Transact>, amount: u64) -> ProgramResult {
        let vault = &mut ctx.accounts.vault;
        if **ctx.accounts.depositor.lamports.borrow() < amount {
            return Err(ProgramError::InsufficientFunds);
        }
        **ctx.accounts.depositor.try_borrow_mut_lamports()? -= amount;
        **vault.to_account_info().try_borrow_mut_lamports()? += amount;
        vault.balance += amount;
        Ok(())
    }

    pub fn withdraw(ctx: Context<Transact>, amount: u64) -> ProgramResult {
        let vault = &mut ctx.accounts.vault;
        if vault.balance < amount {
            return Err(ProgramError::InsufficientFunds);
        }
        **ctx.accounts.receiver.try_borrow_mut_lamports()? += amount;
        **vault.to_account_info().try_borrow_mut_lamports()? -= amount;
        vault.balance -= amount;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + 40)]
    pub vault: Account<'info, Vault>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Transact<'info> {
    #[account(mut, has_one = authority)]
    pub vault: Account<'info, Vault>,
    pub authority: Signer<'info>,
    #[account(mut)]
    pub depositor: AccountInfo<'info>,
    #[account(mut)]
    pub receiver: AccountInfo<'info>,
}

#[account]
pub struct Vault {
    pub authority: Pubkey,
    pub balance: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::system_program;
    use anchor_lang::ToAccountInfos;

    #[test]
    fn test_deposit_and_withdraw() {
        let client = Client::new(Cluster::Devnet);
        let payer = Keypair::new();
        let vault_keypair = Keypair::new();
        let depositor_keypair = Keypair::new();
        let receiver_keypair = Keypair::new();

        // Airdrop SOL to payer, depositor
        let _ = client.request_airdrop(&payer.pubkey(), 1_000_000_000).unwrap();
        let _ = client.request_airdrop(&depositor_keypair.pubkey(), 1_000_000_000).unwrap();

        // Create and send transaction to initialize the vault
        let mut transaction = Transaction::new_with_payer(
            &[solana_vault::initialize(
                ctx.accounts.initialize(
                    vault_keypair.pubkey(),
                    payer.pubkey(),
                    system_program::id(),
                ),
                0, // Assuming space is correctly set in your actual program
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[&payer, &vault_keypair], client.get_recent_blockhash().unwrap());
        client.send_and_confirm_transaction(&transaction).unwrap();

        // Deposit to the vault
        let mut transaction = Transaction::new_with_payer(
            &[solana_vault::deposit(
                ctx.accounts.transact(
                    vault_keypair.pubkey(),
                    payer.pubkey(),
                    depositor_keypair.pubkey(),
                    receiver_keypair.pubkey(),
                ),
                100, // Amount to deposit
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[&payer, &depositor_keypair], client.get_recent_blockhash().unwrap());
        client.send_and_confirm_transaction(&transaction).unwrap();

        // Withdraw from the vault
        let mut transaction = Transaction::new_with_payer(
            &[solana_vault::withdraw(
                ctx.accounts.transact(
                    vault_keypair.pubkey(),
                    payer.pubkey(),
                    depositor_keypair.pubkey(),
                    receiver_keypair.pubkey(),
                ),
                50, // Amount to withdraw
            )],
            Some(&payer.pubkey()),
        );
        transaction.sign(&[&payer, &receiver_keypair], client.get_recent_blockhash().unwrap());
        client.send_and_confirm_transaction(&transaction).unwrap();

        // Assertions would go here, such as checking final balances, etc.
    }
}
