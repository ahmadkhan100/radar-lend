use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use anchor_spl::associated_token::AssociatedToken;
use chainlink_solana as chainlink;

declare_id!("BShdVK2TQLHZV8CZPhkdXteRFb57H3Q5GJDmf36C2NHH");

const INITIAL_USDC_SUPPLY: u64 = 1_000_000_000_000; // 1,000,000 USDC (6 decimals)
const SECONDS_IN_A_YEAR: u64 = 31_536_000; // 365 days in seconds
const MAX_LOANS_PER_USER: usize = 5;

#[program]
pub mod sol_savings_with_chainlink {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.owner = ctx.accounts.owner.key();
        user_account.sol_balance = 0;
        user_account.usdc_balance = 0;
        user_account.loan_count = 0;
        user_account.loans = vec![]; // Initialize loans as an empty vector
        Ok(())
    }

    pub fn withdraw_sol(ctx: Context<WithdrawSol>, amount: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        require!(user_account.sol_balance >= amount, ErrorCode::InsufficientFunds);

        // Transfer SOL from program account to owner
        **user_account.to_account_info().try_borrow_mut_lamports()? -= amount;
        **ctx.accounts.owner.try_borrow_mut_lamports()? += amount;

        // Update the SOL balance
        user_account.sol_balance = user_account.sol_balance.checked_sub(amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        emit!(WithdrawEvent {
            user: ctx.accounts.owner.key(),
            amount,
        });

        Ok(())
    }

    pub fn create_usdc_mint(ctx: Context<CreateUsdcMint>) -> Result<()> {
        // Mint initial USDC supply to the contract's token account
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::MintTo {
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.contract_usdc_account.to_account_info(),
                    authority: ctx.accounts.contract.to_account_info(),
                },
                &[&[&b"contract_authority"[..], &[ctx.bumps.contract]]],
            ),
            INITIAL_USDC_SUPPLY,
        )?;

        emit!(UsdcMintCreated {
            mint: ctx.accounts.usdc_mint.key(),
            supply: INITIAL_USDC_SUPPLY,
        });

        Ok(())
    }

    pub fn deposit_sol_and_take_loan(
        ctx: Context<DepositSolAndTakeLoan>,
        sol_amount: u64,
        usdc_amount: u64,
        ltv: u8,
    ) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        require!(user_account.loans.len() < MAX_LOANS_PER_USER, ErrorCode::MaxLoansReached);

        // Transfer SOL from owner to program account
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &ctx.accounts.owner.key(),
                &user_account.key(),
                sol_amount,
            ),
            &[
                ctx.accounts.owner.to_account_info(),
                user_account.to_account_info(),
            ],
        )?;

        // Update SOL balance
        user_account.sol_balance = user_account.sol_balance.checked_add(sol_amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        // Fetch current SOL price in USD using Chainlink feed
        let round = chainlink::latest_round_data(
            ctx.accounts.chainlink_program.to_account_info(),
            ctx.accounts.chainlink_feed.to_account_info(),
        )?;
        let sol_price = round.answer as u64; // Assume price is in cents

        // Validate the LTV and determine collateral required
        let (ltv_ratio, apy) = match ltv {
            20 => (20, 0),
            25 => (25, 1),
            33 => (33, 5),
            50 => (50, 8),
            _ => return Err(ErrorCode::InvalidLTV.into()),
        };

        // Calculate required collateral based on LTV and SOL price
        let required_collateral = (usdc_amount.checked_mul(100).ok_or(ErrorCode::ArithmeticOverflow)?)
            .checked_div(ltv_ratio as u64)
            .ok_or(ErrorCode::ArithmeticOverflow)?
            .checked_mul(10000)
            .ok_or(ErrorCode::ArithmeticOverflow)?
            .checked_div(sol_price)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        require!(user_account.sol_balance >= required_collateral, ErrorCode::InsufficientCollateral);

        // Transfer USDC from contract to user
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.contract_usdc_account.to_account_info(),
                    to: ctx.accounts.user_usdc_account.to_account_info(),
                    authority: ctx.accounts.contract.to_account_info(),
                },
                &[&[&b"contract_authority"[..], &[ctx.bumps.contract]]],
            ),
            usdc_amount,
        )?;

        // Create loan
        user_account.loan_count = user_account.loan_count.checked_add(1)
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        let loan = Loan {
            id: user_account.loan_count,
            start_date: Clock::get()?.unix_timestamp,
            principal: usdc_amount,
            apy,
            collateral: required_collateral,
            ltv,
            borrower: ctx.accounts.owner.key(),
        };

        // Add the loan to the user's loan list
        user_account.loans.push(loan);

        // Update balances
        user_account.sol_balance = user_account.sol_balance.checked_sub(required_collateral)
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        user_account.usdc_balance = user_account.usdc_balance.checked_add(usdc_amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        // Emit loan creation event
        emit!(LoanCreated {
            loan_id: user_account.loan_count,
            borrower: ctx.accounts.owner.key(),
            usdc_amount,
            collateral: required_collateral,
            ltv,
            apy,
        });

        Ok(())
    }

    pub fn repay_loan(ctx: Context<RepayLoan>, loan_id: u64, usdc_amount: u64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;

        // Ensure the signer is the owner of the user account
        require!(ctx.accounts.owner.key() == user_account.owner, ErrorCode::UnauthorizedAccess);

        // Find the loan by ID
        let loan_index = user_account.loans.iter().position(|loan| loan.id == loan_id)
            .ok_or(ErrorCode::LoanNotFound)?;

        let (principal, interest, collateral, total_owed) = {
            let loan = &user_account.loans[loan_index];

            // Ensure the signer is the original borrower
            require!(ctx.accounts.owner.key() == loan.borrower, ErrorCode::UnauthorizedAccess);

            // Calculate interest based on time passed
            let duration = Clock::get()?.unix_timestamp.checked_sub(loan.start_date)
                .ok_or(ErrorCode::ArithmeticOverflow)?;
            let interest = (duration as u64)
                .checked_mul(loan.apy as u64)
                .and_then(|result| result.checked_mul(loan.principal))
                .and_then(|result| result.checked_div(SECONDS_IN_A_YEAR))
                .and_then(|result| result.checked_div(100))
                .ok_or(ErrorCode::ArithmeticOverflow)?;
            let total_owed = loan.principal.checked_add(interest)
                .ok_or(ErrorCode::ArithmeticOverflow)?;

            require!(usdc_amount <= total_owed, ErrorCode::RepaymentAmountTooHigh);

            (loan.principal, interest, loan.collateral, total_owed)
        };

        // Transfer USDC from user to contract
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.user_usdc_account.to_account_info(),
                    to: ctx.accounts.contract_usdc_account.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            usdc_amount,
        )?;

        // Update USDC balance
        user_account.usdc_balance = user_account.usdc_balance.checked_sub(usdc_amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        // Handle repayment logic
        if usdc_amount == total_owed {
            // Loan fully repaid, return collateral
            user_account.sol_balance = user_account.sol_balance.checked_add(collateral)
                .ok_or(ErrorCode::ArithmeticOverflow)?;
            user_account.loans.remove(loan_index); // Remove loan after full repayment

            emit!(LoanRepaid {
                loan_id,
                borrower: ctx.accounts.owner.key(),
                usdc_amount,
                collateral_returned: collateral,
                interest_paid: interest,
            });
        } else {
            // Partial repayment: update the loan's remaining principal and interest
            let remaining = total_owed.checked_sub(usdc_amount)
                .ok_or(ErrorCode::ArithmeticOverflow)?;
            let remaining_principal = if remaining > interest { 
                remaining.checked_sub(interest).ok_or(ErrorCode::ArithmeticOverflow)?
            } else { 
                0 
            };
            let interest_paid = usdc_amount.saturating_sub(principal.checked_sub(remaining_principal)
                .ok_or(ErrorCode::ArithmeticOverflow)?);

            let loan = &mut user_account.loans[loan_index];
            loan.principal = remaining_principal;
            loan.start_date = Clock::get()?.unix_timestamp; // Reset loan start date

            emit!(PartialRepayment {
                loan_id,
                borrower: ctx.accounts.owner.key(),
                usdc_amount,
                remaining_principal,
                interest_paid,
            });
        }

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = owner, space = 8 + 32 + 8 + 8 + 8 + (200 * MAX_LOANS_PER_USER))]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawSol<'info> {
    #[account(mut, has_one = owner)]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateUsdcMint<'info> {
    #[account(
        seeds = [b"contract_authority"],
        bump,
    )]
    pub contract: SystemAccount<'info>,
    #[account(
        init,
        payer = payer,
        mint::decimals = 6,
        mint::authority = contract.key(),
    )]
    pub usdc_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = payer,
        associated_token::mint = usdc_mint,
        associated_token::authority = contract,
    )]
    pub contract_usdc_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct DepositSolAndTakeLoan<'info> {
    #[account(mut, has_one = owner)]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        seeds = [b"contract_authority"],
        bump,
    )]
    pub contract: SystemAccount<'info>,
    #[account(mut)]
    pub contract_usdc_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_usdc_account.owner == owner.key())]
    pub user_usdc_account: Account<'info, TokenAccount>,
    /// CHECK: This account is not being read or written to. We just pass it through to the Chainlink program.
    pub chainlink_feed: AccountInfo<'info>,
    /// CHECK: This is the Chainlink program ID, which is a valid Solana program.
    pub chainlink_program: AccountInfo<'info>,
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RepayLoan<'info> {
    #[account(mut, has_one = owner)]
    pub user_account: Account<'info, UserAccount>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        seeds = [b"contract_authority"],
        bump,
    )]
    pub contract: SystemAccount<'info>,
    #[account(mut)]
    pub contract_usdc_account: Account<'info, TokenAccount>,
    #[account(mut, constraint = user_usdc_account.owner == owner.key())]
    pub user_usdc_account: Account<'info, TokenAccount>,
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct UserAccount {
    pub owner: Pubkey,
    pub sol_balance: u64,
    pub usdc_balance: u64,
    pub loan_count: u64,
    pub loans: Vec<Loan>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct Loan {
    pub id: u64,
    pub start_date: i64,
    pub principal: u64,
    pub apy: u8,
    pub collateral: u64,
    pub ltv: u8,
    pub borrower: Pubkey,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Insufficient funds for withdrawal")]
    InsufficientFunds,
    #[msg("Insufficient collateral for loan")]
    InsufficientCollateral,
    #[msg("Loan not found")]
    LoanNotFound,
    #[msg("Repayment amount exceeds loan balance")]
    RepaymentAmountTooHigh,
    #[msg("Invalid LTV ratio")]
    InvalidLTV,
    #[msg("Arithmetic overflow occurred")]
    ArithmeticOverflow,
    #[msg("Maximum number of loans reached for this user")]
    MaxLoansReached,
    #[msg("Unauthorized access")]
    UnauthorizedAccess,
}

#[event]
pub struct LoanCreated {
    pub loan_id: u64,
    pub borrower: Pubkey,
    pub usdc_amount: u64,
    pub collateral: u64,
    pub ltv: u8,
    pub apy: u8,
}

#[event]
pub struct LoanRepaid {
    pub loan_id: u64,
    pub borrower: Pubkey,
    pub usdc_amount: u64,
    pub collateral_returned: u64,
    pub interest_paid: u64,
}

#[event]
pub struct PartialRepayment {
    pub loan_id: u64,
    pub borrower: Pubkey,
    pub usdc_amount: u64,
    pub remaining_principal: u64,
    pub interest_paid: u64,
}

#[event]
pub struct WithdrawEvent {
    pub user: Pubkey,
    pub amount: u64,
}

#[event]
pub struct UsdcMintCreated {
    pub mint: Pubkey,
    pub supply: u64,
}