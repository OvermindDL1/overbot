use anyhow::Context;
use clap::Parser;
use tokio::sync::broadcast::error::SendError;
use tokio::task::JoinSet;
use tracing::info;

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

	let mut set = JoinSet::<anyhow::Result<&'static str>>::new();

	let web_exit_tx = exit_tx.clone();
	set.spawn(async move {
		web_server::launch_server(web_exit_tx.receiver(), web_exit_tx)
			.await
			.context("web server error")?;
		Ok("web server")
	});

	let discord_exit_tx = exit_tx.clone();
	set.spawn(async move {
		args.discord_args
			.discord_connection(discord_exit_tx)
			.await
			.context("discord server error")?;
		Ok("discord server")
	});

	let mut waiter_exit_rx = exit_tx.receiver();
	set.spawn(async move {
		waiter_exit_rx.recv().await.context("exit receiver join error")?;
		Ok("exit signal waiter")
	});

	let ctrlc_exit_tx = exit_tx.clone();
	set.spawn(async move {
		let mut ctrlc_exit_rx = ctrlc_exit_tx.receiver();
		tokio::select! {
			_ = ctrlc_exit_rx.recv() => {
				// Normal shutdown requested, wait on everything to shut down
				Ok("ctrl-c: exit signal")
			}
			_ = tokio::signal::ctrl_c() => {
				info!("ctrl-c signal received, sending shutdown signal");
				ctrlc_exit_tx.shutdown().context("shutdown trigger had an error")?;
				Ok("ctrl-c: ctrl-c signal")
			}
		}
	});

	let mut sent = false;
	while let Some(res) = set.join_next().await {
		info!(?res, "main task joined");
		if !sent && exit_rx.try_recv().is_err() {
			exit_tx.shutdown().expect("shutdown trigger had an error");
		}
		sent = true;
		if let Err(error) = res {
			info!(?error, "error in task");
		}
	}

	info!("shut down complete");

	Ok(())
}

#[derive(Clone)]
pub struct ShutdownTrigger(tokio::sync::broadcast::Sender<()>);

impl ShutdownTrigger {
	pub fn shutdown(&self) -> Result<usize, SendError<()>> {
		info!("sending shutdown signal");
		self.0.send(())
	}

	pub fn receiver(&self) -> tokio::sync::broadcast::Receiver<()> {
		self.0.subscribe()
	}
}
