use anchor_lang::prelude::*;

#[error_code]
pub enum CustomError {
    #[msg("Invalid token provided")]
    InvalidToken,
    #[msg("Invalid amount provided")]
    InvalidAmount,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("Invalid swap pool")]
    InvalidSwapPool,
    #[msg("Unauthorized access")]
    UnauthorizedAccess,
    #[msg("Slippage exceeded")]
    SlippageExceeded,
    #[msg("Fee too high")]
    FeeTooHigh,
    #[msg("Pool paused")]
    PoolPaused,
    #[msg("Insufficient liquidity")]
    InsufficientLiquidity,
    #[msg("Unauthorized")]
    Unauthorized,
}