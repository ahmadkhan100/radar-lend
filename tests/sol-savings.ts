import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolSavings } from "../target/types/sol_savings";
import { PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAccount
} from '@solana/spl-token';
import { expect } from 'chai';

describe('sol-savings', () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SolSavings as Program<SolSavings>;

  let userAccount: Keypair;
  let adminAccount: Keypair;
  let usdcMint: PublicKey;
  let adminUsdcAccount: PublicKey;
  let userUsdcAccount: PublicKey;
  let shrubPDA: PublicKey;
  let shrubBump: number;
  let shrubUsdcAccount: PublicKey;

  const CHAINLINK_PROGRAM_ID = new PublicKey("AL2pejr3LLKAiE47G64Br3XprkyrmymLzMQiqZMhQVoa");
  const SOL_USD_FEED = new PublicKey("AL2pejr3LLKAiE47G64Br3XprkyrmymLzMQiqZMhQVoa");

  async function airdropSol(account: Keypair, amount: number): Promise<void> {
    const signature = await provider.connection.requestAirdrop(account.publicKey, amount * LAMPORTS_PER_SOL);
    await provider.connection.confirmTransaction(signature);
  }

  before(async () => {
    userAccount = Keypair.generate();
    adminAccount = Keypair.generate();

    // Airdrop SOL to user and admin accounts
    await airdropSol(userAccount, 10);
    await airdropSol(adminAccount, 10);

    // Verify balances
    const userBalance = await provider.connection.getBalance(userAccount.publicKey);
    const adminBalance = await provider.connection.getBalance(adminAccount.publicKey);
    console.log(`User balance: ${userBalance / LAMPORTS_PER_SOL} SOL`);
    console.log(`Admin balance: ${adminBalance / LAMPORTS_PER_SOL} SOL`);

    // Create USDC Mint
    usdcMint = await createMint(
      provider.connection,
      adminAccount,
      adminAccount.publicKey,
      null,
      6
    );

    // Create admin and user USDC accounts
    adminUsdcAccount = (await getOrCreateAssociatedTokenAccount(
      provider.connection,
      adminAccount,
      usdcMint,
      adminAccount.publicKey
    )).address;

    userUsdcAccount = (await getOrCreateAssociatedTokenAccount(
      provider.connection,
      userAccount,
      usdcMint,
      userAccount.publicKey
    )).address;

    // Derive PDA for shrub
    [shrubPDA, shrubBump] = await PublicKey.findProgramAddress(
      [Buffer.from("contract_authority")],
      program.programId
    );

    // Get shrub USDC account
    shrubUsdcAccount = (await getOrCreateAssociatedTokenAccount(
      provider.connection,
      adminAccount,
      usdcMint,
      shrubPDA,
      true
    )).address;
  });

  it("Initializes user account", async () => {
    await program.methods.initialize()
      .accounts({
        userAccount: userAccount.publicKey,
        owner: userAccount.publicKey,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.owner.toString()).to.equal(userAccount.publicKey.toString());
    expect(account.solBalance.toNumber()).to.equal(0);
    expect(account.usdcBalance.toNumber()).to.equal(0);
    expect(account.loanCount.toNumber()).to.equal(0);
    expect(account.loans.length).to.equal(0);
  });

  it("Creates USDC mint", async () => {
    await program.methods.createUsdcMint()
      .accounts({
        contract: shrubPDA,
        usdcMint: usdcMint,
        contractUsdcAccount: shrubUsdcAccount,
        payer: adminAccount.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      } as any)
      .signers([adminAccount])
      .rpc();

    const mintInfo = await provider.connection.getTokenSupply(usdcMint);
    expect(Number(mintInfo.value.amount)).to.equal(1_000_000_000_000); // 1,000,000 USDC
  });

  it("Deposits SOL and takes a loan", async () => {
    const solAmount = new anchor.BN(1 * LAMPORTS_PER_SOL);
    const usdcAmount = new anchor.BN(50 * 1000000); // 50 USDC
    const ltv = 50; // 50% LTV

    await program.methods.depositSolAndTakeLoan(solAmount, usdcAmount, ltv)
      .accounts({
        userAccount: userAccount.publicKey,
        owner: userAccount.publicKey,
        contract: shrubPDA,
        contractUsdcAccount: shrubUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        chainlinkFeed: SOL_USD_FEED,
        chainlinkProgram: CHAINLINK_PROGRAM_ID,
        usdcMint: usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.loans.length).to.equal(1);
    expect(account.loans[0].ltv).to.equal(ltv);
    expect(account.usdcBalance.toNumber()).to.equal(usdcAmount.toNumber());
  });

  it("Fails to take a loan with invalid LTV", async () => {
    const solAmount = new anchor.BN(1 * LAMPORTS_PER_SOL);
    const usdcAmount = new anchor.BN(50 * 1000000); // 50 USDC
    const invalidLtv = 40; // Invalid LTV

    try {
      await program.methods.depositSolAndTakeLoan(solAmount, usdcAmount, invalidLtv)
        .accounts({
          userAccount: userAccount.publicKey,
          owner: userAccount.publicKey,
          contract: shrubPDA,
          contractUsdcAccount: shrubUsdcAccount,
          userUsdcAccount: userUsdcAccount,
          chainlinkFeed: SOL_USD_FEED,
          chainlinkProgram: CHAINLINK_PROGRAM_ID,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        } as any)
        .signers([userAccount])
        .rpc();
      expect.fail("Expected an error");
    } catch (error) {
      expect((error as Error).message).to.include("Invalid LTV ratio");
    }
  });

  it("Withdraws SOL", async () => {
    const withdrawAmount = new anchor.BN(0.5 * LAMPORTS_PER_SOL);

    await program.methods.withdrawSol(withdrawAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        owner: userAccount.publicKey,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.solBalance.toNumber()).to.be.lessThan(1 * LAMPORTS_PER_SOL);
  });

  it("Fails to withdraw more SOL than available", async () => {
    const excessiveWithdrawAmount = new anchor.BN(10 * LAMPORTS_PER_SOL);

    try {
      await program.methods.withdrawSol(excessiveWithdrawAmount)
        .accounts({
          userAccount: userAccount.publicKey,
          owner: userAccount.publicKey,
          systemProgram: SystemProgram.programId,
        } as any)
        .signers([userAccount])
        .rpc();
      expect.fail("Expected an error");
    } catch (error) {
      expect((error as Error).message).to.include("Insufficient funds for withdrawal");
    }
  });

  it("Repays a loan partially", async () => {
    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    const loanId = account.loans[0].id;
    const partialRepayAmount = new anchor.BN(25 * 1000000); // Repay 25 USDC

    await program.methods.repayLoan(loanId, partialRepayAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        owner: userAccount.publicKey,
        contract: shrubPDA,
        contractUsdcAccount: shrubUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        usdcMint: usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const updatedAccount = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(updatedAccount.loans[0].principal.toNumber()).to.be.lessThan(account.loans[0].principal.toNumber());
  });

  it("Repays a loan fully", async () => {
    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    const loanId = account.loans[0].id;
    const fullRepayAmount = account.loans[0].principal;

    await program.methods.repayLoan(loanId, fullRepayAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        owner: userAccount.publicKey,
        contract: shrubPDA,
        contractUsdcAccount: shrubUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        usdcMint: usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const updatedAccount = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(updatedAccount.loans.length).to.equal(0);
  });

  it("Fails to repay a non-existent loan", async () => {
    const nonExistentLoanId = new anchor.BN(999);
    const repayAmount = new anchor.BN(10 * 1000000);

    try {
      await program.methods.repayLoan(nonExistentLoanId, repayAmount)
        .accounts({
          userAccount: userAccount.publicKey,
          owner: userAccount.publicKey,
          contract: shrubPDA,
          contractUsdcAccount: shrubUsdcAccount,
          userUsdcAccount: userUsdcAccount,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        } as any)
        .signers([userAccount])
        .rpc();
      expect.fail("Expected an error");
    } catch (error) {
      expect((error as Error).message).to.include("Loan not found");
    }
  });

  it("Takes multiple loans up to the maximum limit", async () => {
    const solAmount = new anchor.BN(1 * LAMPORTS_PER_SOL);
    const usdcAmount = new anchor.BN(10 * 1000000); // 10 USDC
    const ltv = 20; // 20% LTV

    for (let i = 0; i < 5; i++) {
      await program.methods.depositSolAndTakeLoan(solAmount, usdcAmount, ltv)
        .accounts({
          userAccount: userAccount.publicKey,
          owner: userAccount.publicKey,
          contract: shrubPDA,
          contractUsdcAccount: shrubUsdcAccount,
          userUsdcAccount: userUsdcAccount,
          chainlinkFeed: SOL_USD_FEED,
          chainlinkProgram: CHAINLINK_PROGRAM_ID,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        } as any)
        .signers([userAccount])
        .rpc();
    }

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.loans.length).to.equal(5);

    // Try to take one more loan
    try {
      await program.methods.depositSolAndTakeLoan(solAmount, usdcAmount, ltv)
        .accounts({
          userAccount: userAccount.publicKey,
          owner: userAccount.publicKey,
          contract: shrubPDA,
          contractUsdcAccount: shrubUsdcAccount,
          userUsdcAccount: userUsdcAccount,
          chainlinkFeed: SOL_USD_FEED,
          chainlinkProgram: CHAINLINK_PROGRAM_ID,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        } as any)
        .signers([userAccount])
        .rpc();
      expect.fail("Expected an error");
    } catch (error) {
      expect((error as Error).message).to.include("Maximum number of loans reached for this user");
    }
  });
});