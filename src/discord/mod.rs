mod shards;

use crate::discord::shards::Shards;
use crate::ShutdownTrigger;
use anyhow::Context;
use clap::Parser;

use std::fmt::{Debug, Formatter};
use std::io::Read;
use tracing::{debug, error, info, instrument};

#[derive(Parser, Clone)]
pub struct DiscordArgs {
	/// Discord bot token
	#[arg(long, env = "DISCORD_TOKEN")]
	discord_token: Option<String>,
}

// We don't want to print the token anywhere, even by accident
impl Debug for DiscordArgs {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "DiscordArgs {{ discord_token: [REDACTED] }}")
	}
}

impl DiscordArgs {
	fn get_token(&self) -> anyhow::Result<String> {
		if let Some(token) = &self.discord_token {
			debug!("Discord token from command line");
			Ok(token.clone())
		} else if let Ok(mut token_file) = std::fs::File::open("../.discord.token") {
			let mut token = String::new();
			token_file.read_to_string(&mut token)?;
			debug!("Discord token from ../.discord.token file");
			Ok(token.trim().to_string())
		} else {
			// TODO: This is untested and doesn't seem to work on a get without a set first for some reason
			#[cfg(feature = "keyring")]
			{
				use cryptex::DynKeyRing;
				let mut keyring = cryptex::get_os_keyring("overbot")?;
				// keyring.set_secret("overbot-discord-token", b"test")?;
				if let Ok(token) = keyring
					.get_secret("overbot-discord-token")
					.map(|s| String::from_utf8_lossy(&s.0).to_string())
					.map_err(|e| Err::<String, _>(e).context("Failed to get Discord token from keyring"))
				{
					info!("Got Discord token from keyring");
					Ok(token)
				} else {
					error!("No Discord token provided via --discord-token or DISCORD_TOKEN environment variable or .discord.token file in the current working directory or keyring");
					anyhow::bail!("No Discord token provided via --discord-token or DISCORD_TOKEN environment variable or .discord.token file in the current working directory or keyring")
				}
			}
			#[cfg(not(feature = "keyring"))]
			{
				anyhow::bail!("No Discord token provided via --discord-token or DISCORD_TOKEN environment variable or .discord.token file in the current working directory")
			}
		}
	}

	#[instrument(skip(exit_tx))]
	pub async fn discord_connection(&self, exit_tx: ShutdownTrigger) -> anyhow::Result<()> {
		let token = self.get_token()?;
		let mut shards = Shards::new(token, exit_tx).await?;

		shards.run_server().await?;

		Ok(())
	}
}
