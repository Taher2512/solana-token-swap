use anchor_lang::prelude::*;

use anchor_spl::token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked};
use crate::error::CustomError;

pub mod error;

declare_id!("AxqzHPnPm5Es17u3PuNHTvU2ivgYvZbzFgEgPiaH7Vj8");

#[program]
pub mod token_swap {
    use super::*;


}