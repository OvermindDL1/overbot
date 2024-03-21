use anyhow::Context;
use clap::Parser;
use tokio::sync::broadcast::error::SendError;

mod discord;
pub mod logging;
pub mod web_server;

#[derive(Parser, Debug)]
#[command(author, version)]
#[command(about = "Overbot is a general Discord/Discourse/Etc bot for the Gregtech/Mechaenetia areas")]
struct Args {
	#[command(flatten)]
	log_args: logging::LogArgs,
	#[command(flatten)]
	discord_args: discord::DiscordArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args = Args::parse();
	args.log_args.init_logger()?;

	let (exit_tx, mut exit_rx) = tokio::sync::broadcast::channel::<()>(1);
	let exit_tx = ShutdownTrigger(exit_tx);

	let web_exit_tx = exit_tx.clone();
	let mut web_server = tokio::spawn(async move {
		web_server::launch_server(web_exit_tx.receiver(), web_exit_tx)
			.await
			.context("web server error")
	});

	let discord_exit_tx = exit_tx.clone();
	let mut discord_server = tokio::spawn(async move {
		args.discord_args
			.discord_connection( discord_exit_tx)
			.await
			.context("discord server error")
	});

	// We don't care about waiting on this to die, it's a background task
	let sig_exit_tx = exit_tx.clone();
	let _signal_handler = tokio::spawn(async move {
		tokio::signal::ctrl_c().await.expect("ctrl-c signal error");
		sig_exit_tx.shutdown().expect("shutdown trigger had an error");
	});

	// If any of these die, shutdown.
	tokio::select! {
		_ = exit_rx.recv() => {
			// Normal shutdown requested, wait on everything to shut down
			Ok(())
		}
		result = &mut web_server => {
			// Web server died, shut down
			exit_tx.shutdown().expect("shutdown trigger had an error");
			result.context("join web server error").and_then(|r| r.context("web server error"))
		}
		result = &mut discord_server => {
			// Discord server died, shut down
			exit_tx.shutdown().expect("shutdown trigger had an error");
			result.context("join discord server error").and_then(|r| r.context("discord server error"))
		}
	}?;

	Ok(())
}

#[derive(Clone)]
pub struct ShutdownTrigger(tokio::sync::broadcast::Sender<()>);

impl ShutdownTrigger {
	pub fn shutdown(&self) -> Result<usize, SendError<()>> {
		self.0.send(())
	}

	pub fn receiver(&self) -> tokio::sync::broadcast::Receiver<()> {
		self.0.subscribe()
	}
}
