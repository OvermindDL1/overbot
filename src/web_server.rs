use axum::extract::State;
use axum::routing::get;
use axum::Router;
use tokio::sync::broadcast::Receiver;
use tracing::instrument;

use crate::ShutdownTrigger;

#[instrument(skip(exit_rx, exit_tx))]
pub async fn launch_server(mut exit_rx: Receiver<()>, exit_tx: ShutdownTrigger) -> anyhow::Result<()> {
	let app = Router::new()
		.route("/", get(index))
		.route("/shutdown", get(shutdown))
		.with_state(exit_tx);

	let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;

	axum::serve(listener, app)
		.with_graceful_shutdown(async move {
			exit_rx.recv().await.expect("exit_rx shutdown improperly");
		})
		.await?;

	Ok(())
}

async fn index() -> &'static str {
	"Hello, world!"
}

async fn shutdown(exit_tx: State<ShutdownTrigger>) -> &'static str {
	exit_tx.shutdown().expect("shutdown trigger had an error");
	"Shutting down..."
}
