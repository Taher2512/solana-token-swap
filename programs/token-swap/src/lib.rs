use anchor_lang::prelude::*;

use anchor_spl::token_interface::{transfer_checked, mint_to, burn, Burn, Mint, TokenAccount, TokenInterface, TransferChecked, MintTo};
use crate::error::CustomError;

pub mod error;

declare_id!("AxqzHPnPm5Es17u3PuNHTvU2ivgYvZbzFgEgPiaH7Vj8");

#[program]
pub mod token_swap {
    use super::*;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        fee_rate: u64,
        bump: u8,
    ) -> Result<()> {
        require!(fee_rate <= 1000, CustomError::FeeTooHigh);

        let swap_pool = &mut ctx.accounts.swap_pool;
        swap_pool.token_a_mint = ctx.accounts.token_a_mint.key();
        swap_pool.token_b_mint = ctx.accounts.token_b_mint.key();
        swap_pool.token_a_vault = ctx.accounts.token_a_vault.key();
        swap_pool.token_b_vault = ctx.accounts.token_b_vault.key();
        swap_pool.lp_mint = ctx.accounts.lp_mint.key();
        swap_pool.pool_authority = ctx.accounts.pool_authority.key();
        swap_pool.fee_rate = fee_rate;
        swap_pool.bump = bump;
        swap_pool.is_paused = false;
        swap_pool.admin = ctx.accounts.admin.key();
        swap_pool.total_fees_a = 0;
        swap_pool.total_fees_b = 0;

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
        mint_to {
           mint_lp_ctx,
           initial_lp_tokens, 
        };
        
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
            let amount_a = (amount_b_desired as u128)
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
        mint_to {
            mint_lp_ctx,
            lp_to_mint,
        }?;

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
}



#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + SwapPool::INIT_SPACE,
    )]
    pub swap_pool: Account<'info, SwapPool>,

    pub token_a_mint: InterfaceAccount<'info, Mint>,
    pub token_b_mint: InterfaceAccount<'info, Mint>,

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
        seeds = [
            b"pool_authority".as_ref(),
            token_a_mint.key().as_ref(),
            token_b_mint.key().as_ref(),
        ],
        bump,
    )]
    /// CHECK: PDA that will have authority over the token vaults
    pub pool_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[account]
#[derive(InitSpace)]
pub struct SwapPool {
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub token_a_vault: Pubkey,
    pub token_b_vault: Pubkey,
    pub pool_authority: Pubkey,
    pub fee_rate: u64,
    pub bump: u8,
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

    pub user_authority: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>
}