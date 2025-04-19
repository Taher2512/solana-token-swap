use anchor_lang::prelude::{borsh::de, *};

use anchor_spl::{associated_token::AssociatedToken, token_interface::{burn, mint_to, transfer_checked, sync_native as native_sync_native, SyncNative as NativeSyncNative, Burn, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked}};
use crate::error::CustomError;

pub mod error;

declare_id!("AxqzHPnPm5Es17u3PuNHTvU2ivgYvZbzFgEgPiaH7Vj8");

#[program]
pub mod token_swap {
    use anchor_lang::Result;

    use super::*;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        fee_rate: u64,
        bump: u8,
    ) -> Result<()> {
        msg!("Initializing token swap pool with simplified access");
    
        // Validate fee rate
        require!(fee_rate <= 1000, CustomError::FeeTooHigh);
        
        // Get a reference to the swap pool
        let swap_pool = &mut ctx.accounts.swap_pool;
        
        // Copy data from accounts to the swap pool one by one very carefully
        let token_a_mint = ctx.accounts.token_a_mint.to_account_info().key();
        msg!("Token A mint key copied: {}", token_a_mint);
        swap_pool.token_a_mint = token_a_mint;
        
        let token_b_mint = ctx.accounts.token_b_mint.to_account_info().key();
        msg!("Token B mint key copied: {}", token_b_mint);
        swap_pool.token_b_mint = token_b_mint;
        
        swap_pool.token_b_vault = ctx.accounts.token_b_vault.key();
        swap_pool.lp_mint = ctx.accounts.lp_mint.key();
        swap_pool.pool_authority = ctx.accounts.pool_authority.key();
        swap_pool.fee_rate = fee_rate;
        swap_pool.bump = bump;
        swap_pool.is_paused = false;
        swap_pool.admin = ctx.accounts.admin.key();
        swap_pool.total_fees_a = 0;
        swap_pool.total_fees_b = 0;
        
        msg!("Token swap pool initialized");
    
        Ok(())
    }

    pub fn add_initial_liquidity(
        ctx: Context<AddInitialLiquidity>,
        amount_a: u64,
        amount_b: u64,
    ) -> Result<()> {
        require!(amount_a > 0 && amount_b > 0, CustomError::InvalidAmount);
        
        // Transfer token A from user to pool
        let transfer_a_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.user_token_a.to_account_info(),
                to: ctx.accounts.token_a_vault.to_account_info(),
                authority: ctx.accounts.user_authority.to_account_info(),
                mint: ctx.accounts.token_a_mint.to_account_info(),
            },
        );

        transfer_checked(
            transfer_a_ctx,
            amount_a,
            ctx.accounts.token_a_mint.decimals
        )?;
        
        // Transfer token B from user to pool
        let transfer_b_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.user_token_b.to_account_info(),
                to: ctx.accounts.token_b_vault.to_account_info(),
                authority: ctx.accounts.user_authority.to_account_info(),
                mint: ctx.accounts.token_b_mint.to_account_info(),
            },
        );

        transfer_checked(
            transfer_b_ctx,
            amount_b,
            ctx.accounts.token_b_mint.decimals
        )?;

        // Initial LP tokens are the geometric mean of token amounts
        // This encourages balanced liquidity provision
        let initial_lp_amount = (amount_a as f64).sqrt() * (amount_b as f64).sqrt();
        let initial_lp_tokens = initial_lp_amount as u64;

        // Mint LP tokens to user
        let seeds = &[
            b"pool_authority".as_ref(),
            ctx.accounts.swap_pool.token_a_mint.as_ref(),
            ctx.accounts.swap_pool.token_b_mint.as_ref(),
            &[ctx.accounts.swap_pool.bump],
        ];
        let signer = &[&seeds[..]];

        let mint_lp_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.user_lp_token.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer,
        );

        mint_to (
           mint_lp_ctx,
           initial_lp_tokens, 
        );
        
        Ok(())
    }

    pub fn add_liquidity(
        ctx: Context<AddLiquidity>,
        amount_a_desired: u64,
        amount_b_desired: u64,
        amount_a_min: u64,
        amount_b_min: u64
    ) -> Result<()> {
        require!(!ctx.accounts.swap_pool.is_paused, CustomError::PoolPaused);
        require!(amount_a_desired > 0 && amount_b_desired > 0, CustomError::InvalidAmount);

        let reserve_a = ctx.accounts.token_a_vault.amount;
        let reserve_b = ctx.accounts.token_b_vault.amount;
        let total_lp_supply = ctx.accounts.lp_mint.supply;

        require!(reserve_a > 0 && reserve_b > 0, CustomError::InsufficientLiquidity);

        // Calculate amounts to actually transfer based on current ratio
        let amount_b_optimal = (amount_a_desired as u128)
            .checked_mul(reserve_b as u128)
            .unwrap()
            .checked_div(reserve_a as u128)
            .unwrap() as u64;

        let (amount_a, amount_b) = if amount_b_desired >= amount_b_optimal {
            let amount_a = amount_a_desired;
            let amount_b = amount_b_optimal;

            require!(amount_b >= amount_b_min, CustomError::SlippageExceeded);
            (amount_a, amount_b)
        } else {
            let amount_a_optimal = (amount_b_desired as u128)
                .checked_mul(reserve_a as u128)
                .unwrap()
                .checked_div(reserve_b as u128)
                .unwrap() as u64;

            let amount_a = amount_a_optimal;
            let amount_b = amount_b_desired;

            require!(amount_a >= amount_a_min, CustomError::SlippageExceeded);
            (amount_a, amount_b)
        };

        // Transfer token A from user to pool
        let transfer_a_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.user_token_a.to_account_info(),
                to: ctx.accounts.token_a_vault.to_account_info(),
                authority: ctx.accounts.user_authority.to_account_info(),
                mint: ctx.accounts.token_a_mint.to_account_info(),
            },
        );
        transfer_checked(
            transfer_a_ctx,
            amount_a,
            ctx.accounts.token_a_mint.decimals
        )?;

        // Transfer token B from user to pool
        let transfer_b_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.user_token_b.to_account_info(),
                to: ctx.accounts.token_b_vault.to_account_info(),
                authority: ctx.accounts.user_authority.to_account_info(),
                mint: ctx.accounts.token_b_mint.to_account_info(),
            },
        );
        transfer_checked(
            transfer_b_ctx,
            amount_b,
            ctx.accounts.token_b_mint.decimals
        )?;

        // Calculate LP tokens to mint
        // The formula uses the minimum ratio to ensure fair distribution
        let lp_amount_a = (amount_a as u128)
            .checked_mul(total_lp_supply as u128)
            .unwrap()
            .checked_div(reserve_a as u128)
            .unwrap() as u64;

        let lp_amount_b = (amount_b as u128)
            .checked_mul(total_lp_supply as u128)
            .unwrap()
            .checked_div(reserve_b as u128)
            .unwrap() as u64;

        let lp_to_mint = std::cmp::min(lp_amount_a, lp_amount_b);

        // Mint LP tokens to user
        let seeds= &[
            b"pool_authority".as_ref(),
            ctx.accounts.swap_pool.token_a_mint.as_ref(),
            ctx.accounts.swap_pool.token_b_mint.as_ref(),
            &[ctx.accounts.swap_pool.bump],
        ];
        let signer = &[&seeds[..]];

        let mint_lp_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.user_lp_token.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer
        );

        mint_to (
            mint_lp_ctx,
            lp_to_mint,
        )?;

        Ok(())
    }

    pub fn remove_liquidity(
        ctx: Context<RemoveLiquidity>,
        lp_amount: u64,
        amount_a_min: u64,
        amount_b_min: u64,
    ) -> Result<()> {
        require!(!ctx.accounts.swap_pool.is_paused, CustomError::PoolPaused);
        require!(lp_amount > 0, CustomError::InvalidAmount);

        // Get current reserves and total supply
        let reserve_a = ctx.accounts.token_a_vault.amount;
        let reserve_b = ctx.accounts.token_b_vault.amount;
        let total_lp_supply = ctx.accounts.lp_mint.supply;

        // Calculate share of pool being withdrawn
        let amount_a = (lp_amount as u128)
            .checked_mul(reserve_a as u128)
            .unwrap()
            .checked_div(total_lp_supply as u128)
            .unwrap() as u64;

        let amount_b = (lp_amount as u128)
            .checked_mul(reserve_b as u128)
            .unwrap()
            .checked_div(total_lp_supply as u128)
            .unwrap() as u64;

        require!(amount_a >= amount_a_min, CustomError::SlippageExceeded);
        require!(amount_b >= amount_b_min, CustomError::SlippageExceeded);

        // Burn LP tokens
        let seeds = &[
            b"pool_authority".as_ref(),
            ctx.accounts.swap_pool.token_a_mint.as_ref(),
            ctx.accounts.swap_pool.token_b_mint.as_ref(),
            &[ctx.accounts.swap_pool.bump],
        ];
        let signer = &[&seeds[..]];

        let burn_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.lp_mint.to_account_info(),
                from: ctx.accounts.user_lp_token.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
            },
            signer
        );
        burn(burn_ctx, lp_amount)?;

        // Transfer tokens from pool to user
        // Transfer token A
        let transfer_a_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.token_a_vault.to_account_info(),
                to: ctx.accounts.user_token_a.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
                mint: ctx.accounts.token_a_mint.to_account_info(),
            },
            signer
        );
        transfer_checked(
            transfer_a_ctx,
            amount_a,
            ctx.accounts.token_a_mint.decimals
        )?;

        // Transfer token B
        let transfer_b_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.token_b_vault.to_account_info(),
                to: ctx.accounts.user_token_b.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
                mint: ctx.accounts.token_b_mint.to_account_info(),
            },
            signer
        );
        transfer_checked(
            transfer_b_ctx,
            amount_b,
            ctx.accounts.token_b_mint.decimals
        )?;

        Ok(())
    }

    pub fn swap(
        ctx: Context<Swap>,
        amount_in: u64,
        min_amount_out: u64
    ) -> Result<()> {
        require!(!ctx.accounts.swap_pool.is_paused, CustomError::PoolPaused);
        require!(amount_in > 0, CustomError::InvalidAmount);

        let swap_pool = &mut ctx.accounts.swap_pool;
        let token_a_vault = &ctx.accounts.token_a_vault;
        let token_b_vault = &ctx.accounts.token_b_vault;
        let user_token_a = &ctx.accounts.user_token_a;
        let user_token_b = &ctx.accounts.user_token_b;
        let token_a_mint = &ctx.accounts.token_a_mint;
        let token_b_mint = &ctx.accounts.token_b_mint;

        let (input_amount, input_token_account, redeem_token_account, input_token_vault, redeem_token_vault, input_token_mint, redeem_token_mint, is_a_to_b) =
        if user_token_a.mint == swap_pool.token_a_mint {
            (amount_in, user_token_a, user_token_b, token_a_vault, token_b_vault, token_a_mint, token_b_mint, true)
        } else if user_token_b.mint == swap_pool.token_b_mint {
            (amount_in, user_token_b, user_token_a, token_b_vault, token_a_vault, token_b_mint, token_a_mint, false)
        } else {
            return Err(CustomError::InvalidToken.into());
        };

        let input_token_vault_amount = input_token_vault.amount;
        let redeem_token_vault_amount = redeem_token_vault.amount;

        let new_input_token_vault_amount = input_token_vault_amount.checked_add(input_amount)
            .ok_or(CustomError::InvalidAmount)?;

        let new_redeem_token_vault_amount = input_token_vault_amount.checked_mul(redeem_token_vault_amount).ok_or(CustomError::InvalidAmount)?.checked_div(new_input_token_vault_amount).ok_or(CustomError::InvalidAmount)?;

        let amount_to_redeem = redeem_token_vault_amount.checked_sub(new_redeem_token_vault_amount)
            .ok_or(CustomError::InvalidAmount)?;

        let fee_amount = amount_to_redeem.checked_mul(swap_pool.fee_rate).ok_or(CustomError::InvalidAmount)?.checked_div(10000).ok_or(CustomError::InvalidAmount)?;

        let final_amount_to_redeem = amount_to_redeem.checked_sub(fee_amount).ok_or(CustomError::InvalidAmount)?;

        if is_a_to_b {
            swap_pool.total_fees_b = swap_pool.total_fees_b.checked_add(fee_amount).ok_or(CustomError::InvalidAmount)?;   
        } else {
            swap_pool.total_fees_a = swap_pool.total_fees_a.checked_add(fee_amount).ok_or(CustomError::InvalidAmount)?;
        }

        require!(final_amount_to_redeem >= min_amount_out, CustomError::SlippageExceeded);

        let transfer_from_user_cpi = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: input_token_account.to_account_info(),
                to: input_token_vault.to_account_info(),
                authority: ctx.accounts.user_authority.to_account_info(),
                mint: input_token_mint.to_account_info(),
            }
        );

        transfer_checked(transfer_from_user_cpi, input_amount, input_token_mint.decimals)?;

        let seeds = &[
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref(),
            &[swap_pool.bump],
        ];
        let signer = &[&seeds[..]];

        let transfer_to_user_cpi = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: redeem_token_vault.to_account_info(),
                to: redeem_token_account.to_account_info(),
                authority: ctx.accounts.pool_authority.to_account_info(),
                mint: redeem_token_mint.to_account_info(),
            },
            signer
        );

        transfer_checked(transfer_to_user_cpi, final_amount_to_redeem, redeem_token_mint.decimals)?;

        Ok(())
    }

    pub fn collect_fees(ctx: Context<CollectFees>) -> Result<()> {
        require!(ctx.accounts.fee_collector.key() == ctx.accounts.swap_pool.admin, CustomError::Unauthorized);

        let swap_pool = &mut ctx.accounts.swap_pool;
        let fee_amount_a = swap_pool.total_fees_a;
        let fee_amount_b = swap_pool.total_fees_b;

        // Reset fee counters
        swap_pool.total_fees_a = 0;
        swap_pool.total_fees_b = 0;

        if fee_amount_a > 0 {
            let seeds = &[
                b"pool_authority".as_ref(),
                swap_pool.token_a_mint.as_ref(),
                swap_pool.token_b_mint.as_ref(),
                &[swap_pool.bump],
            ];
            let signer = &[&seeds[..]];

            let transfer_a_cpi = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.token_a_vault.to_account_info(),
                    to: ctx.accounts.fee_collector_token_a.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info(),
                    mint: ctx.accounts.token_a_mint.to_account_info(),
                },
                signer
            );
            transfer_checked(
                transfer_a_cpi,
                fee_amount_a,
                ctx.accounts.token_a_mint.decimals
            )?;
        }

        if fee_amount_b > 0 {
            let seeds = &[
                b"pool_authority".as_ref(),
                swap_pool.token_a_mint.as_ref(),
                swap_pool.token_b_mint.as_ref(),
                &[swap_pool.bump],
            ];
            let signer = &[&seeds[..]];

            let transfer_b_cpi = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.token_b_vault.to_account_info(),
                    to: ctx.accounts.fee_collector_token_b.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info(),
                    mint: ctx.accounts.token_b_mint.to_account_info(),
                },
                signer
            );
            transfer_checked(
                transfer_b_cpi,
                fee_amount_b,
                ctx.accounts.token_b_mint.decimals
            )?;
        }

        Ok(())
    }

    pub fn set_paused(ctx: Context<AdminAction>, paused: bool) -> Result<()> {
        require!(ctx.accounts.admin.key() == ctx.accounts.swap_pool.admin, CustomError::Unauthorized);

        ctx.accounts.swap_pool.is_paused = paused;
        Ok(())
    }

    pub fn update_fee_rate(ctx: Context<AdminAction>, new_fee_rate: u64) -> Result<()> {
        require!(ctx.accounts.admin.key() == ctx.accounts.swap_pool.admin, CustomError::Unauthorized);
        require!(new_fee_rate <= 1000, CustomError::FeeTooHigh); // Max fee of 10%

        ctx.accounts.swap_pool.fee_rate = new_fee_rate;
        Ok(())
    }

    pub fn transfer_admin(ctx: Context<TransferAdmin>, new_admin: Pubkey) -> Result<()> {
        require!(ctx.accounts.admin.key() == ctx.accounts.swap_pool.admin, CustomError::Unauthorized);

        ctx.accounts.swap_pool.admin = new_admin;
        Ok(())
    }

    // Get token prices
    pub fn get_token_a_price(ctx: Context<GetPrice>) -> Result<u64> {
        let token_a_amount = ctx.accounts.token_a_vault.amount;
        let token_b_amount = ctx.accounts.token_b_vault.amount;

        require!(token_a_amount > 0, CustomError::InsufficientLiquidity);

        // Price of toeken A in terms of token B (scaled by 10^6 for precision)
        let price = (token_b_amount as u128)
            .checked_mul(1_000_000)
            .unwrap()
            .checked_div(token_a_amount as u128)
            .unwrap() as u64;

        Ok(price)
    }
    pub fn get_token_b_price(ctx: Context<GetPrice>) -> Result<u64> {
        let token_a_amount = ctx.accounts.token_a_vault.amount;
        let token_b_amount = ctx.accounts.token_b_vault.amount;

        require!(token_b_amount > 0, CustomError::InsufficientLiquidity);

        // Price of token B in terms of token A (scaled by 10^6 for precision)
        let price = (token_a_amount as u128)
            .checked_mul(1_000_000)
            .unwrap()
            .checked_div(token_b_amount as u128)
            .unwrap() as u64;

        Ok(price)
    }

    // Get total liquidity of both tokens and current LP supply
    pub fn get_pool_stats(ctx: Context<GetPoolStats>) -> Result<(u64, u64, u64)> {
        let token_a_amount = ctx.accounts.token_a_vault.amount;
        let token_b_amount = ctx.accounts.token_b_vault.amount;
        let lp_supply = ctx.accounts.lp_mint.supply;

        Ok((token_a_amount, token_b_amount, lp_supply))
    }

    // Calculate swap result without executing it
    pub fn calculate_swap_result(ctx: Context<GetPrice>, amount_in: u64, is_a_to_b: bool) -> Result<(u64)> {
        let swap_pool = &ctx.accounts.swap_pool;
        
        let source_amount = if is_a_to_b {
            ctx.accounts.token_a_vault.amount
        } else {
            ctx.accounts.token_b_vault.amount
        };

        let destination_amount = if is_a_to_b {
            ctx.accounts.token_b_vault.amount
        } else {
            ctx.accounts.token_a_vault.amount
        };

        let new_source_amount = source_amount.checked_add(amount_in).ok_or(error::CustomError::CalculationFailure)?;

        let constant_product = source_amount.checked_mul(destination_amount).ok_or(error::CustomError::CalculationFailure)?;

        let new_destination_amount = constant_product.checked_div(new_source_amount).ok_or(error::CustomError::CalculationFailure)?;

        let output_amount = destination_amount.checked_sub(new_destination_amount).ok_or(error::CustomError::CalculationFailure)?;

        let fee_amount = output_amount.checked_mul(swap_pool.fee_rate).ok_or(CustomError::CalculationFailure)?.checked_div(10000).ok_or(CustomError::CalculationFailure)?;

        let final_output_amount = output_amount.checked_sub(fee_amount).ok_or(CustomError::CalculationFailure)?;

        Ok(final_output_amount)
    }

    // Function to get the latest trade volume (could be expanded with more tracking in SwapPool)
    pub fn get_pool_volume(_ctx: Context<GetPoolStats>) -> Result<(u64, u64)> {
        // This would need additional state tracking in the SwapPool account
        // For now, returns zeros as placeholder
        // To implement properly, add volume tracking to the SwapPool struct
        // and update it in the swap function
        Ok((0, 0))
    }

    pub fn get_user_pool_share(ctx: Context<GetUserShare>) -> Result<(u64, u64, u64)> {
        let token_a_vault_amount = ctx.accounts.token_a_vault.amount;
        let token_b_vault_amount = ctx.accounts.token_b_vault.amount;
        let lp_total_supply = ctx.accounts.lp_mint.supply;
        let user_lp_balance = ctx.accounts.user_lp_token.amount;

        // Calculate user's share in percentage (scaled by 10^6 for precision)
        let user_share_percentage = if lp_total_supply == 0 {
            0
        } else {
            (user_lp_balance as u128)
                .checked_mul(1_000_000)
                .unwrap()
                .checked_div(lp_total_supply as u128)
                .unwrap() as u64
        };

        // Calculate user's share of tokens
        let user_token_a_share = if lp_total_supply == 0 {
            0
        } else {
            (user_lp_balance as u128)
                .checked_mul(token_a_vault_amount as u128)
                .unwrap()
                .checked_div(lp_total_supply as u128)
                .unwrap() as u64
        };
        let user_token_b_share = if lp_total_supply == 0 {
            0
        } else {
            (user_lp_balance as u128)
                .checked_mul(token_b_vault_amount as u128)
                .unwrap()
                .checked_div(lp_total_supply as u128)
                .unwrap() as u64
        };

        Ok((user_share_percentage, user_token_a_share, user_token_b_share))
    }

    // Function to create wrapper for sync native instruction (for SOL pools)
    pub fn sync_native(ctx: Context<SyncNative>) -> Result<()> {
        require!(!ctx.accounts.swap_pool.is_paused, CustomError::PoolPaused);

        // This is used when one of the tokens is wrapped SOL
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            NativeSyncNative {
                account: ctx.accounts.token_account.to_account_info(),
            },
        );

        native_sync_native(cpi_ctx)?;

        Ok(())
    }

    // Function to update pool implementation (for future upgrades)
    pub fn update_pool_version(ctx: Context<AdminAction>, _new_version: u8) -> Result<()> {
        require!(
            ctx.accounts.admin.key() == ctx.accounts.swap_pool.admin,
            CustomError::Unauthorized
        );
        
        // This is a placeholder for future upgradability
        // Add version field to SwapPool struct to track upgrades
        // For now, this just verifies admin authority
        
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct SwapPool {
    pub token_a_mint: Pubkey,       // Mint address of token A
    pub token_b_mint: Pubkey,       // Mint address of token B
    pub token_a_vault: Pubkey,      // Vault holding token A liquidity
    pub token_b_vault: Pubkey,      // Vault holding token B liquidity
    pub lp_mint: Pubkey,            // Mint for LP tokens
    pub pool_authority: Pubkey,     // PDA with authority over vaults
    pub fee_rate: u64,              // Fee taken on swaps (basis points)
    pub bump: u8,                   // Bump for PDA derivation
    pub is_paused: bool,            // Emergency pause flag
    pub admin: Pubkey,              // Admin address that can pause/unpause
    pub total_fees_a: u64,          // Accumulated fees in token A
    pub total_fees_b: u64,          // Accumulated fees in token B
}

#[derive(Accounts)]
#[instruction(fee_rate: u64, bump: u8)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + 32 + 32 + 32 + 32 + 32 + 32 + 8 +  1 +  1 +  32 + 8 + 8,
    )]
    pub swap_pool: Account<'info, SwapPool>,

    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,

    // #[account(
    //     mut,
    //     constraint = token_a_vault.mint == token_a_mint.key(),
    //     constraint = token_a_vault.owner == pool_authority.key(),
    // )]
    // pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    // #[account(
    //     mut,
    //     constraint = token_b_vault.mint == token_b_mint.key(),
    //     constraint = token_b_vault.owner == pool_authority.key(),
    // )]
    // pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    // #[account(
    //     init,
    //     payer = admin,
    //     seeds = [
    //         b"token_vault".as_ref(),
    //         pool_authority.key().as_ref(),
    //         token_a_mint.key().as_ref(),
    //     ],
    //     bump,
    //     token::mint = token_a_mint,
    //     token::authority = pool_authority,
    // )]
    // pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    // #[account(
    //     init,
    //     payer = admin,
    //     seeds = [
    //         b"token_vault".as_ref(),
    //         pool_authority.key().as_ref(),
    //         token_b_mint.key().as_ref(),
    //     ],
    //     bump,
    //     token::mint = token_b_mint,
    //     token::authority = pool_authority,
    // )]
    // pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    // #[account(
    //     init, 
    //     payer = admin,
    //     associated_token::mint = token_a_mint,
    //     associated_token::authority = pool_authority,
    //     associated_token::token_program = token_program,
    //   )]
    //   pub token_a_vault: InterfaceAccount<'info, TokenAccount>,
      
    //   #[account(
    //     init, 
    //     payer = admin,
    //     associated_token::mint = token_b_mint,
    //     associated_token::authority = pool_authority,
    //     associated_token::token_program = token_program,
    //   )]
    //   pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        constraint = token_a_vault.mint == token_a_mint.key(),
        constraint = token_a_vault.owner == pool_authority.key(),
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        constraint = token_b_vault.mint == token_b_mint.key(),
        constraint = token_b_vault.owner == pool_authority.key(),
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = pool_authority,
    )]
    pub lp_mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            token_a_mint.key().as_ref(),
            token_b_mint.key().as_ref(),
        ],
        bump = bump,
    )]
    /// CHECK: PDA that will have authority over the token vaults
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct AddInitialLiquidity<'info> {
    #[account(mut)]
    pub swap_pool: Account<'info, SwapPool>,

    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key(),
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key(),
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_a.mint == swap_pool.token_a_mint,
        constraint = user_token_a.owner == user_authority.key(),
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_b.mint == swap_pool.token_b_mint,
        constraint = user_token_b.owner == user_authority.key(),
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = lp_mint.key() == swap_pool.lp_mint,
    )]
    pub lp_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user_authority,
        associated_token::mint = lp_mint,
        associated_token::authority = user_authority,
    )]
    pub user_lp_token: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref(),
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub user_authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Swap<'info> {
    pub swap_pool: Account<'info, SwapPool>,

    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key(),
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key(),
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_a.mint == swap_pool.token_a_mint,
        constraint = user_token_a.owner == user_authority.key(),
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_b.mint == swap_pool.token_b_mint,
        constraint = user_token_b.owner == user_authority.key(),
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref(),
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub user_authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    #[account(mut)]
    pub swap_pool: Account<'info, SwapPool>,

    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key(),
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key(),
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_a.mint == swap_pool.token_a_mint,
        constraint = user_token_a.owner == user_authority.key(),
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_b.mint == swap_pool.token_b_mint,
        constraint = user_token_b.owner == user_authority.key(),
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = lp_mint.key() == swap_pool.lp_mint
    )]
    pub lp_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user_authority,
        associated_token::mint = lp_mint,
        associated_token::authority = user_authority,
    )]
    pub user_lp_token: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref(),
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub user_authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct RemoveLiquidity<'info> {
    #[account(mut)]
    pub swap_pool: Account<'info, SwapPool>,
    
    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,
    
    #[account(
        mut,
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key()
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key()
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = user_token_a.mint == swap_pool.token_a_mint,
        constraint = user_token_a.owner == user_authority.key()
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = user_token_b.mint == swap_pool.token_b_mint,
        constraint = user_token_b.owner == user_authority.key()
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,
    
    #[account(
        mut,
        constraint = lp_mint.key() == swap_pool.lp_mint
    )]
    pub lp_mint: InterfaceAccount<'info, Mint>,
    
    #[account(
        mut,
        constraint = user_lp_token.mint == lp_mint.key(),
        constraint = user_lp_token.owner == user_authority.key()
    )]
    pub user_lp_token: InterfaceAccount<'info, TokenAccount>,
    
    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref()
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,
    
    #[account(mut)]
    pub user_authority: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CollectFees<'info> {
    #[account(mut)]
    pub swap_pool: Account<'info, SwapPool>,

    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key()
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key()
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,
    
    #[account(mut)]
    pub fee_collector: Signer<'info>,

    #[account(
        mut,
        constraint = fee_collector_token_a.mint == swap_pool.token_a_mint,
        constraint = fee_collector_token_a.owner == fee_collector.key()
    )]
    pub fee_collector_token_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = fee_collector_token_b.mint == swap_pool.token_b_mint,
        constraint = fee_collector_token_b.owner == fee_collector.key()
    )]
    pub fee_collector_token_b: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref()
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct AdminAction<'info> {
    #[account(mut)]
    pub swap_pool: Account<'info, SwapPool>,

    #[account(mut)]
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct TransferAdmin<'info> {
    #[account(mut)]
    pub swap_pool: Account<'info, SwapPool>,

    #[account(mut)]
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct GetPrice<'info> {
    pub swap_pool: Account<'info, SwapPool>,
    
    #[account(
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key()
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,
    
    #[account(
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key()
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,
    
    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref()
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct GetPoolStats<'info> {
    pub swap_pool: Account<'info, SwapPool>,

    #[account(
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key()
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key()
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        constraint = lp_mint.key() == swap_pool.lp_mint,
    )]
    pub lp_mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref()
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct GetUserShare<'info> {
    pub swap_pool: Account<'info, SwapPool>,

    #[account(
        constraint = token_a_vault.mint == swap_pool.token_a_mint,
        constraint = token_a_vault.owner == pool_authority.key()
    )]
    pub token_a_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        constraint = token_b_vault.mint == swap_pool.token_b_mint,
        constraint = token_b_vault.owner == pool_authority.key()
    )]
    pub token_b_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        constraint = lp_mint.key() == swap_pool.lp_mint
    )]
    pub lp_mint: InterfaceAccount<'info, Mint>,

    #[account(
        constraint = user_lp_token.mint == lp_mint.key(),
        constraint = user_lp_token.owner == user_authority.key()
    )]
    pub user_lp_token: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [
            b"pool_authority".as_ref(),
            swap_pool.token_a_mint.as_ref(),
            swap_pool.token_b_mint.as_ref()
        ],
        bump = swap_pool.bump
    )]
    /// CHECK: This is a PDA used as the authority
    pub pool_authority: UncheckedAccount<'info>,
    
    pub user_authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct SyncNative<'info> {
    #[account(mut)]
    pub swap_pool: Account<'info, SwapPool>,
    
    #[account(mut)]
    pub token_account: InterfaceAccount<'info, TokenAccount>,
    
    #[account(mut)]
    pub admin: Signer<'info>,
    
    pub token_program: Interface<'info, TokenInterface>,
}