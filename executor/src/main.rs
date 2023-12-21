use clap::Parser;
use std::net::SocketAddr;

mod server;
use server::run_executor_server;

/// Simple executor server, emulating EVM node
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// Socket address to bind
    #[arg(short, long, default_value_t=("127.0.0.1:54321".parse()).unwrap())]
    address: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    println!("Running executor server at {}", args.address);
    run_executor_server(args.address).await
}
