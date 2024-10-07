import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolSavings } from "../target/types/sol_savings";
import { PublicKey, Keypair } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, createMint, createAssociatedTokenAccount, mintTo } from "@solana/spl-token";
import { expect } from 'chai';

describe('sol-savings', () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SolSavings as Program<SolSavings>;

  let userAccount: Keypair;
  let secondUserAccount: Keypair;
  let usdcMint: PublicKey;
  let contractUsdcAccount: PublicKey;
  let userUsdcAccount: PublicKey;
  let secondUserUsdcAccount: PublicKey;
  let contractAccount: Keypair;

  const CHAINLINK_PROGRAM_ID = new PublicKey("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny");
  const SOL_USD_FEED = new PublicKey("HgTtcbcmp5BeThax5AU8vg4VwK79qAvAKKFMs8txMLW6");

  before(async () => {
    userAccount = Keypair.generate();
    secondUserAccount = Keypair.generate();
    contractAccount = Keypair.generate();

    // Airdrop SOL to accounts
    await provider.connection.requestAirdrop(userAccount.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(secondUserAccount.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(contractAccount.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);

    // Create USDC mint
    usdcMint = await createMint(
      provider.connection,
      contractAccount,
      contractAccount.publicKey,
      null,
      6
    );

    // Create contract USDC account
    contractUsdcAccount = await createAssociatedTokenAccount(
      provider.connection,
      contractAccount,
      usdcMint,
      contractAccount.publicKey
    );

    // Create user USDC accounts
    userUsdcAccount = await createAssociatedTokenAccount(
      provider.connection,
      userAccount,
      usdcMint,
      userAccount.publicKey
    );
    secondUserUsdcAccount = await createAssociatedTokenAccount(
      provider.connection,
      secondUserAccount,
      usdcMint,
      secondUserAccount.publicKey
    );

    // Mint some USDC to the contract account
    await mintTo(
      provider.connection,
      contractAccount,
      usdcMint,
      contractUsdcAccount,
      contractAccount,
      1000000 * 1000000 // 1,000,000 USDC
    );
  });

  it("Initializes multiple user accounts", async () => {
    for (const account of [userAccount, secondUserAccount]) {
      await program.methods.initialize()
        .accounts({
          userAccount: account.publicKey,
          user: account.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        } as any)
        .signers([account])
        .rpc();

      const fetchedAccount = await program.account.userAccount.fetch(account.publicKey);
      expect(fetchedAccount.owner.toString()).to.equal(account.publicKey.toString());
      expect(fetchedAccount.solBalance.toNumber()).to.equal(0);
      expect(fetchedAccount.usdcBalance.toNumber()).to.equal(0);
    }
  });

  it("Fails to withdraw SOL with insufficient balance", async () => {
    const withdrawAmount = new anchor.BN(1 * anchor.web3.LAMPORTS_PER_SOL);

    try {
      await program.methods.withdrawSol(withdrawAmount)
        .accounts({
          userAccount: userAccount.publicKey,
          user: userAccount.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        } as any)
        .signers([userAccount])
        .rpc();
      expect.fail("Expected withdrawal to fail");
    } catch (error) {
      expect((error as Error).message).to.include("Insufficient funds for withdrawal");
    }
  });

  it("Takes maximum allowed loan based on LTV", async () => {
    const solAmount = new anchor.BN(5 * anchor.web3.LAMPORTS_PER_SOL);
    const ltv = 50; // 50% LTV
    const usdcAmount = new anchor.BN(250 * 1000000); // Assuming 1 SOL = $100, max loan would be 250 USDC

    await program.methods.depositSolAndTakeLoan(solAmount, usdcAmount, ltv)
      .accounts({
        userAccount: userAccount.publicKey,
        user: userAccount.publicKey,
        contract: contractAccount.publicKey,
        contractUsdcAccount: contractUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        chainlinkFeed: SOL_USD_FEED,
        chainlinkProgram: CHAINLINK_PROGRAM_ID,
        usdcMint: usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.loans.length).to.equal(1);
    expect(account.loans[0].ltv).to.equal(ltv);
    expect(account.usdcBalance.toNumber()).to.equal(usdcAmount.toNumber());
  });

  it("Fails to take a loan exceeding LTV limit", async () => {
    const solAmount = new anchor.BN(1 * anchor.web3.LAMPORTS_PER_SOL);
    const usdcAmount = new anchor.BN(51 * 1000000); // Trying to borrow more than 50% LTV
    const ltv = 50;

    try {
      await program.methods.depositSolAndTakeLoan(solAmount, usdcAmount, ltv)
        .accounts({
          userAccount: secondUserAccount.publicKey,
          user: secondUserAccount.publicKey,
          contract: contractAccount.publicKey,
          contractUsdcAccount: contractUsdcAccount,
          userUsdcAccount: secondUserUsdcAccount,
          chainlinkFeed: SOL_USD_FEED,
          chainlinkProgram: CHAINLINK_PROGRAM_ID,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        } as any)
        .signers([secondUserAccount])
        .rpc();
      expect.fail("Expected loan to fail due to exceeding LTV limit");
    } catch (error) {
      expect((error as Error).message).to.include("Insufficient collateral for loan");
    }
  });

  it("Partially repays a loan", async () => {
    const loanId = new anchor.BN(1); // Assuming this is the first loan taken
    const partialRepayAmount = new anchor.BN(100 * 1000000); // Repay 100 USDC

    await program.methods.repayLoan(loanId, partialRepayAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        user: userAccount.publicKey,
        contract: contractAccount.publicKey,
        contractUsdcAccount: contractUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        usdcMint: usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.loans.length).to.equal(1);
    expect(account.loans[0].principal.toNumber()).to.be.lessThan(250 * 1000000);
  });

  it("Fails to repay a non-existent loan", async () => {
    const nonExistentLoanId = new anchor.BN(999);
    const repayAmount = new anchor.BN(100 * 1000000);

    try {
      await program.methods.repayLoan(nonExistentLoanId, repayAmount)
        .accounts({
          userAccount: secondUserAccount.publicKey,
          user: secondUserAccount.publicKey,
          contract: contractAccount.publicKey,
          contractUsdcAccount: contractUsdcAccount,
          userUsdcAccount: secondUserUsdcAccount,
          usdcMint: usdcMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: anchor.web3.SystemProgram.programId,
        } as any)
        .signers([secondUserAccount])
        .rpc();
      expect.fail("Expected repayment to fail due to non-existent loan");
    } catch (error) {
      expect((error as Error).message).to.include("Loan not found");
    }
  });

  it("Fully repays a loan and retrieves collateral", async () => {
    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    const loanId = new anchor.BN(1);
    const fullRepayAmount = account.loans[0].principal;

    await program.methods.repayLoan(loanId, fullRepayAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        user: userAccount.publicKey,
        contract: contractAccount.publicKey,
        contractUsdcAccount: contractUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        usdcMint: usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const updatedAccount = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(updatedAccount.loans.length).to.equal(0);
    expect(updatedAccount.solBalance.toNumber()).to.be.greaterThan(account.solBalance.toNumber());
  });
});