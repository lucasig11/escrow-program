use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Mint, SetAuthority, TokenAccount, Transfer};
use spl_token::instruction::AuthorityType;

declare_id!("1GdLS7WG2NsZX2Ba7yKCGMMb6mR9NDdrbQZtfLjm9C2");

#[program]
pub mod escrow_program {
    use super::*;

    const ESCROW_PDA_SEED: &[u8] = b"escrow-pda-seed";

    pub fn initialize_escrow(
        ctx: Context<InitializeEscrow>,
        _vault_account_bump: u8,
        initializer_amount: u64,
        taker_amount: u64,
    ) -> ProgramResult {
        let escrow_account = &mut ctx.accounts.escrow_account;
        escrow_account.initializer_key = *ctx.accounts.initializer.key;
        escrow_account.initializer_amount = initializer_amount;
        escrow_account.taker_amount = taker_amount;

        escrow_account.initializer_deposit_token_account = *ctx
            .accounts
            .initializer_deposit_token_account
            .to_account_info()
            .key;

        escrow_account.initializer_receive_token_account = *ctx
            .accounts
            .initializer_receive_token_account
            .to_account_info()
            .key;

        // Generate vault authority PDA
        let (vault_authority, _) = Pubkey::find_program_address(&[ESCROW_PDA_SEED], ctx.program_id);

        token::set_authority(
            ctx.accounts.as_set_authority_context(),
            AuthorityType::AccountOwner,
            Some(vault_authority),
        )?;

        token::transfer(
            ctx.accounts.as_transfer_to_pda_context(),
            initializer_amount,
        )
    }

    pub fn cancel_escrow(ctx: Context<CancelEscrow>) -> ProgramResult {
        let (_, vault_authority_bump) =
            Pubkey::find_program_address(&[ESCROW_PDA_SEED], ctx.program_id);
        let authority_seeds = &[ESCROW_PDA_SEED, &[vault_authority_bump]];

        // transfer back from vault to initializer
        token::transfer(
            ctx.accounts
                .as_transfer_to_initializer_context()
                .with_signer(&[authority_seeds]),
            ctx.accounts.escrow_account.initializer_amount,
        )?;

        // close vault and escrow_account
        token::close_account(
            ctx.accounts
                .as_close_context()
                .with_signer(&[authority_seeds]),
        )
    }

    pub fn exchange(ctx: Context<Exchange>) -> ProgramResult {
        let (_, vault_authority_bump) =
            Pubkey::find_program_address(&[ESCROW_PDA_SEED], ctx.program_id);
        let authority_seeds = &[ESCROW_PDA_SEED, &[vault_authority_bump]];

        // transfer from taker to initializer
        token::transfer(
            ctx.accounts.as_transfer_to_initializer_context(),
            ctx.accounts.escrow_account.taker_amount,
        )?;

        // transfer from initializer to taker
        token::transfer(
            ctx.accounts
                .as_transfer_to_taker_context()
                .with_signer(&[authority_seeds]),
            ctx.accounts.escrow_account.initializer_amount,
        )?;

        token::close_account(
            ctx.accounts
                .as_close_context()
                .with_signer(&[authority_seeds]),
        )
    }
}

#[derive(Accounts)]
#[instruction(vault_account_bump: u8, initializer_amount: u64)]
pub struct InitializeEscrow<'info> {
    #[account(mut, signer)]
    pub initializer: AccountInfo<'info>,
    pub mint: Account<'info, Mint>,
    #[account(
        init,
        seeds = [b"token-seed".as_ref()],
        bump = vault_account_bump,
        payer = initializer,
        token::mint = mint,
        token::authority = initializer,
    )]
    pub vault_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint =
            initializer_deposit_token_account.amount >= initializer_amount,
    )]
    pub initializer_deposit_token_account: Account<'info, TokenAccount>,
    pub initializer_receive_token_account: Account<'info, TokenAccount>,
    #[account(zero)]
    pub escrow_account: ProgramAccount<'info, EscrowAccount>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
    pub token_program: AccountInfo<'info>,
}

impl<'info> InitializeEscrow<'info> {
    fn as_transfer_to_pda_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.initializer_deposit_token_account.to_account_info(),
            to: self.vault_account.to_account_info(),
            authority: self.initializer.clone(),
        };

        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn as_set_authority_context(&self) -> CpiContext<'_, '_, '_, 'info, SetAuthority<'info>> {
        let cpi_accounts = SetAuthority {
            account_or_mint: self.vault_account.to_account_info(),
            current_authority: self.initializer.clone(),
        };
        let cpi_program = self.token_program.to_account_info();

        CpiContext::new(cpi_program, cpi_accounts)
    }
}

#[derive(Accounts)]
pub struct CancelEscrow<'info> {
    #[account(mut, signer)]
    pub initializer: AccountInfo<'info>,
    #[account(mut)]
    pub initializer_deposit_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub vault_account: Account<'info, TokenAccount>,
    pub vault_authority: AccountInfo<'info>,
    #[account(
        mut,
        constraint =
            escrow_account.initializer_key == *initializer.key,
        constraint =
            escrow_account
            .initializer_deposit_token_account == *initializer_deposit_token_account
            .to_account_info().key,
        close = initializer,
    )]
    pub escrow_account: ProgramAccount<'info, EscrowAccount>,
    pub token_program: AccountInfo<'info>,
}

impl<'info> CancelEscrow<'info> {
    fn as_transfer_to_initializer_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.vault_account.to_account_info(),
            to: self.initializer_deposit_token_account.to_account_info(),
            authority: self.vault_authority.clone(),
        };

        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn as_close_context(&self) -> CpiContext<'_, '_, '_, 'info, CloseAccount<'info>> {
        let cpi_accounts = CloseAccount {
            account: self.vault_account.to_account_info(),
            destination: self.initializer.clone(),
            authority: self.vault_authority.clone(),
        };
        let cpi_program = self.token_program.to_account_info();

        CpiContext::new(cpi_program, cpi_accounts)
    }
}

#[derive(Accounts)]
pub struct Exchange<'info> {
    #[account(signer)]
    pub taker: AccountInfo<'info>,
    #[account(mut)]
    pub taker_deposit_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub taker_receive_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub initializer_deposit_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub initializer_receive_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub initializer: AccountInfo<'info>,
    #[account(
        mut,
        constraint =
            escrow_account.taker_amount <= taker_deposit_token_account.amount,
        constraint =
            escrow_account
            .initializer_deposit_token_account == *initializer_deposit_token_account
            .to_account_info()
            .key,
        constraint =
            escrow_account
            .initializer_receive_token_account == *initializer_receive_token_account
            .to_account_info()
            .key,
        constraint =
            escrow_account.initializer_key == *initializer.key,
        close = initializer,
    )]
    pub escrow_account: ProgramAccount<'info, EscrowAccount>,
    #[account(mut)]
    pub vault_account: Account<'info, TokenAccount>,
    pub vault_authority: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
}

impl<'info> Exchange<'info> {
    fn as_transfer_to_initializer_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.taker_deposit_token_account.to_account_info(),
            to: self.initializer_deposit_token_account.to_account_info(),
            authority: self.taker.clone(),
        };

        CpiContext::new(self.token_program.to_account_info(), cpi_accounts)
    }

    fn as_transfer_to_taker_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.vault_account.to_account_info(),
            to: self.taker_receive_token_account.to_account_info(),
            authority: self.vault_authority.clone(),
        };

        CpiContext::new(self.token_program.clone(), cpi_accounts)
    }

    fn as_close_context(&self) -> CpiContext<'_, '_, '_, 'info, CloseAccount<'info>> {
        let cpi_accounts = CloseAccount {
            account: self.vault_account.to_account_info(),
            destination: self.initializer.clone(),
            authority: self.vault_authority.clone(),
        };
        let cpi_program = self.token_program.to_account_info();

        CpiContext::new(cpi_program, cpi_accounts)
    }
}

#[account]
pub struct EscrowAccount {
    /// Key to authorize actions properly.
    pub initializer_key: Pubkey,
    /// Initializer's deposit account.
    pub initializer_deposit_token_account: Pubkey,
    /// Initializer's receive account.
    pub initializer_receive_token_account: Pubkey,
    /// How many tokens the initializer should send to taker.
    pub initializer_amount: u64,
    /// How many tokens the initializer should receive from the taker.
    pub taker_amount: u64,
}
