use clap::Parser;
use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;

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
    #[arg(short, long, default_value_t = 100)]
    sleep: u64,
    /// Sleep duration, ms
    #[arg(long, default_value_t = 10)]
    min_size: u32,
    /// Sleep duration, ms
    #[arg(long, default_value_t = 100)]
    max_size: u32,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut rng = rand::thread_rng();
    loop {
        println!("Connecting to {}...", args.endpoint.clone());
        match connect(args.endpoint.clone()).await {
            Ok(mut client) => {
                let transaction: Vec<u8> = (1..rng.gen_range(args.min_size..args.max_size))
                    .map(|_| rng.gen_range(0..255))
                    .collect();
                println!("Connected. Sending transaction {:?}", transaction);
                match client.submit_transaction(Transaction { transaction }).await {
                    Ok(_) => println!("Done. Sleep for {}ms", args.sleep),
                    Err(e) => println!("Failed to send transaction: {}", e),
                }
            }
            Err(e) => {
                println!("Failed to connect: {}", e);
            }
        }
        sleep(Duration::from_millis(args.sleep)).await;
    }
}

async fn connect(endpoint: String) -> Result<TransactionsClient<Channel>, tonic::transport::Error> {
    TransactionsClient::connect(endpoint).await
}
