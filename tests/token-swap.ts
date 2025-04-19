import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { TokenSwap } from "../target/types/token_swap";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
} from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  TOKEN_PROGRAM_ID,
  mintTo as splMintTo,
  createAccount as createTokenAccount,
} from "@solana/spl-token";
import { expect } from "chai";

describe("token-swap tests", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.TokenSwap as Program<TokenSwap>;

  // Test Accounts
  const admin = Keypair.generate();
  const user1 = Keypair.generate();
  const user2 = Keypair.generate();
  const feeCollector = Keypair.generate();
  let swapPool = Keypair.generate();
  const lpMint = Keypair.generate();

  // Test token mints and accounts
  let tokenAMint: PublicKey;
  let tokenBMint: PublicKey;

  let tokenAVault: PublicKey;
  let tokenBVault: PublicKey;
  let vaultABump: number;
  let vaultBBump: number;

  let adminTokenA: PublicKey;
  let adminTokenB: PublicKey;
  let user1TokenA: PublicKey;
  let user1TokenB: PublicKey;
  let user1LpToken: PublicKey;
  let user2TokenA: PublicKey;
  let user2TokenB: PublicKey;
  let user2LpToken: PublicKey;
  let feeCollectorTokenA: PublicKey;
  let feeCollectorTokenB: PublicKey;

  // Pool authority PDA
  let poolAuthority: PublicKey;
  let poolAuthorityBump: number;

  // Constants\
  const FEE_RATE = 30; // 0.3% fee
  const INITIAL_LIQUIDITY_A = 1_000_000_000; // 1,000 tokens (assuming 6 decimals)
  const INITIAL_LIQUIDITY_B = 2_000_000_000; // 2,000 tokens (assuming 6 decimals)
  const TOKEN_DECIMALS = 6;

  before(async () => {
    // Airdrop SOL to test accounts
    await provider.connection.requestAirdrop(admin.publicKey, 10_000_000_000);
    await provider.connection.requestAirdrop(user1.publicKey, 10_000_000_000);
    await provider.connection.requestAirdrop(user2.publicKey, 10_000_000_000);
    await provider.connection.requestAirdrop(
      feeCollector.publicKey,
      10_000_000_000
    );

    // Wait for confirmation
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Create token mints
    tokenAMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      TOKEN_DECIMALS,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    tokenBMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      TOKEN_DECIMALS,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    // Create token accounts for all users
    adminTokenA = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        admin,
        tokenAMint,
        admin.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    adminTokenB = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        admin,
        tokenBMint,
        admin.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    user1TokenA = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        user1,
        tokenAMint,
        user1.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    user1TokenB = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        user1,
        tokenBMint,
        user1.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    user2TokenA = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        user2,
        tokenAMint,
        user2.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    user2TokenB = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        user2,
        tokenBMint,
        user2.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    feeCollectorTokenA = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        feeCollector,
        tokenAMint,
        feeCollector.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    feeCollectorTokenB = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        feeCollector,
        tokenBMint,
        feeCollector.publicKey,
        false,
        undefined,
        undefined,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    ).address;

    // Mint initial tokens to users
    await splMintTo(
      provider.connection,
      admin,
      tokenAMint,
      user1TokenA,
      admin.publicKey,
      INITIAL_LIQUIDITY_A * 10,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    await splMintTo(
      provider.connection,
      admin,
      tokenBMint,
      user1TokenB,
      admin.publicKey,
      INITIAL_LIQUIDITY_B * 10,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    await splMintTo(
      provider.connection,
      admin,
      tokenAMint,
      user2TokenA,
      admin.publicKey,
      INITIAL_LIQUIDITY_A * 10,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    await splMintTo(
      provider.connection,
      admin,
      tokenBMint,
      user2TokenB,
      admin.publicKey,
      INITIAL_LIQUIDITY_B * 10,
      undefined,
      undefined,
      TOKEN_PROGRAM_ID
    );

    // Calculate pool authority PDA
    const [authority, bump] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("pool_authority"),
        tokenAMint.toBuffer(),
        tokenBMint.toBuffer(),
      ],
      program.programId
    );

    poolAuthority = authority;
    poolAuthorityBump = bump;

    console.log("hello5", authority.toString());

    // [tokenAVault, vaultABump] = PublicKey.findProgramAddressSync(
    //   [
    //     Buffer.from("token_vault"),
    //     poolAuthority.toBuffer(),
    //     tokenAMint.toBuffer(),
    //   ],
    //   program.programId
    // );
    // [tokenBVault, vaultBBump] = PublicKey.findProgramAddressSync(
    //   [
    //     Buffer.from("token_vault"),
    //     poolAuthority.toBuffer(),
    //     tokenBMint.toBuffer(),
    //   ],
    //   program.programId
    // );

    // Create token vaults
    // tokenAVault = await createTokenAccount(
    //   provider.connection,
    //   admin,
    //   tokenAMint,
    //   poolAuthority,
    //   undefined,
    //   undefined,
    //   TOKEN_PROGRAM_ID
    // );

    // console.log("hello6");

    // tokenBVault = await createTokenAccount(
    //   provider.connection,
    //   admin,
    //   tokenBMint,
    //   poolAuthority,
    //   undefined,
    //   undefined,
    //   TOKEN_PROGRAM_ID
    // );

    const [tokenAVaultAddress] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("token_vault"),
        poolAuthority.toBuffer(),
        tokenAMint.toBuffer(),
      ],
      program.programId
    );

    const [tokenBVaultAddress] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("token_vault"),
        poolAuthority.toBuffer(),
        tokenBMint.toBuffer(),
      ],
      program.programId
    );

    tokenAVault = tokenAVaultAddress;
    tokenBVault = tokenBVaultAddress;
  });

  it("Initialize pool", async () => {
    console.log("Starting simplified pool initialization");

    try {
      // Calculate PDA for pool authority
      const [poolAuthority, poolAuthorityBump] =
        PublicKey.findProgramAddressSync(
          [
            Buffer.from("pool_authority"),
            tokenAMint.toBuffer(),
            tokenBMint.toBuffer(),
          ],
          program.programId
        );

      console.log("Pool Authority:", poolAuthority.toString());

      // Create token A vault with a NEW keypair
      const tokenAVaultKeypair = Keypair.generate();
      console.log(
        "Token A Vault keypair:",
        tokenAVaultKeypair.publicKey.toString()
      );

      await createTokenAccount(
        provider.connection,
        admin,
        tokenAMint,
        poolAuthority,
        tokenAVaultKeypair // Use the keypair for creating the account
      );

      // Create token B vault with a NEW keypair
      const tokenBVaultKeypair = Keypair.generate();
      console.log(
        "Token B Vault keypair:",
        tokenBVaultKeypair.publicKey.toString()
      );

      await createTokenAccount(
        provider.connection,
        admin,
        tokenBMint,
        poolAuthority,
        tokenBVaultKeypair // Use the keypair for creating the account
      );

      // Explicitly check if the swap pool account exists
      const swapPoolInfo = await provider.connection.getAccountInfo(
        swapPool.publicKey
      );
      if (swapPoolInfo !== null) {
        console.log(
          "Warning: swapPool account already exists. Generating a new one..."
        );
        swapPool = Keypair.generate();
        console.log("New swapPool:", swapPool.publicKey.toString());
      }

      // Initialize the pool with manually created accounts
      await program.methods
        .initializePool(new anchor.BN(FEE_RATE), poolAuthorityBump)
        .accounts({
          swapPool: swapPool.publicKey,
          tokenAMint,
          tokenBMint,
          tokenAVault: tokenAVaultKeypair.publicKey,
          tokenBVault: tokenBVaultKeypair.publicKey,
          lpMint: lpMint.publicKey,
          poolAuthority,
          admin: admin.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: SYSVAR_RENT_PUBKEY,
        })
        .signers([admin, swapPool, lpMint]) // Include vault keypairs if needed
        .rpc();

      console.log("Pool initialized successfully");

      // Rest of your test
    } catch (err) {
      console.error("Error details:", err);
      if (err.logs) console.log("Logs:", err.logs);
      throw err;
    }

    // Verify pool initialization
    const poolAccount = await program.account.swapPool.fetch(
      swapPool.publicKey
    );
    expect(poolAccount.tokenAMint.toString()).to.equal(tokenAMint.toString());
  });
});
