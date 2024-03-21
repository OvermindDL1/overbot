use crate::ShutdownTrigger;
use anyhow::Context;
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
	cache: InMemoryCache,
	exit_tx: ShutdownTrigger,
}

impl Shards {
	#[instrument(skip(token, exit_tx))]
	pub async fn new(token: String, exit_tx: ShutdownTrigger) -> anyhow::Result<Shards> {
		let intents = Intents::all(); // Let's request the all permissions!
		let client = twilight_http::Client::new(token.clone());
		let config = twilight_gateway::Config::new(token, intents);
		let cache = DefaultInMemoryCache::builder()
			.resource_types(twilight_cache_inmemory::ResourceType::MESSAGE)
			.build();

		Ok(Shards {
			client,
			config,
			cache,
			exit_tx,
		})
	}

	#[instrument(skip(self))]
	pub async fn run_server(&mut self) -> anyhow::Result<()> {
		let per_shard_config = |_id, builder: ConfigBuilder| builder.build();
		let mut exit_rx = self.exit_tx.receiver();
		let (shard_rx, shard_tx) = tokio::sync::broadcast::channel(1);
		// TODO: Not supporting resharding right now, probably not necessary anyway anymore, confirm please
		let mut timeout = tokio::time::sleep(std::time::Duration::from_secs(60 * 60 * 24 * 10000));
		loop {
			let mut set = JoinSet::new();
			for mut shard in
				twilight_gateway::create_recommended(&self.client, self.config.clone(), per_shard_config).await?
			{
				set.spawn(Self::handle_shard(shard, shard_rx.subscribe()));
			}
		}
	}

	#[instrument(skip(shard, receiver))]
	pub async fn handle_shard(mut shard: Shard, mut receiver: Receiver<ShardMessage>) -> anyhow::Result<()> {
		let shard_id = shard.id();
		loop {
			tokio::select! {
				msg = receiver.recv() => {
					match msg? {
						ShardMessage::Close(close_frame) => {
							info!("Shutting down Discord connection");
							shard.close(CloseFrame::NORMAL);
							// Leave running to process messages until the close message is received from the shard
						}
					}
				}
				event = shard.next_event(EventTypeFlags::all()) => {
					if let Some(event) = event {
						let event = event.context("error receiving discord event")?;
						debug!(?event, shard = ?shard.id(), "received event");
						tokio::spawn(Self::handle_event(shard_id, event));
					} else {
						info!(shard = ?shard.id(), "shard closed");
						break Ok(());
					}
				}
			}
		}
	}

	pub async fn handle_event(shard_id: ShardId, event: Event) {
		dbg!(event);
	}
}

#[derive(Clone, Debug)]
pub enum ShardMessage {
	Close(CloseFrame<'static>),
}
