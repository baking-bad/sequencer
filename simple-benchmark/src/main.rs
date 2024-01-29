use clap::{command, Args, Parser, Subcommand};
mod narwhal {
    tonic::include_proto!("narwhal");
}

mod exporter {
    tonic::include_proto!("exporter");
}

mod listener;
mod spammer;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Adds files to myapp
    Spammer(SpammerArgs),
    Listener(ListenerArgs),
}

/// Simple transactions generator that connects to a worker
/// and sends generated transactions with a given interval
#[derive(Args)]
struct SpammerArgs {
    /// Worker's gRPC endpoint to connect to
    #[arg(short, long, default_value_t=("http://127.0.0.1:64013".parse()).unwrap())]
    endpoint: String,
    /// Sleep duration, ms
    #[arg(short, long, default_value_t = 1000)]
    sleep: u64,
    /// Min transaction size, bytes
    #[arg(long, default_value_t = 1024)]
    min_size: u32,
    /// Max transaction size, bytes
    #[arg(long, default_value_t = 32768)]
    max_size: u32,
}

#[derive(Args)]
struct ListenerArgs {
    /// Primary's gRPC endpoint to connect to
    #[arg(short, long, default_value_t=("http://127.0.0.1:64011".parse()).unwrap())]
    endpoint: String,
    /// Subdag id from which to receive updates
    #[arg(short, long, default_value_t = 0)]
    from_id: u64,
    /// Path to csv file to store transaction stats
    #[arg(short, long)]
    tx_output: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Spammer(args) => spammer::run(args).await,
        Commands::Listener(args) => listener::run(args).await,
    }
}
