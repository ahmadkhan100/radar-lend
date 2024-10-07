import * as anchor from "@coral-xyz/anchor";
import { expect } from 'chai';
import { SolSavingsWithChainlink } from "../target/types/sol_savings_with_chainlink";

import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  getAccount,
  getMint,
} from "@solana/spl-token";

const { web3 } = anchor;
const { SystemProgram } = web3;

describe('sol-savings-with-chainlink', () => {
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);

  const program = anchor.workspace.SolSavingsWithChainlink as anchor.Program<SolSavingsWithChainlink>;

  let userAccountKeypair: anchor.web3.Keypair;
  let owner: anchor.web3.Keypair;
  let contract: anchor.web3.Keypair;
  let usdcMintKeypair: anchor.web3.Keypair;
  let contractUsdcAccount: anchor.web3.PublicKey;
  let userUsdcAccount: anchor.web3.PublicKey;

  // Chainlink feed and program IDs (mocked for local testing)
  const chainlinkFeed = anchor.web3.Keypair.generate().publicKey;
  const chainlinkProgram = anchor.web3.Keypair.generate().publicKey;

  before(async () => {
    // Generate keypairs
    owner = anchor.web3.Keypair.generate();
    userAccountKeypair = anchor.web3.Keypair.generate();
    contract = anchor.web3.Keypair.generate();
    usdcMintKeypair = anchor.web3.Keypair.generate();

    // Airdrop SOL to owner and contract accounts
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(owner.publicKey, 2e9),
      "confirmed"
    );

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(contract.publicKey, 2e9),
      "confirmed"
    );

    // Derive contract USDC account
    contractUsdcAccount = await anchor.utils.token.associatedAddress({
      mint: usdcMintKeypair.publicKey,
      owner: contract.publicKey,
    });

    // Derive user's USDC account
    userUsdcAccount = await anchor.utils.token.associatedAddress({
      mint: usdcMintKeypair.publicKey,
      owner: owner.publicKey,
    });
  });

  it('User account has the correct amount of SOL', async () => {
    const balance = await provider.connection.getBalance(owner.publicKey);
    expect(balance).to.equal(2000000000);
  });

  it('Initializes a user account', async () => {
    await program.methods.initialize()
      .accounts({
        userAccount: userAccountKeypair.publicKey,
        owner: owner.publicKey,
        // Remove 'systemProgram' as it's auto-included by Anchor
      })
      .signers([owner, userAccountKeypair])
      .preInstructions([
        await anchor.web3.SystemProgram.createAccount({
          fromPubkey: owner.publicKey,
          newAccountPubkey: userAccountKeypair.publicKey,
          lamports: await provider.connection.getMinimumBalanceForRentExemption(8 + 32 + 8 + 8 + 8 + 1024),
          space: 8 + 32 + 8 + 8 + 8 + 1024,
          programId: program.programId,
        }),
      ])
      .rpc();

    const userAccountData = await program.account.userAccount.fetch(userAccountKeypair.publicKey);

    expect(userAccountData.owner.toBase58()).to.equal(owner.publicKey.toBase58());
    expect(userAccountData.solBalance).to.equal(BigInt(0));
    expect(userAccountData.usdcBalance).to.equal(BigInt(0));
    expect(userAccountData.loanCount).to.equal(BigInt(0));
    expect(userAccountData.loans.length).to.equal(0);
  });

  it('Creates USDC mint and contract USDC account', async () => {
    await program.methods.createUsdcMint()
      .accounts({
        contract: contract.publicKey,
        usdcMint: usdcMintKeypair.publicKey,
        contractUsdcAccount: contractUsdcAccount,
        // Remove 'tokenProgram', 'associatedTokenProgram', 'systemProgram', and 'rent' as they're auto-included
      })
      .signers([contract, usdcMintKeypair])
      .preInstructions([
        await anchor.web3.SystemProgram.createAccount({
          fromPubkey: contract.publicKey,
          newAccountPubkey: usdcMintKeypair.publicKey,
          lamports: await provider.connection.getMinimumBalanceForRentExemption(82),
          space: 82,
          programId: TOKEN_PROGRAM_ID,
        }),
      ])
      .rpc();

    // Fetch the USDC mint
    const usdcMintAccount = await getMint(provider.connection, usdcMintKeypair.publicKey);
    expect(usdcMintAccount.decimals).to.equal(6);
    expect(usdcMintAccount.mintAuthority?.toBase58()).to.equal(contract.publicKey.toBase58());

    // Fetch the contract's USDC token account
    const contractUsdcTokenAccount = await getAccount(provider.connection, contractUsdcAccount);
    expect(contractUsdcTokenAccount.amount).to.equal(BigInt(1_000_000_000_000));
  });

  it('Deposits SOL and takes a loan', async () => {
    const solAmount = 1 * anchor.web3.LAMPORTS_PER_SOL; // 1 SOL
    const usdcAmount = 100_000_000; // 100 USDC (6 decimals)
    const ltv = 25; // Loan-to-value ratio

    // Ensure the user has enough SOL to deposit
    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(owner.publicKey, solAmount),
      "confirmed"
    );

    // Create user's USDC token account if it doesn't exist
    const userUsdcAccountInfo = await provider.connection.getAccountInfo(userUsdcAccount);
    if (!userUsdcAccountInfo) {
      const tx = new anchor.web3.Transaction().add(
        createAssociatedTokenAccountInstruction(
          owner.publicKey,
          userUsdcAccount,
          owner.publicKey,
          usdcMintKeypair.publicKey
        )
      );
      await provider.sendAndConfirm(tx, [owner]);
    }

    // Call the deposit and take loan function
    await program.methods.depositSolAndTakeLoan(new anchor.BN(solAmount), new anchor.BN(usdcAmount), ltv)
      .accounts({
        userAccount: userAccountKeypair.publicKey,
        owner: owner.publicKey,
        contract: contract.publicKey,
        contractUsdcAccount: contractUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        chainlinkFeed: chainlinkFeed,
        chainlinkProgram: chainlinkProgram,
        usdcMint: usdcMintKeypair.publicKey,
        // Remove 'tokenProgram' and 'systemProgram' as they're auto-included
      })
      .signers([owner])
      .rpc();

    // Fetch the user account data
    const userAccountData = await program.account.userAccount.fetch(userAccountKeypair.publicKey);
    expect(userAccountData.solBalance).to.be.greaterThan(BigInt(0));
    expect(userAccountData.usdcBalance).to.equal(BigInt(usdcAmount));
    expect(userAccountData.loanCount).to.equal(BigInt(1));
    expect(userAccountData.loans.length).to.equal(1);

    // Fetch the user's USDC token account balance
    const userUsdcTokenAccount = await getAccount(provider.connection, userUsdcAccount);
    expect(userUsdcTokenAccount.amount).to.equal(BigInt(usdcAmount));
  });

  it('Withdraws SOL from user account', async () => {
    const withdrawAmount = 0.1 * anchor.web3.LAMPORTS_PER_SOL; // Withdraw 0.1 SOL

    // Get initial balances
    const initialUserBalance = await provider.connection.getBalance(owner.publicKey);
    const userAccountDataBefore = await program.account.userAccount.fetch(userAccountKeypair.publicKey);

    await program.methods.withdrawSol(new anchor.BN(withdrawAmount))
      .accounts({
        userAccount: userAccountKeypair.publicKey,
        owner: owner.publicKey,
        // Remove 'systemProgram' as it's auto-included
      })
      .signers([owner])
      .rpc();

    // Get updated balances
    const finalUserBalance = await provider.connection.getBalance(owner.publicKey);
    const userAccountDataAfter = await program.account.userAccount.fetch(userAccountKeypair.publicKey);

    // Verify balances
    expect(userAccountDataAfter.solBalance).to.equal(
      userAccountDataBefore.solBalance - BigInt(withdrawAmount)
    );
    expect(finalUserBalance).to.be.greaterThan(initialUserBalance);
  });

  it('Repays the loan', async () => {
    const loanId = new anchor.BN(1); // Assuming this is the first loan
    const repayAmount = new anchor.BN(100_500_000); // Principal plus interest

    // Ensure the user has enough USDC to repay
    // In this case, the user already has USDC from the loan
    // If additional USDC is needed, mint or transfer more USDC to userUsdcAccount

    await program.methods.repayLoan(loanId, repayAmount)
      .accounts({
        userAccount: userAccountKeypair.publicKey,
        owner: owner.publicKey,
        contract: contract.publicKey,
        contractUsdcAccount: contractUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        usdcMint: usdcMintKeypair.publicKey,
        // Remove 'tokenProgram' and 'systemProgram' as they're auto-included
      })
      .signers([owner])
      .rpc();

    // Fetch the user account data
    const userAccountData = await program.account.userAccount.fetch(userAccountKeypair.publicKey);
    expect(userAccountData.loanCount).to.equal(BigInt(0));
    expect(userAccountData.loans.length).to.equal(0);

    // Fetch the user's USDC token account balance
    const userUsdcTokenAccount = await getAccount(provider.connection, userUsdcAccount);
    expect(userUsdcTokenAccount.amount).to.be.lessThan(BigInt(100_000_000)); // Less than initial loan amount
  });
});
