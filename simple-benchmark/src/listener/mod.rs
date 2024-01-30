use log::error;
use log::info;
use tokio::sync::mpsc;
use tokio::{sync::broadcast::Receiver, time::sleep};
use tonic::transport::Channel;
use tonic::Streaming;

use super::exporter::exporter_client::ExporterClient;
use super::exporter::*;
use super::ListenerArgs;
use std::time::Duration;

enum ClientError {
    ConnectionError(tonic::transport::Error),
    GrpcError(tonic::Status),
    NoClient,
}

impl From<tonic::transport::Error> for ClientError {
    fn from(connection_error: tonic::transport::Error) -> ClientError {
        ClientError::ConnectionError(connection_error)
    }
}

impl From<tonic::Status> for ClientError {
    fn from(status: tonic::Status) -> ClientError {
        ClientError::GrpcError(status)
    }
}

struct Client {
    endpoint: String,
    next_id: u64,
    client: ExporterClient<Channel>,
    stream: Streaming<SubDag>,
}

impl Client {
    pub async fn try_new(endpoint: String, from_id: u64) -> Result<Self, ClientError> {
        let mut client = connect(endpoint.clone()).await?;
        let stream = client.export(ExportRequest { from_id }).await?.into_inner();
        info!("Connected to {}", endpoint);
        Ok(Client {
            endpoint,
            next_id: from_id,
            client,
            stream,
        })
    }

    pub async fn reconnect(&mut self) -> Result<(), ClientError> {
        self.client = connect(self.endpoint.clone()).await?;
        self.stream = self
            .client
            .export(ExportRequest {
                from_id: self.next_id,
            })
            .await?
            .into_inner();
        info!("Connected to {}", self.endpoint);
        Ok(())
    }

    pub async fn next_message(&mut self) -> Result<Option<SubDag>, ClientError> {
        match self.stream.message().await? {
            Some(subdag) => {
                self.next_id = self.next_id + 1;
                Ok(Some(subdag))
            }
            None => Ok(None),
        }
    }

    pub async fn next_message_opt(
        client: &mut Option<Self>,
    ) -> Result<Option<SubDag>, ClientError> {
        match client {
            None => Err(ClientError::NoClient),
            Some(client) => client.next_message().await,
        }
    }
}

pub async fn run(args: ListenerArgs, mut rx_stop: Receiver<()>) {
    let (tx_reconnect, mut rx_reconnect) = mpsc::channel::<()>(1);
    let mut client: Option<Client> =
        match Client::try_new(args.endpoint.clone(), args.from_id).await {
            Ok(client) => Some(client),
            Err(_e) => {
                error!("Could not initialize client");
                let tx_reconnect = tx_reconnect.clone();
                sleep(Duration::from_secs(1)).await;
                tokio::spawn(async move { tx_reconnect.send(()).await });
                None
            }
        };
    let mut tx_writer = match args.tx_output {
        Some(path) => Some(csv::Writer::from_path(path).unwrap()),
        None => None,
    };
    loop {
        tokio::select! {
            Ok(()) = rx_stop.recv() => {
                println!("Timeout reached: listener will stop now");
                break;
            }

            Some(()) = rx_reconnect.recv() => {
                match client.take() {
                    None => {
                        match Client::try_new(args.endpoint.clone(), args.from_id).await {
                            Ok(new_client) => client = Some(new_client),
                            Err(_e) => {
                                error!("Could not initialize client");
                                let tx_reconnect = tx_reconnect.clone();
                                sleep(Duration::from_secs(1)).await;
                                tokio::spawn(async move { tx_reconnect.send(()).await });
                            }
                        }
                    }
                    Some(mut new_client) => match new_client.reconnect().await {
                        Ok(()) => { client = Some(new_client) }
                        Err(_e) => {
                            error!("Could not initialize client");
                            //necessary to keep track of next_id
                            client = Some(new_client);
                            let tx_reconnect = tx_reconnect.clone();
                            sleep(Duration::from_secs(1)).await;
                            tokio::spawn(async move { tx_reconnect.send(()).await });
                        }
                    }
                }
            }

            next_message_opt = Client::next_message_opt(&mut client) => {
                match next_message_opt {
                    Err(_e) => {
                        error!("Could not get next message from stream");
                        let tx_reconnect = tx_reconnect.clone();
                        sleep(Duration::from_secs(1)).await;
                        tokio::spawn(async move { tx_reconnect.send(()).await });
                    }
                    Ok(None) => {
                        error!("Could not get next message from stream");
                        let tx_reconnect = tx_reconnect.clone();
                        client = None;
                        sleep(Duration::from_secs(1)).await;
                        tokio::spawn(async move { tx_reconnect.send(()).await });
                    }

                    Ok(Some(subdag)) => {
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
                }
            }
        }
    }
}

async fn connect(endpoint: String) -> Result<ExporterClient<Channel>, tonic::transport::Error> {
    ExporterClient::connect(endpoint).await
}

mod stats;
