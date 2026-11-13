//! Key material and the addresses derived from it.
//!
//! Everything lives under `target/devnet/`, which is gitignored, so a rerun
//! works against the same accounts without ever holding a key in the repo.

use {
    anchor_lang::prelude::Pubkey,
    anchor_spl::associated_token,
    solana_keypair::{read_keypair_file, write_keypair_file, Keypair},
    solana_signer::Signer,
    std::{
        error::Error,
        fs,
        path::{Path, PathBuf},
    },
};

pub(crate) struct Keys {
    pub owner: Keypair,
    pub agent: Keypair,
    pub merchant: Keypair,
    pub payee: Keypair,
    pub mint: Keypair,
}

impl Keys {
    pub(crate) fn load() -> Result<Self, Box<dyn Error>> {
        let home = std::env::var("HOME")?;
        let owner = read_keypair_file(PathBuf::from(home).join(".config/solana/id.json"))?;
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/devnet");
        fs::create_dir_all(&dir)?;

        Ok(Self {
            owner,
            agent: keypair_at(&dir.join("agent.json"))?,
            merchant: keypair_at(&dir.join("merchant.json"))?,
            payee: keypair_at(&dir.join("payee.json"))?,
            mint: keypair_at(&dir.join("mint.json"))?,
        })
    }

    pub(crate) fn mandate_address(&self) -> Pubkey {
        mandate_address(&self.owner.pubkey(), &self.agent.pubkey())
    }

    pub(crate) fn owner_account(&self) -> Pubkey {
        associated_token_address(&self.owner.pubkey(), &self.mint.pubkey())
    }

    pub(crate) fn merchant_account(&self) -> Pubkey {
        associated_token_address(&self.merchant.pubkey(), &self.mint.pubkey())
    }

    pub(crate) fn payee_account(&self) -> Pubkey {
        associated_token_address(&self.payee.pubkey(), &self.mint.pubkey())
    }

    pub(crate) fn vault_account(&self, mandate: &Pubkey) -> Pubkey {
        associated_token_address(mandate, &self.mint.pubkey())
    }
}

/// Reuses a key on disk so a rerun targets the same accounts.
pub(crate) fn keypair_at(path: &Path) -> Result<Keypair, Box<dyn Error>> {
    if path.exists() {
        return read_keypair_file(path);
    }
    let keypair = Keypair::new();
    write_keypair_file(&keypair, path)?;
    Ok(keypair)
}

pub(crate) fn associated_token_address(authority: &Pubkey, mint: &Pubkey) -> Pubkey {
    associated_token::get_associated_token_address(authority, mint)
}

pub(crate) fn mandate_address(owner: &Pubkey, agent: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            mandate::constants::MANDATE_SEED,
            owner.as_ref(),
            agent.as_ref(),
        ],
        &mandate::id(),
    )
    .0
}
