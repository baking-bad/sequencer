// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use clap::{Parser, Subcommand};
use config::{ChainIdentifier, Committee, Import, Parameters, WorkerCache, WorkerId};
use crypto::{KeyPair, NetworkKeyPair, AuthorityKeyPair, get_key_pair_from_rng};
use eyre::Context;
use fastcrypto::traits::KeyPair as _;
use utils::metrics::RegistryService;
use narwhal_node as node;
use narwhal_node::primary_node::PrimaryNode;
use narwhal_node::worker_node::WorkerNode;
use network::client::NetworkClient;
use node::{
    execution_state::SimpleExecutionState,
    metrics::{primary_metrics_registry, start_prometheus_server, worker_metrics_registry},
};
use prometheus::Registry;
use std::path::{Path, PathBuf};
use storage::{CertificateStoreCacheMetrics, NodeStorage};
use crypto::keypair_file::{
    read_authority_keypair_from_file, read_network_keypair_from_file,
    write_authority_keypair_to_file, write_network_keypair_to_file,
};
use utils::protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use tokio::sync::mpsc::channel;
use tracing::{info, warn};
use worker::TrivialTransactionValidator;

#[derive(Parser)]
#[command(author, version, about)]
/// A production implementation of Narwhal and Bullshark.
struct App {
    #[arg(short, action = clap::ArgAction::Count)]
    /// Sets the level of verbosity
    verbosity: u8,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Save an encoded bls12381 keypair (Base64 encoded `privkey`) to file
    GenerateKeys {
        /// The file where to save the encoded authority key pair
        #[arg(long)]
        filename: PathBuf,
    },
    /// Save an encoded ed25519 network keypair (Base64 encoded `flag || privkey`) to file
    GenerateNetworkKeys {
        /// The file where to save the encoded network key pair
        #[arg(long)]
        filename: PathBuf,
    },
    /// Get the public key from a keypair file
    GetPubKey {
        /// The file where the keypair is stored
        #[arg(long)]
        filename: PathBuf,
    },
    /// Get the network public key from a keypair file
    GetNetworkPubKey {
        /// The file where the keypair is stored
        #[arg(long)]
        filename: PathBuf,
    },
    /// Run a node
    Run {
        /// The file containing the node's primary keys
        #[arg(long)]
        primary_keys: PathBuf,
        /// The file containing the node's primary network keys
        #[arg(long)]
        primary_network_keys: PathBuf,
        /// The file containing the node's worker network keys
        #[arg(long)]
        worker_keys: PathBuf,
        /// The file containing committee information
        #[arg(long)]
        committee: String,
        /// The file containing worker information
        #[arg(long)]
        workers: String,
        /// The file containing the node parameters
        #[arg(long)]
        parameters: Option<String>,
        /// The path where to create the data store
        #[arg(long)]
        store: PathBuf,

        #[command(subcommand)]
        subcommand: NodeType,
    },
    /// Run a primary with a single worker
    RunComb {
        /// The file containing the node's primary keys
        #[arg(long)]
        primary_keys: PathBuf,
        /// The file containing the node's primary network keys
        #[arg(long)]
        primary_network_keys: PathBuf,
        /// The file containing the node's worker network keys
        #[arg(long)]
        worker_keys: PathBuf,
        /// The file containing committee information
        #[arg(long)]
        committee: String,
        /// The file containing worker information
        #[arg(long)]
        workers: String,
        /// The file containing the node parameters
        #[arg(long)]
        parameters: Option<String>,
        /// The path where to create the data store
        #[arg(long)]
        primary_store: PathBuf,
        /// The path where to create the data store
        #[arg(long)]
        worker_store: PathBuf,
    },
}

#[derive(Subcommand)]
enum NodeType {
    /// Run a single primary
    Primary,
    /// Run a single worker
    Worker {
        /// The worker Id
        id: WorkerId,
    },
}

#[tokio::main]
async fn main() -> Result<(), eyre::Report> {
    let app = App::parse();

    let tracing_level = match app.verbosity {
        0 => "error",
        1 => "warn",
        2 => "info",
        3 => "debug",
        _ => "trace",
    };

    // some of the network is very verbose, so we require more 'v's
    let network_tracing_level = match app.verbosity {
        0 | 1 => "error",
        2 => "warn",
        3 => "info",
        4 => "debug",
        _ => "trace",
    };

    let _guard = utils::tracing::setup_tracing(tracing_level, network_tracing_level);

    match &app.command {
        Commands::GenerateKeys { filename } => {
            let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng);
            write_authority_keypair_to_file(&keypair, filename).unwrap();
        }
        Commands::GenerateNetworkKeys { filename } => {
            let network_keypair: NetworkKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng);
            write_network_keypair_to_file(&network_keypair, filename).unwrap();
        }
        Commands::GetPubKey { filename } => {

            match read_authority_keypair_from_file(filename) {
                Ok(kp) => println!("{:?}", kp.public()),
                Err(e) => {
                    println!("Failed to read keypair at path {:?} err: {:?}", filename, e)
                }
            }

/*            match read_network_keypair_from_file(filename) {
                Ok(keypair) => {
                    // Network keypair file is stored as `flag || privkey`.
                    println!("{:?}", keypair.public())
                }
                Err(_) => {
                    // Authority keypair file is stored as `privkey`.
                    match read_authority_keypair_from_file(filename) {
                        Ok(kp) => println!("{:?}", kp.public()),
                        Err(e) => {
                            println!("Failed to read keypair at path {:?} err: {:?}", filename, e)
                        }
                    }
                }
            }*/
        }
        Commands::GetNetworkPubKey { filename } => {
            match read_network_keypair_from_file(filename) {
                Ok(kp) => println!("{:?}", kp.public()),
                Err(e) => {
                    println!("Failed to read keypair at path {:?} err: {:?}", filename, e)
                }
            }
        }
        Commands::Run {
            primary_keys,
            primary_network_keys,
            worker_keys,
            committee,
            workers,
            parameters,
            store,
            subcommand,
        } => {
            let primary_keypair = read_authority_keypair_from_file(primary_keys)
                .expect("Failed to load the node's primary keypair");
            let primary_network_keypair = read_network_keypair_from_file(primary_network_keys)
                .expect("Failed to load the node's primary network keypair");
            let worker_keypair = read_network_keypair_from_file(worker_keys)
                .expect("Failed to load the node's worker keypair");

            let mut committee =
                Committee::import(committee).context("Failed to load the committee information")?;
            committee.load();

            let authority_id = committee
                .authority_by_key(primary_keypair.public())
                .unwrap()
                .id();

            let registry = match subcommand {
                NodeType::Primary => primary_metrics_registry(authority_id),
                NodeType::Worker { id } => worker_metrics_registry(*id, authority_id),
            };

            run(
                subcommand,
                workers,
                parameters.as_deref(),
                store,
                committee,
                primary_keypair,
                primary_network_keypair,
                worker_keypair,
                registry,
            )
            .await?
        }
        Commands::RunComb {
            primary_keys,
            primary_network_keys,
            worker_keys,
            committee,
            workers,
            parameters,
            primary_store,
            worker_store
        } => {
            let primary_keypair = read_authority_keypair_from_file(primary_keys)
                .expect("Failed to load the node's primary keypair");
            let primary_network_keypair = read_network_keypair_from_file(primary_network_keys)
                .expect("Failed to load the node's primary network keypair");
            let worker_keypair = read_network_keypair_from_file(worker_keys)
                .expect("Failed to load the node's worker keypair");

            let mut committee =
                Committee::import(committee).context("Failed to load the committee information")?;
            committee.load();

            run_comb(
                primary_keypair,
                primary_network_keypair,
                worker_keypair,
                committee,
                workers,
                parameters.as_deref(),
                primary_store,
                worker_store,
            )
            .await?;
        }
    }

    Ok(())
}

// Runs either a worker or a primary.
async fn run(
    node_type: &NodeType,
    workers: &str,
    parameters: Option<&str>,
    store: &Path,
    committee: Committee,
    primary_keypair: KeyPair,
    primary_network_keypair: NetworkKeyPair,
    worker_keypair: NetworkKeyPair,
    registry: Registry,
) -> Result<(), eyre::Report> {
    // Read the workers and node's keypair from file.
    let worker_cache =
        WorkerCache::import(workers).context("Failed to load the worker information")?;

    // Load default parameters if none are specified.
    let parameters = match parameters {
        Some(filename) => {
            Parameters::import(filename).context("Failed to load the node's parameters")?
        }
        None => Parameters::default(),
    };

    // Make the data store.
    let registry_service = RegistryService::new(Registry::new());
    let certificate_store_cache_metrics =
        CertificateStoreCacheMetrics::new(&registry_service.default_registry());

    let store = NodeStorage::reopen(store, Some(certificate_store_cache_metrics));

    let client = NetworkClient::new_from_keypair(&primary_network_keypair);

    // The channel returning the result for each transaction's execution.
    let (_tx_transaction_confirmation, _rx_transaction_confirmation) = channel(100);

    // Check whether to run a primary, a worker, or an entire authority.
    let (primary, worker) = match node_type {
        NodeType::Primary => {
            let primary = PrimaryNode::new(parameters.clone(), registry_service);

            primary
                .start(
                    primary_keypair,
                    primary_network_keypair,
                    committee,
                    ChainIdentifier::unknown(),
                    ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
                    worker_cache,
                    client.clone(),
                    &store,
                    SimpleExecutionState::new(_tx_transaction_confirmation),
                )
                .await?;

            (Some(primary), None)
        }
        NodeType::Worker { id } => {
            let worker = WorkerNode::new(
                *id,
                ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
                parameters.clone(),
                registry_service,
            );

            worker
                .start(
                    primary_keypair.public().clone(),
                    worker_keypair,
                    committee,
                    worker_cache,
                    client,
                    &store,
                    TrivialTransactionValidator,
                    None,
                )
                .await?;

            (None, Some(worker))
        }
    };

    // spin up prometheus server exporter
    let prom_address = parameters.prometheus_metrics.socket_addr;
    info!(
        "Starting Prometheus HTTP metrics endpoint at {}",
        prom_address
    );
    let _metrics_server_handle = start_prometheus_server(prom_address, &registry);

    if let Some(primary) = primary {
        primary.wait().await;
    } else if let Some(worker) = worker {
        worker.wait().await;
    }

    // If this expression is reached, the program ends and all other tasks terminate.
    Ok(())
}

// Runs either a worker or a primary.
async fn run_comb(
    primary_keypair: KeyPair,
    primary_network_keypair: NetworkKeyPair,
    worker_keypair: NetworkKeyPair,
    committee: Committee,
    workers: &str,
    parameters: Option<&str>,
    primary_store: &Path,
    worker_store: &Path,
) -> Result<(), eyre::Report> {

    let authority_id = committee
        .authority_by_key(primary_keypair.public())
        .unwrap()
        .id();

    // Read the workers and node's keypair from file.
    let worker_cache =
        WorkerCache::import(workers).context("Failed to load the worker information")?;

    // Load default parameters if none are specified.
    let parameters = match parameters {
        Some(filename) => {
            Parameters::import(filename).context("Failed to load the node's parameters")?
        }
        None => Parameters::default(),
    };

    // Make primary's data store.
    let primary_registry_service = RegistryService::new(Registry::new());
    let primary_store_cache_metrics = CertificateStoreCacheMetrics::new(&primary_registry_service.default_registry());
    let primary_store = NodeStorage::reopen(primary_store, Some(primary_store_cache_metrics));

    // Make worker's data store.
    let worker_registry_service = RegistryService::new(Registry::new());
    let worker_store_cache_metrics = CertificateStoreCacheMetrics::new(&worker_registry_service.default_registry());
    let worker_store = NodeStorage::reopen(worker_store, Some(worker_store_cache_metrics));

    let client = NetworkClient::new_from_keypair(&primary_network_keypair);

    // The channel returning the result for each transaction's execution.
    let (_tx_transaction_confirmation, _rx_transaction_confirmation) = channel(100);

    let primary = PrimaryNode::new(parameters.clone(), primary_registry_service);

    primary
        .start(
            primary_keypair.copy(),
            primary_network_keypair,
            committee.clone(),
            ChainIdentifier::unknown(),
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
            worker_cache.clone(),
            client.clone(),
            &primary_store,
            SimpleExecutionState::new(_tx_transaction_confirmation),
        )
        .await?;

    let worker = WorkerNode::new(
        0,
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
        parameters.clone(),
        worker_registry_service,
    );

    worker
        .start(
            primary_keypair.public().clone(),
            worker_keypair,
            committee,
            worker_cache,
            client,
            &worker_store,
            TrivialTransactionValidator,
            None,
        )
        .await?;

    // spin up prometheus server exporter
    let primary_registry = primary_metrics_registry(authority_id);
    let prom_address = parameters.prometheus_metrics.socket_addr;
    info!("Starting Prometheus HTTP metrics endpoint at {prom_address}");
    let _primary_metrics_server_handle = start_prometheus_server(prom_address, &primary_registry);
    
    // spin up prometheus server exporter
    let worker_registry = worker_metrics_registry(0, authority_id);
    let worker_prom_address = parameters.worker_prometheus_metrics.socket_addr;
    info!("Starting Worker Prometheus HTTP metrics endpoint at {worker_prom_address}");
    let _worker_metrics_server_handle = start_prometheus_server(worker_prom_address, &worker_registry);

    primary.wait().await;
    worker.wait().await;

    // If this expression is reached, the program ends and all other tasks terminate.
    Ok(())
}
