use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{error, info};
use rand::Rng;
use tokio::{
    pin,
    sync::broadcast::Receiver,
    time::{sleep, Instant},
};
use tonic::transport::Channel;

use crate::narwhal::{transactions_client::TransactionsClient, Transaction};

use super::SpammerArgs;

fn gen_values(min_size: u32, max_size: u32) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    (1..rng.gen_range(min_size..max_size))
        .map(|_| rng.gen_range(0..255))
        .collect()
}

pub async fn run(args: SpammerArgs, mut rx_stop: Receiver<()>) {
    info!("Listener started");
    let time_to_next_tx = sleep(Duration::from_millis(args.sleep));
    pin!(time_to_next_tx);
    loop {
        tokio::select! {
        () = &mut time_to_next_tx => {
            info!("Connecting to {}...", args.endpoint);
            match connect(args.endpoint.clone()).await {
                Ok(mut client) => {
                    let payload: Vec<u8> = gen_values(args.min_size, args.max_size);
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
            time_to_next_tx.as_mut().reset(Instant::now() + Duration::from_millis(args.sleep));
        }
        Ok(()) = rx_stop.recv() => {
            info!("Timeout received: spammer will stop now!");
            break;
        }
        }
    }
}

async fn connect(endpoint: String) -> Result<TransactionsClient<Channel>, tonic::transport::Error> {
    TransactionsClient::connect(endpoint).await
}
