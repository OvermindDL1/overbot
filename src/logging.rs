use clap::{Parser, ValueEnum};
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::FmtSubscriber;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LogFormat {
	Compact,
	Pretty,
	JSON,
	// None,
}

#[derive(Parser, Clone, Debug)]
pub struct LogArgs {
	/// Verbosity logging level
	#[arg(short, long, default_value_t = Level::WARN)]
	pub verbosity: Level,

	/// Log format type on stderr.
	#[arg(value_enum, long, default_value_t = LogFormat::Compact)]
	pub log_format: LogFormat,
}

impl LogArgs {
	pub fn init_logger(&self) -> anyhow::Result<()> {
		let builder = FmtSubscriber::builder()
			.with_writer(std::io::stderr)
			.with_max_level(self.verbosity)
			.with_level(true)
			//.with_span_events(FmtSpan::FULL)
			.with_span_events(match self.verbosity {
				Level::ERROR => FmtSpan::NEW | FmtSpan::CLOSE,
				Level::WARN => FmtSpan::NEW | FmtSpan::CLOSE,
				Level::INFO => FmtSpan::NEW | FmtSpan::CLOSE,
				Level::DEBUG => FmtSpan::FULL,
				Level::TRACE => FmtSpan::FULL,
			})
			.with_target(true)
			.with_ansi(true);
		match self.log_format {
			LogFormat::Compact => builder.compact().finish().try_init()?,
			LogFormat::Pretty => builder.pretty().finish().try_init()?,
			LogFormat::JSON => builder.json().finish().try_init()?,
			// LogFormat::None => builder.finish().try_init()?,
		}
		tracing::info!("Initialized logging system at level: {}", self.verbosity);
		Ok(())
	}
}
