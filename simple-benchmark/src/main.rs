use clap::Parser;
use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;
use log::{error, info};
use std::time::{SystemTime, UNIX_EPOCH};

mod narwhal {
    tonic::include_proto!("narwhal");
}
use narwhal::transactions_client::TransactionsClient;
use narwhal::Transaction;

/// Simple transactions generator that connects to a worker
/// and sends generated transactions with a given interval
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
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

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    let mut rng = rand::thread_rng();
    loop {
        info!("Connecting to {}...", args.endpoint.clone());
        match connect(args.endpoint.clone()).await {
            Ok(mut client) => {
                let payload: Vec<u8> = (1..rng.gen_range(args.min_size..args.max_size))
                    .map(|_| rng.gen_range(0..255))
                    .collect();
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                let timestamp_bytes = timestamp.to_be_bytes().to_vec();
                let transaction = [timestamp_bytes, payload].concat();

                info!(
                    "Connected. Sending transaction (size {}, timestamp {} ms)",
                    transaction.len(),
                    timestamp,
                );
                match client.submit_transaction(Transaction { transaction }).await {
                    Ok(_) => info!("Done. Sleep for {}ms", args.sleep),
                    Err(e) => error!("Failed to send transaction: {}", e,),
                }
            }
            Err(e) => {
                error!("Failed to connect: {}", e);
            }
        }
        sleep(Duration::from_millis(args.sleep)).await;
    }
}

async fn connect(endpoint: String) -> Result<TransactionsClient<Channel>, tonic::transport::Error> {
    TransactionsClient::connect(endpoint).await
}
