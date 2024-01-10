use clap::Parser;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;
use log::{error, info};
use std::time::{SystemTime, UNIX_EPOCH};
mod exporter {
    tonic::include_proto!("exporter");
}
use exporter::exporter_client::ExporterClient;
use exporter::*;

/// Simple consensus output listener that connects to a primary
/// and prints all received subdags
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// Primary's gRPC endpoint to connect to
    #[arg(short, long, default_value_t=("http://127.0.0.1:64011".parse()).unwrap())]
    endpoint: String,
    /// Subdag id from which to receive updates
    #[arg(short, long, default_value_t = 0)]
    from_id: u64,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Args::parse();
    loop {
        info!("Connecting to {}...", args.endpoint.clone());
        match connect(args.endpoint.clone()).await {
            Ok(client) => {
                info!("Connected. Exporting subdags from #{}...", args.from_id);
                match export(client, args.from_id).await {
                    Ok(_) => {
                        info!("Exit");
                        break;
                    }
                    Err(e) => {
                        error!("Failed to export: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to connect: {}", e);
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
}

async fn connect(endpoint: String) -> Result<ExporterClient<Channel>, tonic::transport::Error> {
    ExporterClient::connect(endpoint).await
}

async fn export(
    mut client: ExporterClient<Channel>,
    from_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = client.export(ExportRequest { from_id }).await?.into_inner();

    while let Some(subdag) = stream.message().await? {
        let stats = stats(&subdag);
        if stats.num_txs > 0 {
            info!(
                "Received subdag #{} (num txs {}, payload size {}, avg latency {} ms, cert time delta {} ms)",
                subdag.id,
                stats.num_txs,
                stats.payload_size,
                stats.avg_latency,
                stats.cert_time_delta,
            );
        } else {
            info!(
                "Empty subdag #{}",
                subdag.id,
            );
        }
    }

    info!("Close client");
    Ok(())
}

#[derive(Debug)]
pub struct Stats {
    pub subdag_time: u128,
    pub num_txs: usize,
    pub payload_size: usize,
    pub avg_latency: u128,
    pub cert_time_delta: u128,
}

fn stats(subdag: &SubDag) -> Stats {
    let subdag_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let mut num_txs = 0;
    let mut payload_size = 0;
    let mut sum_latency = 0;

    let first_cert_ts = subdag.certificates[0].clone().header.unwrap().created_at as u128;
    let last_cert_ts = subdag.leader.clone().unwrap().header.unwrap().created_at as u128;

    for payload in subdag.payloads.iter() {
        for batch in payload.batches.iter() {
            num_txs += batch.transactions.len();
            for tx in batch.transactions.iter() {
                payload_size += tx.len();
                // FIXME: handle parsing errors
                let tx_time_bytes: [u8; 16] = tx[..16].try_into().unwrap();
                let tx_time = u128::from_be_bytes(tx_time_bytes);
                sum_latency += subdag_time - tx_time;
            }
        }
    }

    Stats {
        subdag_time,
        num_txs,
        payload_size,
        avg_latency: if num_txs > 0 { sum_latency / (num_txs as u128) } else { 0 },
        cert_time_delta: last_cert_ts - first_cert_ts,
    }
}
