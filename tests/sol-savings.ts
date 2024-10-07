import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolSavings } from "../target/types/sol_savings";
import { PublicKey, Keypair } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, createMint, createAssociatedTokenAccount, getAssociatedTokenAddress } from "@solana/spl-token";
import { expect } from 'chai';

describe('sol-savings', () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SolSavings as Program<SolSavings>;

  let userAccount: Keypair;
  let usdcMint: PublicKey;
  let contractUsdcAccount: PublicKey;
  let userUsdcAccount: PublicKey;
  let contractAccount: Keypair;

  const CHAINLINK_PROGRAM_ID = new PublicKey("HEvSKofvBgfaexv23kMabbYqxasxU3mQ4ibBMEmJWHny");
  const SOL_USD_FEED = new PublicKey("HgTtcbcmp5BeThax5AU8vg4VwK79qAvAKKFMs8txMLW6");

  before(async () => {
    userAccount = Keypair.generate();
    contractAccount = Keypair.generate();

    // Airdrop SOL to user account and contract account
    await provider.connection.requestAirdrop(userAccount.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
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

    // Create user USDC account
    userUsdcAccount = await createAssociatedTokenAccount(
      provider.connection,
      userAccount,
      usdcMint,
      userAccount.publicKey
    );
  });

  it("Initializes user account", async () => {
    await program.methods.initialize()
      .accounts({
        userAccount: userAccount.publicKey,
        user: userAccount.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.owner.toString()).to.equal(userAccount.publicKey.toString());
    expect(account.solBalance.toNumber()).to.equal(0);
    expect(account.usdcBalance.toNumber()).to.equal(0);
  });

  it("Creates USDC mint", async () => {
    await program.methods.createUsdcMint()
      .accounts({
        contract: contractAccount.publicKey,
        usdcMint: usdcMint,
        contractUsdcAccount: contractUsdcAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      } as any)
      .signers([contractAccount])
      .rpc();

    const mintInfo = await provider.connection.getTokenSupply(usdcMint);
    expect(mintInfo.value.uiAmount).to.equal(1000000); // 1,000,000 USDC
  });

  const ltvOptions = [20, 25, 33, 50];
  
  for (const ltv of ltvOptions) {
    it(`Takes a USDC loan with SOL collateral (LTV ${ltv}%)`, async () => {
      const solAmount = new anchor.BN(1 * anchor.web3.LAMPORTS_PER_SOL);
      const usdcAmount = new anchor.BN(ltv * 10000); // ltv% of 1 SOL worth of USDC

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

      // Repay the loan immediately for the next test
      await program.methods.repayLoan(new anchor.BN(account.loanCount), usdcAmount)
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
    });
  }

  it("Withdraws SOL", async () => {
    const withdrawAmount = new anchor.BN(0.5 * anchor.web3.LAMPORTS_PER_SOL);

    await program.methods.withdrawSol(withdrawAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        user: userAccount.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      } as any)
      .signers([userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.solBalance.toNumber()).to.be.lessThan(4 * anchor.web3.LAMPORTS_PER_SOL); // Less than 4 SOL due to previous deposits and withdrawals
  });

  it("Repays a loan", async () => {
    // First, take out a loan
    const solAmount = new anchor.BN(1 * anchor.web3.LAMPORTS_PER_SOL);
    const usdcAmount = new anchor.BN(20 * 1000000); // 20 USDC
    const ltv = 20;

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

    // Now repay the loan
    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    const loanId = new anchor.BN(account.loanCount);
    const repayAmount = usdcAmount; // Repay full amount

    await program.methods.repayLoan(loanId, repayAmount)
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
    expect(updatedAccount.loans.length).to.equal(0); // Loan should be fully repaid and removed
    expect(updatedAccount.usdcBalance.toNumber()).to.equal(0);
  });
});