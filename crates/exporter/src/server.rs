use async_trait::async_trait;
use futures::FutureExt;
use std::net::SocketAddr;
use std::result::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, info, warn};

use config::{AuthorityIdentifier, Committee, WorkerCache};
use network::client::NetworkClient;
use storage::{CertificateStore, ConsensusStore};
use types::{CommittedSubDag, ConditionalBroadcastReceiver};
use utils::metered_channel;

use crate::client::Client;
use crate::proto::{ExportRequest, Exporter, ExporterServer, SubDag};

pub struct ExporterService {
    authority_id: Arc<AuthorityIdentifier>,
    worker_cache: Arc<WorkerCache>,
    committee: Arc<Committee>,
    client: Arc<NetworkClient>,
    consensus_store: Arc<ConsensusStore>,
    certificate_store: Arc<CertificateStore>,

    clients: Mutex<Vec<(Arc<Client>, JoinHandle<()>)>>,
}

#[async_trait]
impl Exporter for ExporterService {
    type ExportStream = ReceiverStream<Result<SubDag, Status>>;

    async fn export(
        &self,
        request: Request<ExportRequest>,
    ) -> Result<Response<Self::ExportStream>, Status> {
        let (tx_subdags, rx_subdags) = mpsc::channel(32);
        let last_subdag = request.into_inner().from_id;
        let client = Arc::new(Client::new(
            self.authority_id.clone(),
            self.worker_cache.clone(),
            self.committee.clone(),
            self.client.clone(),
            self.consensus_store.clone(),
            self.certificate_store.clone(),
            tx_subdags,
            last_subdag,
        ));

        let _client = client.clone();
        let handle = tokio::spawn(async move {
            _client.run().await;
        });

        let mut clients = self.clients.lock().await;
        clients.push((client, handle));

        info!(
            "New client with last subdag #{} registered. Total clients: {}",
            last_subdag,
            clients.len()
        );

        Ok(Response::new(ReceiverStream::new(rx_subdags)))
    }
}

impl ExporterService {
    pub fn spawn(
        authority_id: Arc<AuthorityIdentifier>,
        worker_cache: Arc<WorkerCache>,
        committee: Arc<Committee>,
        client: Arc<NetworkClient>,
        consensus_store: Arc<ConsensusStore>,
        certificate_store: Arc<CertificateStore>,
        mut rx_shutdown: ConditionalBroadcastReceiver,
        rx_subdags: metered_channel::Receiver<CommittedSubDag>,
    ) -> JoinHandle<()> {
        let grpc_address = committee
            .authority(&authority_id)
            .expect("Committee doesn't contain our authority")
            .grpc_address()
            .to_socket_addr()
            .expect("Invalid primary's grpc address");

        let service = Arc::new(ExporterService {
            authority_id,
            worker_cache,
            committee,
            client,
            consensus_store,
            certificate_store,
            clients: Mutex::new(Vec::new()),
        });
        let _service = service.clone();

        let (tx_server_shutdown, rx_server_shutdown) = tokio::sync::oneshot::channel::<()>();
        let server_handle =
            tokio::spawn(Self::run_server(grpc_address, _service, rx_server_shutdown));

        let (tx_processor_shutdown, rx_processor_shutdown) = tokio::sync::oneshot::channel::<()>();
        let processor_handle = tokio::spawn(async move {
            Self::run_processor(service, rx_subdags, rx_processor_shutdown)
                .await
                .unwrap();
        });

        tokio::spawn(async move {
            let _ = rx_shutdown.receiver.recv().await;

            info!("Shutdown signal received");

            tx_server_shutdown.send(()).unwrap();
            tx_processor_shutdown.send(()).unwrap();

            match timeout(Duration::from_secs(2), server_handle).await {
                Ok(_) => {
                    info!("Exporter server gracefully shut down");
                }
                Err(err) => {
                    warn!("Exporter server timed out: {}", err);
                }
            }

            match timeout(Duration::from_secs(2), processor_handle).await {
                Ok(_) => {
                    info!("Exporter processor gracefully shut down");
                }
                Err(err) => {
                    warn!("Exporter processor timed out: {}", err);
                }
            }
        })
    }

    async fn run_server(
        addr: SocketAddr,
        service: Arc<ExporterService>,
        rx_shutdown: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), Box<tonic::transport::Error>> {
        info!("Run exporter gRPC server at {addr}");
        Server::builder()
            .add_service(ExporterServer::from_arc(service))
            .serve_with_shutdown(addr, rx_shutdown.map(|_| ()))
            .await?;
        Ok(())
    }

    async fn run_processor(
        service: Arc<ExporterService>,
        mut rx_subdags: metered_channel::Receiver<CommittedSubDag>,
        mut rx_shutdown: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Run exporter processor");
        loop {
            tokio::select! {
                Some(subdag) = rx_subdags.recv() => {
                    debug!("Committed subdag #{} received", subdag.sub_dag_index);
                    let mut clients = service.clients.lock().await;
                    let mut i = 0;
                    while i < clients.len() {
                        let (client, _) = &clients[i];
                        if client.is_active() {
                            debug!("Enqueue subdag #{} for client #{}", subdag.sub_dag_index, i);
                            client.enqueue(subdag.clone()).await;
                            i += 1;
                        }
                        else {
                            debug!("Drop client #{} due to inactivity", i);
                            clients.remove(i);
                        }
                    }
                }
                _ = &mut rx_shutdown => {
                    debug!("Shutdown signal received");
                    let mut clients = service.clients.lock().await;
                    for (i, (client, _)) in clients.iter().enumerate() {
                        debug!("Close client #{} due to sutting down", i);
                        client.close();
                        // TODO: wait for client's handle to finish
                    }
                    debug!("Drop {} clients due to sutting down", clients.len());
                    clients.clear();
                    return Ok(());
                }
            }
        }
    }
}
