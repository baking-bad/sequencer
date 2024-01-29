use log::error;
use log::info;
use tokio::pin;
use tokio::time::Instant;
use tokio::{sync::broadcast::Receiver, time::sleep};
use tonic::transport::Channel;

use super::exporter::exporter_client::ExporterClient;
use super::exporter::*;
use super::ListenerArgs;
use std::time::Duration;

pub async fn run(args: ListenerArgs, mut rx_stop: Receiver<()>) {
    let time_to_next_request = sleep(Duration::from_secs(1));
    pin!(time_to_next_request);
    loop {
        tokio::select! {
                () = &mut time_to_next_request => {
                    info!("Connecting to {}...", args.endpoint);
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
                    time_to_next_request.as_mut().reset(Instant::now() + Duration::from_secs(1));
                }

                Ok(()) = rx_stop.recv() => {
                    info!("Timeout reached, stopping listener!");
                    break;
            }
        }
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
            stats::write_tx_stats(&subdag, writer);
        }

        let stats = stats::stats(&subdag);
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
            info!("Empty subdag #{}", subdag.id,);
        }
    }

    info!("Close client");
    Ok(())
}

mod stats;
