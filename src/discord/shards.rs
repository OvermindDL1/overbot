use crate::ShutdownTrigger;
use anyhow::Context;
use std::sync::Arc;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinSet;
use tracing::{debug, error, info, instrument, warn};
use twilight_cache_inmemory::{DefaultInMemoryCache, InMemoryCache};
use twilight_gateway::{Config, ConfigBuilder, EventTypeFlags, Shard, StreamExt};
use twilight_http::Client;
use twilight_model::gateway::event::Event;
use twilight_model::gateway::{CloseFrame, Intents, ShardId};

pub struct Shards {
	client: Client,
	config: Config,
	cache: Arc<InMemoryCache>,
	exit_tx: ShutdownTrigger,
}

impl Shards {
	#[instrument(skip(token, exit_tx))]
	pub async fn new(token: String, exit_tx: ShutdownTrigger) -> anyhow::Result<Shards> {
		let intents = Intents::all(); // Let's request the all permissions!
		let client = Client::new(token.clone());
		let config = Config::new(token, intents);
		let cache = Arc::new(
			DefaultInMemoryCache::builder()
				// Yes the `message_cache_size` is per channel, not global
				.message_cache_size(128)
				.resource_types(twilight_cache_inmemory::ResourceType::MESSAGE)
				.build(),
		);

		Ok(Shards {
			client,
			config,
			cache,
			exit_tx,
		})
	}

	#[instrument(skip(self))]
	pub async fn run_server(&mut self) -> anyhow::Result<()> {
		use twilight_gateway::create_recommended;
		let per_shard_config = |_id, builder: ConfigBuilder| builder.build();
		let mut exit_rx = self.exit_tx.receiver();
		let (shard_tx, shard_rx) = tokio::sync::broadcast::channel(1);
		// TODO: Not supporting resharding right now, probably not necessary anyway anymore, confirm please
		// let mut timeout = tokio::time::sleep(std::time::Duration::from_secs(60 * 60 * 24 * 10000));
		// This is left as-is in case we want to support resharding sometime, so accept it:
		#[allow(clippy::never_loop)]
		loop {
			let mut set = JoinSet::new();
			for shard in create_recommended(&self.client, self.config.clone(), per_shard_config).await? {
				set.spawn(Self::handle_shard(shard, shard_tx.subscribe(), self.cache.clone()));
			}
			if let Err(error) = exit_rx.recv().await {
				error!(?error, "error receiving exit message");
			}
			info!("shutting down shards");
			shard_tx
				.send(ShardMessage::Close(CloseFrame::NORMAL))
				.context("error sending shard close message")?;
			drop(shard_rx);
			while let Some(res) = set.join_next().await {
				info!(?res, "shard task joined");
				if let Err(error) = res
					.context("join error")
					.and_then(|res| res.context("shard close error"))
				{
					warn!(?error, "error in shard task");
				}
			}
			info!("shard shut down");
			break Ok(());
		}
	}

	#[instrument(skip(shard, receiver))]
	pub async fn handle_shard(
		mut shard: Shard,
		mut receiver: Receiver<ShardMessage>,
		cache: Arc<InMemoryCache>,
	) -> anyhow::Result<()> {
		let shard_id = shard.id();
		loop {
			tokio::select! {
				msg = receiver.recv() => {
					match msg? {
						ShardMessage::Close(close_frame) => {
							info!("shutting down Discord connection");
							shard.close(close_frame);
							// Leave running to process messages until the close message is received from the shard
						}
					}
				}
				event = shard.next_event(EventTypeFlags::all()) => {
					if let Some(event) = event {
						let event = event.context("error receiving discord event")?;
						debug!(?event, shard = ?shard.id(), "received event");
						if let Event::GatewayClose(close_frame) = &event {
							info!(shard = ?shard.id(), ?close_frame, "shard closed");
							break Ok(())
						}
						cache.update(&event);
						tokio::spawn(Self::handle_event(shard_id, event, cache.clone()));
					} else {
						info!(shard = ?shard.id(), "shard closed");
						break Ok(())
					}
				}
			}
		}
	}

	#[instrument(skip(event))]
	pub async fn handle_event(shard_id: ShardId, event: Event, _cache: Arc<InMemoryCache>) {
		dbg!((shard_id, event.kind()));
		// dbg!(event);
	}
}

#[derive(Clone, Debug)]
pub enum ShardMessage {
	Close(CloseFrame<'static>),
}
