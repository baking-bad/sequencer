use clap::Parser;
use std::fs::File;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;
use log::{error, info, warn};
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
    /// Path to csv file to store transaction stats
    #[arg(short, long)]
    tx_output: Option<String>,
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
                match export(client, args.from_id, args.tx_output.clone()).await {
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
    tx_output: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = client.export(ExportRequest { from_id }).await?.into_inner();

    let mut tx_writer = match tx_output {
        Some(path) => Some(csv::Writer::from_path(path).unwrap()),
        None => None,
    };

    while let Some(subdag) = stream.message().await? {
        if let Some(ref mut writer) = tx_writer {
            write_tx_stats(&subdag, writer);
        }

        let stats = stats(&subdag);
        if stats.num_txs > 0 {
            info!(
                "Received subdag #{} (num txs {}, payload size {}, avg latency {} ms, cert delta {} ms / {} rounds)",
                subdag.id,
                stats.num_txs,
                stats.payload_size,
                stats.avg_latency,
                stats.cert_time_delta,
                stats.cert_round_delta,
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
    pub cert_round_delta: u64,
}

fn stats(subdag: &SubDag) -> Stats {
    let subdag_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let mut num_txs = 0;
    let mut payload_size = 0;
    let mut sum_latency = 0;

    let first_cert_ts = subdag.certificates[0].clone().header.unwrap().created_at as u128;
    let last_cert_ts = subdag.leader.clone().unwrap().header.unwrap().created_at as u128;

    let first_cert_round = subdag.certificates[0].clone().header.unwrap().round;
    let last_cert_round = subdag.leader.clone().unwrap().header.unwrap().round;

    for payload in subdag.payloads.iter() {
        for batch in payload.batches.iter() {
            num_txs += batch.transactions.len();
            for tx in batch.transactions.iter() {
                payload_size += tx.len();
                let tx_time_bytes: [u8; 16] = match tx.get(0..16) {
                    Some(value) => value.try_into().unwrap(),
                    None => {
                        warn!("Foreign transaction {}", hex::encode(tx));
                        continue
                    }
                };
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
        cert_round_delta: last_cert_round - first_cert_round,
    }
}

fn write_tx_stats(subdag: &SubDag, writer: &mut csv::Writer<File>) {
    let received_at = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let first_cert_round = subdag.certificates[0].clone().header.unwrap().round;
    let last_cert_round = subdag.leader.clone().unwrap().header.unwrap().round;
    let num_rounds = last_cert_round - first_cert_round + 1;
    let num_blocks = subdag.payloads.len();

    for payload in subdag.payloads.iter() {
        for batch in payload.batches.iter() {
            for tx in batch.transactions.iter() {
                let tx_time_bytes: [u8; 16] = match tx.get(0..16) {
                    Some(value) => value.try_into().unwrap(),
                    None => {
                        warn!("Foreign transaction {}", hex::encode(tx));
                        continue
                    }
                };
                let tx_time = u128::from_be_bytes(tx_time_bytes);

                writer.write_field(tx_time.to_string()).unwrap();
                writer.write_field(received_at.to_string()).unwrap();
                writer.write_field(tx.len().to_string()).unwrap();
                writer.write_field(num_rounds.to_string()).unwrap();
                writer.write_field(num_blocks.to_string()).unwrap();
                writer.write_record(None::<&[u8]>).unwrap();
            }
        }
    }
}
