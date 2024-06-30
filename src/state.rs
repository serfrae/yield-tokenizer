use {
	borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
	solana_program::pubkey::Pubkey,
};

pub const LYSERGIC_TOKENIZER_STATE_SIZE: usize = 180;

#[derive(BorshSerialize, BorshDeserialize, BorshSchema, Debug, PartialEq)]
pub struct LysergicTokenizerState {
	pub authority: Pubkey,
	pub principal_token_mint: Pubkey,
	pub yield_token_mint: Pubkey,
	pub underlying_mint: Pubkey,
	pub underlying_vault: Pubkey,
	pub expiry_date: i64,
	pub fixed_apy: u64,
}
