use config::{AuthorityIdentifier, Committee, WorkerCache, WorkerId};
use crypto::NetworkPublicKey;
use network::client::NetworkClient;
use network::PrimaryToWorkerClient;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use storage::{CertificateStore, ConsensusStore};
use tokio::{
    sync::{mpsc::Sender, Mutex},
    time::{self, timeout},
};
use tonic::Status;
use tracing::{debug, error, info, warn};
use types::{
    Batch, BatchDigest, Certificate, CertificateAPI, CommittedSubDag, ConsensusCommit,
    FetchBatchesRequest, HeaderAPI,
};

use crate::proto::SubDag;

pub struct Client {
    authority_id: Arc<AuthorityIdentifier>,
    worker_cache: Arc<WorkerCache>,
    committee: Arc<Committee>,
    client: Arc<NetworkClient>,
    consensus_store: Arc<ConsensusStore>,
    certificate_store: Arc<CertificateStore>,

    tx_subdags: Sender<Result<SubDag, Status>>,
    last_subdag: AtomicU64,
    active: AtomicBool,
    synced: AtomicBool,
    queue: Mutex<VecDeque<CommittedSubDag>>,
}

impl Client {
    pub fn new(
        authority_id: Arc<AuthorityIdentifier>,
        worker_cache: Arc<WorkerCache>,
        committee: Arc<Committee>,
        client: Arc<NetworkClient>,
        consensus_store: Arc<ConsensusStore>,
        certificate_store: Arc<CertificateStore>,
        tx_subdags: Sender<Result<SubDag, Status>>,
        last_subdag: u64,
    ) -> Self {
        Client {
            authority_id,
            worker_cache,
            committee,
            client,
            consensus_store,
            certificate_store,
            tx_subdags,
            last_subdag: AtomicU64::new(last_subdag),
            active: AtomicBool::new(true),
            synced: AtomicBool::new(false),
            queue: Mutex::new(VecDeque::new()),
        }
    }

    pub async fn run(&self) {
        while self.is_active() {
            if self.is_synced() {
                // send real-time data
                if let Err(e) = self.process_queue().await {
                    warn!("Failed to process client's queue: {}", e);
                    self.close();
                    return;
                }
            } else {
                // send historical data
                if let Err(e) = self.process_history().await {
                    warn!("Failed to process client's history: {}", e);
                    self.close();
                    return;
                }
            }
            time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub fn close(&self) {
        self.active.store(false, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn is_synced(&self) -> bool {
        self.synced.load(Ordering::Relaxed)
    }

    pub fn set_synced(&self, value: bool) {
        self.synced.store(value, Ordering::Relaxed)
    }

    pub async fn enqueue(&self, subdag: CommittedSubDag) {
        self.queue.lock().await.push_back(subdag);
    }

    async fn dequeue(&self) -> Option<CommittedSubDag> {
        self.queue.lock().await.pop_front()
    }

    async fn pending_subdag_id(&self) -> Option<u64> {
        match self.queue.lock().await.front() {
            Some(subdag) => Some(subdag.sub_dag_index),
            _ => None,
        }
    }

    async fn process_queue(&self) -> Result<(), Box<dyn Error>> {
        if let Some(subdag) = self.dequeue().await {
            let next_id = self.last_subdag.load(Ordering::Relaxed) + 1;
            if subdag.sub_dag_index < next_id {
                info!("Skip pending subdag");
                return Ok(());
            }
            if subdag.sub_dag_index != next_id {
                Err("state has been corrupted")?;
            }
            debug!("Build consensus output");
            let output = self.output_from_subdag(&subdag).await?;
            debug!("Send consensus output");
            self.tx_subdags.send(Ok(output)).await?;
            debug!("Update client's state");
            self.last_subdag.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    async fn process_history(&self) -> Result<(), Box<dyn Error>> {
        let next_id = self.last_subdag.load(Ordering::Relaxed) + 1;
        if let Some(pending_subdag_id) = self.pending_subdag_id().await {
            if next_id >= pending_subdag_id {
                debug!("Client has received all historical data. Start processing live queue");
                self.set_synced(true);
                return Ok(());
            }
        }
        // TODO: fetch multiple commits and process them in parallel
        let commits = self.consensus_store.read_committed_sub_dags(&next_id, 1)?;
        for commit in commits {
            debug!("Build historical consensus output");
            let output = self.output_from_commit(commit).await?;
            debug!("Send historical consensus output");
            self.tx_subdags.send(Ok(output)).await?;
            debug!("Update client's state");
            self.last_subdag.fetch_add(1, Ordering::Relaxed);
            // temp check
            if self.last_subdag.load(Ordering::Relaxed) != next_id {
                panic!("Sequence corrupted!");
            }
        }
        Ok(())
    }

    async fn output_from_commit(&self, commit: ConsensusCommit) -> Result<SubDag, Box<dyn Error>> {
        let subdag = self.load_certs(commit)?;
        self.output_from_subdag(&subdag).await
    }

    async fn output_from_subdag(&self, subdag: &CommittedSubDag) -> Result<SubDag, Box<dyn Error>> {
        match timeout(Duration::from_secs(30), self.fetch_batches(subdag)).await {
            Ok(batches) => Ok(SubDag::from(&subdag, &batches)),
            Err(err) => {
                warn!("Fetching batches failed with {}", err);
                Err(err.into())
            }
        }
    }

    fn load_certs(
        &self,
        commit: ConsensusCommit,
    ) -> Result<CommittedSubDag, Box<dyn std::error::Error>> {
        let digests = commit.certificates();
        debug!(
            "Load {} certificates for subdag #{}",
            digests.len(),
            commit.sub_dag_index()
        );

        let certificates = self
            .certificate_store
            .read_all(digests)?
            .into_iter()
            .flatten()
            .collect();

        let leader = self.certificate_store.read(commit.leader())?.unwrap();

        Ok(CommittedSubDag::from_commit(commit, certificates, leader))
    }

    async fn fetch_batches(&self, subdag: &CommittedSubDag) -> HashMap<BatchDigest, Batch> {
        let mut fetched_batches = HashMap::new();

        if subdag.num_batches() == 0 {
            debug!("No batches to fetch, payload is empty");
            return fetched_batches;
        }

        let mut workers_and_digests =
            HashMap::<NetworkPublicKey, (HashSet<BatchDigest>, HashSet<NetworkPublicKey>)>::new();

        for cert in &subdag.certificates {
            for (digest, (worker_id, _)) in cert.header().payload().iter() {
                let own_worker = self.get_own_worker(worker_id);
                let known_workers = self.get_known_workers(cert, worker_id);

                let (batches_set, workers_set) = workers_and_digests.entry(own_worker).or_default();

                batches_set.insert(*digest);
                workers_set.extend(known_workers);
            }
        }

        for (worker, (digests, known_workers)) in workers_and_digests {
            debug!(
                "Fetch {} batches from {} known workers",
                digests.len(),
                known_workers.len()
            );
            let request = FetchBatchesRequest {
                digests,
                known_workers,
            };
            // NOTE: Here we can be stuck forever, because:
            // 1. Worker blocks until all batches are available
            // 2. We also do infinite retries on failure
            let batches = loop {
                match self
                    .client
                    .fetch_batches(worker.clone(), request.clone())
                    .await
                {
                    Ok(resp) => break resp.batches,
                    Err(e) => {
                        error!("Failed to fetch batches: {e:?}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                }
            };
            for (digest, batch) in batches {
                fetched_batches.insert(digest, batch);
            }
        }

        fetched_batches
    }

    fn get_own_worker(&self, worker_id: &WorkerId) -> NetworkPublicKey {
        self.worker_cache
            .worker(
                self.committee
                    .authority(&self.authority_id)
                    .unwrap_or_else(|| {
                        panic!("Authority {} is not in the committee", self.authority_id)
                    })
                    .protocol_key(),
                worker_id,
            )
            .unwrap_or_else(|_| panic!("Worker {} is not in the worker cache", worker_id))
            .name
    }

    fn get_known_workers(
        &self,
        certificate: &Certificate,
        worker_id: &WorkerId,
    ) -> Vec<NetworkPublicKey> {
        let authorities = certificate.signed_authorities(&self.committee);
        authorities
            .into_iter()
            .filter_map(|authority| {
                let worker = self.worker_cache.worker(&authority, worker_id);
                match worker {
                    Ok(worker) => Some(worker.name),
                    Err(err) => {
                        error!("Worker {worker_id} not found for authority {authority}: {err:?}");
                        None
                    }
                }
            })
            .collect()
    }
}
