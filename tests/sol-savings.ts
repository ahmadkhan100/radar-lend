import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from '@solana/web3.js';
import { TOKEN_PROGRAM_ID, createMint, getOrCreateAssociatedTokenAccount, mintTo} from '@solana/spl-token';
import { expect } from 'chai';

describe('sol-savings', () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.SolSavings as Program<any>;
  const owner = Keypair.generate();
  let userAccount: Keypair;
  let usdcMint: Token;
  let userUsdcAccount: PublicKey;
  let contractUsdcAccount: PublicKey;

  before(async () => {
    // Airdrop SOL to owner
    await provider.connection.requestAirdrop(owner.publicKey, 10 * LAMPORTS_PER_SOL);

    // Create USDC mint
    usdcMint = await Token.createMint(
      provider.connection,
      owner,
      owner.publicKey,
      null,
      6,
      TOKEN_PROGRAM_ID
    );

    // Create user USDC account
    userUsdcAccount = await usdcMint.createAccount(owner.publicKey);

    // Create contract USDC account
    contractUsdcAccount = await usdcMint.createAccount(program.programId);
  });

  it('Initializes user account', async () => {
    userAccount = Keypair.generate();
    await program.methods.initialize()
      .accounts({
        userAccount: userAccount.publicKey,
        owner: owner.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([owner, userAccount])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.owner.toString()).to.equal(owner.publicKey.toString());
    expect(account.solBalance.toNumber()).to.equal(0);
    expect(account.usdcBalance.toNumber()).to.equal(0);
    expect(account.loanCount.toNumber()).to.equal(0);
  });

  it('Deposits SOL and takes a loan', async () => {
    const solAmount = new anchor.BN(1 * LAMPORTS_PER_SOL);
    const usdcAmount = new anchor.BN(50 * 10**6); // 50 USDC
    const ltv = 50; // 50% LTV

    await program.methods.depositSolAndTakeLoan(solAmount, usdcAmount, ltv)
      .accounts({
        userAccount: userAccount.publicKey,
        owner: owner.publicKey,
        contract: program.programId,
        contractUsdcAccount: contractUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        usdcMint: usdcMint.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([owner])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.solBalance.toNumber()).to.be.greaterThan(0);
    expect(account.usdcBalance.toNumber()).to.equal(usdcAmount.toNumber());
    expect(account.loanCount.toNumber()).to.equal(1);
    expect(account.loans.length).to.equal(1);
  });

  it('Repays a loan', async () => {
    const repayAmount = new anchor.BN(25 * 10**6); // 25 USDC
    const loanId = new anchor.BN(1);

    await program.methods.repayLoan(loanId, repayAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        owner: owner.publicKey,
        contract: program.programId,
        contractUsdcAccount: contractUsdcAccount,
        userUsdcAccount: userUsdcAccount,
        usdcMint: usdcMint.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([owner])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.usdcBalance.toNumber()).to.be.lessThan(50 * 10**6);
    expect(account.loans[0].principal.toNumber()).to.be.lessThan(50 * 10**6);
  });

  it('Withdraws SOL', async () => {
    const withdrawAmount = new anchor.BN(0.5 * LAMPORTS_PER_SOL);
    await program.methods.withdrawSol(withdrawAmount)
      .accounts({
        userAccount: userAccount.publicKey,
        owner: owner.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([owner])
      .rpc();

    const account = await program.account.userAccount.fetch(userAccount.publicKey);
    expect(account.solBalance.toNumber()).to.be.lessThan(1 * LAMPORTS_PER_SOL);
  });
});