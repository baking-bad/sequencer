use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{error, info};
use rand::Rng;
use tokio::time::sleep;
use tonic::transport::Channel;

use crate::narwhal::{transactions_client::TransactionsClient, Transaction};

use super::SpammerArgs;

pub async fn run(args: SpammerArgs) {
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
