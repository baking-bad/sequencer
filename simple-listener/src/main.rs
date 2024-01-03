use clap::Parser;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Channel;

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
    let args = Args::parse();
    loop {
        println!("Connecting to {}...", args.endpoint.clone());
        match connect(args.endpoint.clone()).await {
            Ok(client) => {
                println!("Connected. Exporting subdags from #{}...", args.from_id);
                match export(client, args.from_id).await {
                    Ok(_) => {
                        println!("Exit");
                        break;
                    }
                    Err(e) => {
                        println!("Failed to export: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Failed to connect: {}", e);
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
        println!(
            "Received subdag #{} with {} txs",
            subdag.id,
            total_txs(&subdag)
        );
    }

    println!("Close client");
    Ok(())
}

fn total_txs(subdag: &SubDag) -> usize {
    let mut cnt = 0;
    for payload in subdag.payloads.iter() {
        for batch in payload.batches.iter() {
            cnt += batch.transactions.len();
        }
    }
    cnt
}
