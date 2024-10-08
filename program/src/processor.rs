use {
	crate::{
		error::TokenizerError,
		get_principal_mint_address, get_tokenizer_address, get_yield_mint_address,
		instruction::TokenizerInstruction,
		state::{TokenizerState, STATE_SIZE},
		Expiry,
	},
	borsh::{BorshDeserialize, BorshSerialize},
	solana_program::{
		account_info::{next_account_info, AccountInfo},
		clock,
		entrypoint::ProgramResult,
		msg,
		program::{invoke, invoke_signed},
		program_error::ProgramError,
		program_pack::Pack,
		pubkey::Pubkey,
		system_instruction, system_program,
		sysvar::{clock::Clock, rent, Sysvar},
	},
};

const MINT_SIZE: usize = 82;

pub enum RedemptionMode {
	Mature,
	PrincipalYield,
}

pub struct TokenizerProcessor;

impl TokenizerProcessor {
	pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
		if program_id != &crate::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		let instruction: TokenizerInstruction =
			TokenizerInstruction::try_from_slice(data)
				.map_err(|_| ProgramError::InvalidInstructionData)?;

		match instruction {
			TokenizerInstruction::InitializeTokenizer {
				underlying_mint,
				principal_token_mint,
				yield_token_mint,
				expiry,
				fixed_apy,
			} => Self::process_initialize_lysergic_tokenizer(
				accounts,
				underlying_mint,
				principal_token_mint,
				yield_token_mint,
				&expiry,
				fixed_apy,
			),
			TokenizerInstruction::InitializeMints {
				underlying_mint,
				expiry,
			} => Self::process_initialize_mints(accounts, underlying_mint, &expiry),
			TokenizerInstruction::InitializeTokenizerAndMints {
				underlying_mint,
				principal_token_mint,
				yield_token_mint,
				expiry,
				fixed_apy,
			} => Self::process_initialize_tokenizer_and_mints(
				accounts,
				underlying_mint,
				principal_token_mint,
				yield_token_mint,
				expiry,
				fixed_apy,
			),
			TokenizerInstruction::DepositUnderlying { amount } => {
				Self::process_deposit_underlying(accounts, amount)
			}
			TokenizerInstruction::TokenizePrincipal { amount } => {
				Self::process_tokenize_principal(accounts, amount)
			}
			TokenizerInstruction::TokenizeYield { amount } => {
				Self::process_tokenize_yield(accounts, amount)
			}
			TokenizerInstruction::DepositAndTokenize { amount } => {
				Self::process_deposit_and_tokenize(accounts, amount)
			}
			TokenizerInstruction::RedeemPrincipalAndYield { amount } => {
				Self::process_redeem_principal_and_yield(accounts, amount)
			}
			TokenizerInstruction::RedeemMaturePrincipal { principal_amount } => {
				Self::process_redeem_mature_principal(accounts, principal_amount)
			}
			TokenizerInstruction::ClaimYield { yield_amount } => {
				Self::process_claim_yield(accounts, yield_amount)
			}
			TokenizerInstruction::Terminate => Self::process_terminate(accounts),
			TokenizerInstruction::TerminateTokenizer => {
				Self::process_terminate_lysergic_tokenizer(accounts)
			}
			TokenizerInstruction::TerminateMints => Self::process_terminate_mints(accounts),
		}
	}

	fn process_initialize_lysergic_tokenizer(
		accounts: &[AccountInfo],
		underlying_mint: Pubkey,
		principal_token_mint: Pubkey,
		yield_token_mint: Pubkey,
		expiry: &Expiry,
		fixed_apy: u64,
	) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let authority = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let underlying_mint_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;
		let system_program = next_account_info(account_info_iter)?;
		let atoken_program = next_account_info(account_info_iter)?;

		let rent = rent::Rent::get()?;
		let timestamp = Clock::get()?.unix_timestamp;

		let expiry_date = expiry
			.to_expiry_date(timestamp)
			.expect("Invalid expiry date");

		let (tokenizer_key, bump) =
			get_tokenizer_address(&underlying_mint_account.key, expiry_date);
		msg!("Tokenizer key: {:?}", tokenizer_key);
		let (principal_mint, _) = get_principal_mint_address(&tokenizer_key);
		let (yield_mint, _) = get_yield_mint_address(&tokenizer_key);

		// Check if lysergic tokenizer account address is correct
		if lysergic_tokenizer_account.key != &tokenizer_key {
			return Err(TokenizerError::IncorrectTokenizerAddress.into());
		}

		if !authority.is_signer {
			return Err(ProgramError::MissingRequiredSignature);
		}

		// Check if the underlying vault account address is correct
		if underlying_vault_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				&lysergic_tokenizer_account.key,
				&underlying_mint,
			) {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		// Check the underlying mint account
		if &underlying_mint != underlying_mint_account.key {
			return Err(TokenizerError::IncorrectUnderlyingMintAddress.into());
		}

		// Check principal token mint address
		if &principal_token_mint != &principal_mint {
			return Err(TokenizerError::IncorrectPrincipalMintAddress.into());
		}

		// Check yield token mint address
		if &yield_token_mint != &yield_mint {
			return Err(TokenizerError::IncorrectYieldMintAddress.into());
		}

		// Check token program
		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		if atoken_program.key != &spl_associated_token_account::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		// Check system program
		if system_program.key != &system_program::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		// Check if the lysergic tokenizer account is already initialized
		if lysergic_tokenizer_account.owner != &crate::id() {
			let size = STATE_SIZE;
			let required_lamports = rent
				.minimum_balance(size)
				.max(1)
				.saturating_sub(lysergic_tokenizer_account.lamports());

			msg!("Creating lysergic tokenizer account");
			// Create lysergic tokenizer account
			invoke_signed(
				&system_instruction::create_account(
					authority.key,
					&tokenizer_key,
					required_lamports,
					size as u64,
					&crate::id(),
				),
				&[
					authority.clone(),
					lysergic_tokenizer_account.clone(),
					system_program.clone(),
				],
				&[&[
					b"tokenizer",
					&underlying_mint_account.key.to_bytes()[..],
					&expiry_date.to_le_bytes(),
					&[bump],
				]],
			)?;

			msg!("Creating underlying vault account");
			// Create underlying vault account
			invoke_signed(
				&spl_associated_token_account::instruction::create_associated_token_account(
					authority.key,
					lysergic_tokenizer_account.key,
					&underlying_mint,
					token_program.key,
				),
				&[
					authority.clone(),
					underlying_vault_account.clone(),
					lysergic_tokenizer_account.clone(),
					underlying_mint_account.clone(),
					system_program.clone(),
					token_program.clone(),
					atoken_program.clone(),
				],
				&[&[
					b"tokenizer",
					&underlying_mint_account.key.to_bytes()[..],
					&expiry_date.to_le_bytes(),
					&[bump],
				]],
			)?;

			let lysergic_tokenizer_state = TokenizerState {
				bump,
				authority: *authority.key,
				principal_token_mint,
				yield_token_mint,
				underlying_mint,
				underlying_vault: *underlying_vault_account.key,
				expiry_date,
				fixed_apy,
			};

			lysergic_tokenizer_state
				.serialize(&mut &mut lysergic_tokenizer_account.data.borrow_mut()[..])?;
			msg!("Lysergic tokenizer account created");

			Ok(())
		} else {
			return Err(TokenizerError::TokenizerAlreadyInitialized.into());
		}
	}

	fn process_initialize_mints(
		accounts: &[AccountInfo],
		underlying_mint: Pubkey,
		expiry: &Expiry,
	) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let authority = next_account_info(account_info_iter)?;
		let underlying_mint_account = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;
		let system_program = next_account_info(account_info_iter)?;

		let rent = rent::Rent::get()?;
		let timestamp = Clock::get()?.unix_timestamp;

		let expiry_date = match expiry.to_expiry_date(timestamp) {
			Some(expiry_date) => expiry_date,
			None => return Err(TokenizerError::InvalidExpiryDate.into()),
		};

		let (tokenizer_key, bump) = get_tokenizer_address(&underlying_mint, expiry_date);
		let (principal_mint, pbump) = get_principal_mint_address(&tokenizer_key);
		let (yield_mint, ybump) = get_yield_mint_address(&tokenizer_key);

		// General safety checks
		if lysergic_tokenizer_account.key != &tokenizer_key {
			return Err(TokenizerError::IncorrectTokenizerAddress.into());
		}

		if !authority.is_signer {
			return Err(ProgramError::MissingRequiredSignature);
		}
		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		// Run different safety checks if the lysergic tokenizer account is initialized or
		// unintialized
		if lysergic_tokenizer_account.owner == &crate::id() {
			let lysergic_tokenizer_state = match TokenizerState::try_from_slice(
				&lysergic_tokenizer_account.data.borrow(),
			) {
				Ok(data) => data,
				Err(_) => return Err(ProgramError::InvalidAccountData),
			};

			if &lysergic_tokenizer_state.principal_token_mint != principal_token_mint_account.key {
				return Err(TokenizerError::IncorrectPrincipalMintAddress.into());
			}

			if &lysergic_tokenizer_state.yield_token_mint != yield_token_mint_account.key {
				return Err(TokenizerError::IncorrectYieldMintAddress.into());
			}

			if &lysergic_tokenizer_state.underlying_mint != underlying_mint_account.key {
				return Err(TokenizerError::IncorrectUnderlyingMintAddress.into());
			}

			if lysergic_tokenizer_state.expiry_date != expiry_date {
				return Err(TokenizerError::InvalidExpiryDate.into());
			}

			if lysergic_tokenizer_state.underlying_vault
				!= spl_associated_token_account::get_associated_token_address(
					lysergic_tokenizer_account.key,
					&lysergic_tokenizer_state.underlying_mint,
				) {
				return Err(TokenizerError::IncorrectVaultAddress.into());
			}
		} else if lysergic_tokenizer_account.owner != &crate::id() {
			if principal_token_mint_account.key != &principal_mint {
				return Err(TokenizerError::IncorrectPrincipalMintAddress.into());
			}
			if yield_token_mint_account.key != &yield_mint {
				return Err(TokenizerError::IncorrectYieldMintAddress.into());
			}
		}

		let required_lamports_principal = rent
			.minimum_balance(MINT_SIZE)
			.max(1)
			.saturating_sub(principal_token_mint_account.lamports());
		let required_lamports_yield = rent
			.minimum_balance(MINT_SIZE)
			.max(1)
			.saturating_sub(yield_token_mint_account.lamports());

		msg!("Creating principal mint account");
		invoke_signed(
			&system_instruction::create_account(
				authority.key,
				&principal_token_mint_account.key,
				required_lamports_principal,
				MINT_SIZE as u64,
				&spl_token::id(),
			),
			&[
				authority.clone(),
				principal_token_mint_account.clone(),
				system_program.clone(),
			],
			&[&[
				b"principal",
				&lysergic_tokenizer_account.key.to_bytes()[..],
				&[pbump],
			]],
		)?;

		msg!("Creating yield mint account");
		invoke_signed(
			&system_instruction::create_account(
				authority.key,
				&yield_token_mint_account.key,
				required_lamports_yield,
				MINT_SIZE as u64,
				&spl_token::id(),
			),
			&[
				authority.clone(),
				yield_token_mint_account.clone(),
				system_program.clone(),
			],
			&[&[
				b"yield",
				&lysergic_tokenizer_account.key.to_bytes()[..],
				&[ybump],
			]],
		)?;

		msg!("Initializing principal token mint");
		// Initialize principal token mint
		invoke_signed(
			&spl_token::instruction::initialize_mint2(
				token_program.key,
				principal_token_mint_account.key,
				lysergic_tokenizer_account.key,
				None,
				6,
			)?,
			&[principal_token_mint_account.clone(), token_program.clone()],
			&[&[
				b"tokenizer",
				&underlying_mint_account.key.to_bytes()[..],
				&expiry_date.to_le_bytes(),
				&[bump],
			]],
		)?;

		msg!("Initializing yield token mint");
		// Initialize yield token mint
		invoke_signed(
			&spl_token::instruction::initialize_mint2(
				token_program.key,
				yield_token_mint_account.key,
				lysergic_tokenizer_account.key,
				None,
				6,
			)?,
			&[yield_token_mint_account.clone(), token_program.clone()],
			&[&[
				b"tokenizer",
				&underlying_mint_account.key.to_bytes()[..],
				&expiry_date.to_le_bytes(),
				&[bump],
			]],
		)?;

		Ok(())
	}

	fn process_initialize_tokenizer_and_mints(
		accounts: &[AccountInfo],
		underlying_mint: Pubkey,
		principal_token_mint: Pubkey,
		yield_token_mint: Pubkey,
		expiry: Expiry,
		fixed_apy: u64,
	) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let authority = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let underlying_mint_account = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;

		let token_program = next_account_info(account_info_iter)?;
		let system_program = next_account_info(account_info_iter)?;
		let atoken_program = next_account_info(account_info_iter)?;

		let initialize_tokenizer_accounts = [
			lysergic_tokenizer_account.clone(),
			authority.clone(),
			underlying_vault_account.clone(),
			underlying_mint_account.clone(),
			token_program.clone(),
			system_program.clone(),
			atoken_program.clone(),
		];

		let initialize_mint_accounts = [
			lysergic_tokenizer_account.clone(),
			authority.clone(),
			underlying_mint_account.clone(),
			principal_token_mint_account.clone(),
			yield_token_mint_account.clone(),
			token_program.clone(),
			system_program.clone(),
		];

		Self::process_initialize_lysergic_tokenizer(
			&initialize_tokenizer_accounts,
			underlying_mint,
			principal_token_mint,
			yield_token_mint,
			&expiry,
			fixed_apy,
		)?;

		Self::process_initialize_mints(&initialize_mint_accounts, underlying_mint, &expiry)?;

		Ok(())
	}

	fn process_deposit_underlying(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let user_account = next_account_info(account_info_iter)?;
		let user_underlying_token_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;

		let amount = spl_token::ui_amount_to_amount(amount as f64, 6);

		let lysergic_tokenizer_state = TokenizerState::try_from_slice(
			&lysergic_tokenizer_account.data.borrow()[..STATE_SIZE],
		)?;

		// Safety checks
		if lysergic_tokenizer_account.owner != &crate::id() {
			return Err(TokenizerError::TokenizerNotInitialized.into());
		}

		if underlying_vault_account.owner != &spl_token::id() {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		if underlying_vault_account.key != &lysergic_tokenizer_state.underlying_vault {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		if !user_account.is_signer {
			return Err(ProgramError::MissingRequiredSignature);
		}

		if user_underlying_token_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				user_account.key,
				&lysergic_tokenizer_state.underlying_mint,
			) {
			return Err(TokenizerError::InvalidUserAccount.into());
		}

		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		msg!("Depositing underlying...");
		// Transfer underlying token from user to lysergic tokenizer
		invoke(
			&spl_token::instruction::transfer(
				token_program.key,
				user_underlying_token_account.key,
				underlying_vault_account.key,
				user_account.key,
				&[],
				amount,
			)?,
			&[
				user_underlying_token_account.clone(),
				underlying_vault_account.clone(),
				user_account.clone(),
				token_program.clone(),
			],
		)?;

		Ok(())
	}

	fn process_tokenize_principal(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let user_account = next_account_info(account_info_iter)?;
		let user_principal_token_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;

		let amount = spl_token::ui_amount_to_amount(amount as f64, 6);

		if lysergic_tokenizer_account.owner != &crate::id() {
			return Err(TokenizerError::TokenizerNotInitialized.into());
		}

		let lysergic_tokenizer_state =
			TokenizerState::try_from_slice(&lysergic_tokenizer_account.data.borrow()[..])?;

		// Check to see if the expiry date has elapsed
		if lysergic_tokenizer_state.expiry_date < clock::Clock::get()?.unix_timestamp {
			return Err(TokenizerError::ExpiryDateElapsed.into());
		}

		if principal_token_mint_account.key != &lysergic_tokenizer_state.principal_token_mint {
			return Err(TokenizerError::IncorrectPrincipalMintAddress.into());
		}

		if user_principal_token_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				user_account.key,
				&lysergic_tokenizer_state.principal_token_mint,
			) {
			return Err(TokenizerError::InvalidUserAccount.into());
		}

		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		// We may want to create a principal token account for the user if it doesn't exist
		if user_principal_token_account.owner != token_program.key {
			msg!("No user principal account found, creating...");
			let system_program = next_account_info(account_info_iter)?;
			let atoken_program = next_account_info(account_info_iter)?;

			if system_program.key != &system_program::id() {
				return Err(ProgramError::IncorrectProgramId);
			}

			if atoken_program.key != &spl_associated_token_account::id() {
				return Err(ProgramError::IncorrectProgramId);
			}

			invoke(
				&spl_associated_token_account::instruction::create_associated_token_account(
					user_account.key,
					user_account.key,
					&lysergic_tokenizer_state.principal_token_mint,
					token_program.key,
				),
				&[
					user_account.clone(),
					user_principal_token_account.clone(),
					user_account.clone(),
					principal_token_mint_account.clone(),
					system_program.clone(),
					token_program.clone(),
					atoken_program.clone(),
				],
			)?;
		}

		msg!("Minting principal to user...");
		// Mint principal token to user
		invoke_signed(
			&spl_token::instruction::mint_to(
				token_program.key,
				principal_token_mint_account.key,
				user_principal_token_account.key,
				lysergic_tokenizer_account.key,
				&[],
				amount,
			)?,
			&[
				principal_token_mint_account.clone(),
				user_principal_token_account.clone(),
				lysergic_tokenizer_account.clone(),
				token_program.clone(),
			],
			&[&[
				b"tokenizer",
				&lysergic_tokenizer_state.underlying_mint.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		Ok(())
	}

	fn process_tokenize_yield(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;
		let user_account = next_account_info(account_info_iter)?;
		let user_yield_token_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;

		let amount = spl_token::ui_amount_to_amount(amount as f64, 6);

		if lysergic_tokenizer_account.owner != &crate::id() {
			return Err(TokenizerError::TokenizerNotInitialized.into());
		}

		let lysergic_tokenizer_state =
			TokenizerState::try_from_slice(&lysergic_tokenizer_account.data.borrow()[..])?;

		if lysergic_tokenizer_state.expiry_date < clock::Clock::get()?.unix_timestamp {
			return Err(TokenizerError::ExpiryDateElapsed.into());
		}

		if yield_token_mint_account.key != &lysergic_tokenizer_state.yield_token_mint {
			return Err(TokenizerError::IncorrectYieldMintAddress.into());
		}

		if user_yield_token_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				user_account.key,
				&lysergic_tokenizer_state.yield_token_mint,
			) {
			return Err(TokenizerError::InvalidUserAccount.into());
		}

		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		// We may want to create a yield token account for the user if it doesn't exist
		if user_yield_token_account.owner != token_program.key {
			msg!("No user yield account found, creating...");
			let system_program = next_account_info(account_info_iter)?;
			let atoken_program = next_account_info(account_info_iter)?;

			if system_program.key != &system_program::id() {
				return Err(ProgramError::IncorrectProgramId);
			}

			if atoken_program.key != &spl_associated_token_account::id() {
				return Err(ProgramError::IncorrectProgramId);
			}

			invoke(
				&spl_associated_token_account::instruction::create_associated_token_account(
					user_account.key,
					user_account.key,
					&lysergic_tokenizer_state.yield_token_mint,
					token_program.key,
				),
				&[
					user_account.clone(),
					user_yield_token_account.clone(),
					user_account.clone(),
					yield_token_mint_account.clone(),
					system_program.clone(),
					token_program.clone(),
					atoken_program.clone(),
				],
			)?;
		}

		msg!("Minting yield to user...");
		// Mint yield token to user
		invoke_signed(
			&spl_token::instruction::mint_to(
				token_program.key,
				yield_token_mint_account.key,
				user_yield_token_account.key,
				lysergic_tokenizer_account.key,
				&[],
				amount,
			)?,
			&[
				yield_token_mint_account.clone(),
				user_yield_token_account.clone(),
				lysergic_tokenizer_account.clone(),
				token_program.clone(),
			],
			&[&[
				b"tokenizer",
				&lysergic_tokenizer_state.underlying_mint.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		Ok(())
	}

	fn process_deposit_and_tokenize(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;
		let user_account = next_account_info(account_info_iter)?;
		let user_underlying_token_account = next_account_info(account_info_iter)?;
		let user_principal_token_account = next_account_info(account_info_iter)?;
		let user_yield_token_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;
		let system_program = next_account_info(account_info_iter)?;
		let atoken_program = next_account_info(account_info_iter)?;

		let deposit_accounts = [
			lysergic_tokenizer_account.clone(),
			underlying_vault_account.clone(),
			user_account.clone(),
			user_underlying_token_account.clone(),
			token_program.clone(),
		];

		let tokenize_principal_accounts = vec![
			lysergic_tokenizer_account.clone(),
			principal_token_mint_account.clone(),
			user_account.clone(),
			user_principal_token_account.clone(),
			token_program.clone(),
			system_program.clone(),
			atoken_program.clone(),
		];

		let tokenize_yield_accounts = vec![
			lysergic_tokenizer_account.clone(),
			yield_token_mint_account.clone(),
			user_account.clone(),
			user_yield_token_account.clone(),
			token_program.clone(),
			system_program.clone(),
			atoken_program.clone(),
		];

		Self::process_deposit_underlying(&deposit_accounts, amount)?;
		Self::process_tokenize_principal(&tokenize_principal_accounts, amount)?;
		Self::process_tokenize_yield(&tokenize_yield_accounts, amount)?;

		Ok(())
	}

	fn process_redeem_principal_and_yield(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
		msg!("Redeem principal and yield...");
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let underlying_mint_account = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;
		let user_account = next_account_info(account_info_iter)?;
		let user_underlying_token_account = next_account_info(account_info_iter)?;
		let user_principal_token_account = next_account_info(account_info_iter)?;
		let user_yield_token_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;

		let redeem_principal_accounts = [
			lysergic_tokenizer_account.clone(),
			underlying_vault_account.clone(),
			underlying_mint_account.clone(),
			principal_token_mint_account.clone(),
			user_account.clone(),
			user_underlying_token_account.clone(),
			user_principal_token_account.clone(),
			token_program.clone(),
		];

		let claim_yield_accounts = [
			lysergic_tokenizer_account.clone(),
			underlying_vault_account.clone(),
			underlying_mint_account.clone(),
			yield_token_mint_account.clone(),
			user_account.clone(),
			user_underlying_token_account.clone(),
			user_yield_token_account.clone(),
			token_program.clone(),
		];

		Self::process_redeem_principal(
			&redeem_principal_accounts,
			RedemptionMode::PrincipalYield,
			amount,
		)?;
		Self::process_claim_yield(&claim_yield_accounts, amount)?;

		Ok(())
	}

	fn process_redeem_mature_principal(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
		Self::process_redeem_principal(accounts, RedemptionMode::Mature, amount)
	}

	fn process_redeem_principal(
		accounts: &[AccountInfo],
		redemption_mode: RedemptionMode,
		amount: u64,
	) -> ProgramResult {
		msg!("Redeeming principal...");
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let underlying_mint_account = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let user_account = next_account_info(account_info_iter)?;
		let user_underlying_token_account = next_account_info(account_info_iter)?;
		let user_principal_token_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;

		let amount = spl_token::ui_amount_to_amount(amount as f64, 6);

		if lysergic_tokenizer_account.owner != &crate::id() {
			return Err(TokenizerError::TokenizerNotInitialized.into());
		}

		let lysergic_tokenizer_state =
			TokenizerState::try_from_slice(&lysergic_tokenizer_account.data.borrow()[..])?;

		if let RedemptionMode::Mature = redemption_mode {
			if lysergic_tokenizer_state.expiry_date >= clock::Clock::get()?.unix_timestamp {
				return Err(TokenizerError::ExpiryDateNotElapsed.into());
			}
		}

		if underlying_vault_account.owner != &spl_token::id() {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		if underlying_vault_account.key != &lysergic_tokenizer_state.underlying_vault {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		if underlying_mint_account.key != &lysergic_tokenizer_state.underlying_mint {
			return Err(TokenizerError::IncorrectUnderlyingMintAddress.into());
		}

		if principal_token_mint_account.key != &lysergic_tokenizer_state.principal_token_mint {
			return Err(TokenizerError::IncorrectPrincipalMintAddress.into());
		}

		if !user_account.is_signer {
			return Err(ProgramError::MissingRequiredSignature);
		}

		if user_underlying_token_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				user_account.key,
				&lysergic_tokenizer_state.underlying_mint,
			) {
			return Err(TokenizerError::InvalidUserAccount.into());
		}

		if user_principal_token_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				user_account.key,
				&lysergic_tokenizer_state.principal_token_mint,
			) {
			return Err(TokenizerError::InvalidUserAccount.into());
		}

		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		// Check if the user has enough principal tokens to redeem
		let user_principal_token_account_data = spl_token::state::Account::unpack_from_slice(
			&user_principal_token_account.data.borrow(),
		)?;

		if user_principal_token_account_data.amount < amount {
			return Err(TokenizerError::InsufficientFunds.into());
		}

		// In the rather unlikely event that a user does not have an underlying token account;
		// create one for them
		if user_underlying_token_account.owner != token_program.key {
			let system_program = next_account_info(account_info_iter)?;
			if system_program.key != &system_program::id() {
				return Err(ProgramError::IncorrectProgramId);
			}

			invoke(
				&spl_associated_token_account::instruction::create_associated_token_account(
					user_account.key,
					user_underlying_token_account.key,
					&lysergic_tokenizer_state.underlying_mint,
					user_account.key,
				),
				&[
					user_underlying_token_account.clone(),
					user_account.clone(),
					underlying_mint_account.clone(),
					token_program.clone(),
					system_program.clone(),
				],
			)?;
		}

		invoke(
			&spl_token::instruction::burn(
				token_program.key,
				user_principal_token_account.key,
				principal_token_mint_account.key,
				user_account.key,
				&[],
				amount,
			)?,
			&[
				user_principal_token_account.clone(),
				principal_token_mint_account.clone(),
				user_account.clone(),
				token_program.clone(),
			],
		)?;

		invoke_signed(
			&spl_token::instruction::transfer(
				token_program.key,
				underlying_vault_account.key,
				user_underlying_token_account.key,
				lysergic_tokenizer_account.key,
				&[],
				amount,
			)?,
			&[
				underlying_vault_account.clone(),
				user_underlying_token_account.clone(),
				lysergic_tokenizer_account.clone(),
			],
			&[&[
				b"tokenizer",
				&underlying_mint_account.key.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		Ok(())
	}

	fn process_claim_yield(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
		msg!("Claiming yield...");
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let underlying_mint_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;
		let user_account = next_account_info(account_info_iter)?;
		let user_underlying_token_account = next_account_info(account_info_iter)?;
		let user_yield_token_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;

		let amount = spl_token::ui_amount_to_amount(amount as f64, 6);

		if lysergic_tokenizer_account.owner != &crate::id() {
			return Err(TokenizerError::TokenizerNotInitialized.into());
		}

		let lysergic_tokenizer_state =
			TokenizerState::try_from_slice(&lysergic_tokenizer_account.data.borrow()[..])?;

		if underlying_vault_account.owner != &spl_token::id() {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		if underlying_vault_account.key != &lysergic_tokenizer_state.underlying_vault {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		if yield_token_mint_account.key != &lysergic_tokenizer_state.yield_token_mint {
			return Err(TokenizerError::IncorrectYieldMintAddress.into());
		}

		if !user_account.is_signer {
			return Err(ProgramError::MissingRequiredSignature);
		}

		if user_underlying_token_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				user_account.key,
				&lysergic_tokenizer_state.underlying_mint,
			) {
			return Err(TokenizerError::InvalidUserAccount.into());
		}

		if user_yield_token_account.key
			!= &spl_associated_token_account::get_associated_token_address(
				user_account.key,
				&lysergic_tokenizer_state.yield_token_mint,
			) {
			return Err(TokenizerError::InvalidUserAccount.into());
		}

		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		// Check if the user has enough yield tokens to redeem
		if spl_token::state::Account::unpack_from_slice(&user_yield_token_account.data.borrow())?
			.amount < amount
		{
			return Err(TokenizerError::InsufficientFunds.into());
		}

		// In the rather unlikely event that a user does not have an underlying token account;
		// create one for them
		if user_underlying_token_account.owner != token_program.key {
			let system_program = next_account_info(account_info_iter)?;

			if system_program.key != &system_program::id() {
				return Err(ProgramError::IncorrectProgramId);
			}

			invoke(
				&spl_associated_token_account::instruction::create_associated_token_account(
					user_account.key,
					user_underlying_token_account.key,
					&lysergic_tokenizer_state.underlying_mint,
					user_account.key,
				),
				&[
					user_underlying_token_account.clone(),
					user_account.clone(),
					underlying_mint_account.clone(),
					token_program.clone(),
					system_program.clone(),
				],
			)?;
		}

		invoke(
			&spl_token::instruction::burn(
				token_program.key,
				user_yield_token_account.key,
				yield_token_mint_account.key,
				user_account.key,
				&[],
				amount,
			)?,
			&[
				user_yield_token_account.clone(),
				yield_token_mint_account.clone(),
				user_account.clone(),
				token_program.clone(),
			],
		)?;

		invoke_signed(
			&spl_token::instruction::transfer(
				token_program.key,
				underlying_vault_account.key,
				user_underlying_token_account.key,
				lysergic_tokenizer_account.key,
				&[],
				amount,
			)?,
			&[
				underlying_vault_account.clone(),
				user_underlying_token_account.clone(),
				lysergic_tokenizer_account.clone(),
			],
			&[&[
				b"tokenizer",
				&underlying_mint_account.key.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		Ok(())
	}

	fn process_terminate(accounts: &[AccountInfo]) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let authority = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;
		let system_program = next_account_info(account_info_iter)?;

		let terminate_tokenizer_accounts = [
			lysergic_tokenizer_account.clone(),
			authority.clone(),
			underlying_vault_account.clone(),
			token_program.clone(),
			system_program.clone(),
		];

		let terminate_mint_accounts = [
			lysergic_tokenizer_account.clone(),
			authority.clone(),
			principal_token_mint_account.clone(),
			yield_token_mint_account.clone(),
			token_program.clone(),
			system_program.clone(),
		];

		Self::process_terminate_mints(&terminate_tokenizer_accounts)?;
		Self::process_terminate_lysergic_tokenizer(&terminate_mint_accounts)?;

		Ok(())
	}

	fn process_terminate_lysergic_tokenizer(accounts: &[AccountInfo]) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let authority = next_account_info(account_info_iter)?;
		let underlying_vault_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;
		let system_program = next_account_info(account_info_iter)?;

		if lysergic_tokenizer_account.owner != &crate::id() {
			return Err(TokenizerError::TokenizerNotInitialized.into());
		}

		if !authority.is_signer {
			return Err(ProgramError::MissingRequiredSignature);
		}

		let lysergic_tokenizer_state =
			TokenizerState::try_from_slice(&lysergic_tokenizer_account.data.borrow()[..])?;

		if authority.key != &lysergic_tokenizer_state.authority {
			return Err(TokenizerError::Unauthorised.into());
		}

		if lysergic_tokenizer_state.expiry_date >= clock::Clock::get()?.unix_timestamp {
			return Err(TokenizerError::ExpiryDateNotElapsed.into());
		}

		// Check vault is empty
		if spl_token::state::Account::unpack_from_slice(&underlying_vault_account.data.borrow())?
			.amount != 0
		{
			return Err(TokenizerError::VaultNotEmpty.into());
		}

		if underlying_vault_account.key != &lysergic_tokenizer_state.underlying_vault {
			return Err(TokenizerError::IncorrectVaultAddress.into());
		}

		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		if system_program.key != &system_program::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		invoke_signed(
			&system_instruction::transfer(
				lysergic_tokenizer_account.key,
				authority.key,
				match lysergic_tokenizer_account
					.lamports
					.as_ref()
					.try_borrow()
					.as_deref()
				{
					Ok(lamports) => **lamports,
					Err(_) => return Err(ProgramError::InvalidAccountData),
				},
			),
			&[
				lysergic_tokenizer_account.clone(),
				authority.clone(),
				system_program.clone(),
			],
			&[&[
				b"tokenizer",
				&lysergic_tokenizer_state.underlying_mint.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		// Terminate the Lysergic tokenizer account
		lysergic_tokenizer_account.assign(&system_program::id());
		lysergic_tokenizer_account.realloc(0, false)?;

		Ok(())
	}

	fn process_terminate_mints(accounts: &[AccountInfo]) -> ProgramResult {
		let account_info_iter = &mut accounts.iter();
		let lysergic_tokenizer_account = next_account_info(account_info_iter)?;
		let authority = next_account_info(account_info_iter)?;
		let principal_token_mint_account = next_account_info(account_info_iter)?;
		let yield_token_mint_account = next_account_info(account_info_iter)?;
		let token_program = next_account_info(account_info_iter)?;
		let system_program = next_account_info(account_info_iter)?;

		if lysergic_tokenizer_account.owner != &crate::id() {
			return Err(TokenizerError::TokenizerNotInitialized.into());
		}

		if !authority.is_signer {
			return Err(ProgramError::MissingRequiredSignature);
		}

		let lysergic_tokenizer_state =
			TokenizerState::try_from_slice(&lysergic_tokenizer_account.data.borrow()[..])?;

		if authority.key != &lysergic_tokenizer_state.authority {
			return Err(TokenizerError::Unauthorised.into());
		}

		if lysergic_tokenizer_state.expiry_date >= clock::Clock::get()?.unix_timestamp {
			return Err(TokenizerError::ExpiryDateNotElapsed.into());
		}

		if principal_token_mint_account.key != &lysergic_tokenizer_state.principal_token_mint {
			return Err(TokenizerError::IncorrectPrincipalMintAddress.into());
		}

		if yield_token_mint_account.key != &lysergic_tokenizer_state.yield_token_mint {
			return Err(TokenizerError::IncorrectYieldMintAddress.into());
		}

		if token_program.key != &spl_token::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		if system_program.key != &system_program::id() {
			return Err(ProgramError::IncorrectProgramId);
		}

		invoke_signed(
			&spl_token::instruction::close_account(
				token_program.key,
				principal_token_mint_account.key,
				authority.key,
				lysergic_tokenizer_account.key,
				&[],
			)?,
			&[
				principal_token_mint_account.clone(),
				authority.clone(),
				lysergic_tokenizer_account.clone(),
				token_program.clone(),
			],
			&[&[
				b"tokenizer",
				&lysergic_tokenizer_state.underlying_mint.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		invoke_signed(
			&spl_token::instruction::close_account(
				token_program.key,
				yield_token_mint_account.key,
				authority.key,
				lysergic_tokenizer_account.key,
				&[],
			)?,
			&[
				yield_token_mint_account.clone(),
				authority.clone(),
				lysergic_tokenizer_account.clone(),
				token_program.clone(),
			],
			&[&[
				b"tokenizer",
				&lysergic_tokenizer_state.underlying_mint.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		invoke_signed(
			&system_instruction::transfer(
				lysergic_tokenizer_account.key,
				authority.key,
				match lysergic_tokenizer_account
					.lamports
					.as_ref()
					.try_borrow()
					.as_deref()
				{
					Ok(lamports) => **lamports,
					Err(_) => return Err(ProgramError::InvalidAccountData),
				},
			),
			&[
				lysergic_tokenizer_account.clone(),
				authority.clone(),
				system_program.clone(),
			],
			&[&[
				b"tokenizer",
				&lysergic_tokenizer_state.underlying_mint.to_bytes()[..],
				&lysergic_tokenizer_state.expiry_date.to_le_bytes(),
				&[lysergic_tokenizer_state.bump],
			]],
		)?;

		Ok(())
	}
}
