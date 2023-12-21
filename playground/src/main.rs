use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

use bytes::Bytes;
use config::WorkerIndex;
use config::WorkerInfo;
use config::{ChainIdentifier, Committee, CommitteeBuilder, Parameters, WorkerCache};
use crypto::{get_key_pair_from_bytes, AuthorityKeyPair, NetworkKeyPair};
use fastcrypto::traits::KeyPair as _;
use network::client::NetworkClient;
use node::execution_state::SimpleExecutionState;
use node::primary_node::PrimaryNode;
use node::worker_node::WorkerNode;
use prometheus::Registry;
use storage::NodeStorage;
use test_utils::cluster::Cluster;
use tokio::sync::mpsc::channel;
use types::TransactionProto;
use utils::metrics::RegistryService;
use utils::network::Multiaddr;
use utils::protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use worker::TrivialTransactionValidator;

#[tokio::main]
async fn main() {
    println!("Bonjour, epta!");

    let _guard = utils::tracing::setup_tracing("info", "info");

    let mut cluster = Cluster::new(Some(Parameters::default()));

    cluster
        .start(Some(4), Some(1), Some(Duration::from_millis(100)))
        .await;

    let client = cluster.authority(0).new_transactions_client(&0).await;

    let mut receiver = cluster
        .authority(0)
        .primary()
        .await
        .tx_transaction_confirmation
        .subscribe();

    let mut c = client.clone();
    tokio::spawn(async move {
        let tx = TransactionProto {
            transaction: Bytes::from(bcs::to_bytes("qwerty").unwrap()),
        };
        c.submit_transaction(tx).await.unwrap();
    });

    if let Ok(result) = receiver.recv().await {
        println!("Confirmed: {result:?}");
    } else {
        println!("Failed to receive tx");
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    for node in cluster.authorities().await {
        cluster.stop_node(node.id).await;
    }

    println!("Exited");
}

#[allow(dead_code)]
async fn start_worker(id: u32) -> WorkerNode {
    let primary_key = get_key_pair_from_bytes::<AuthorityKeyPair>([1; 128].as_slice()).unwrap();
    let network_key = get_key_pair_from_bytes::<NetworkKeyPair>([1; 64].as_slice()).unwrap();

    let worker_key = get_key_pair_from_bytes::<NetworkKeyPair>([11; 64].as_slice()).unwrap();

    let committee = generate_committee();
    //println!("{:?}", committee);

    let worker_cache = generate_workers();
    //println!("{:?}", worker_cache);

    let parameters = Parameters::default();
    //println!("{:?}", parameters);

    let store = NodeStorage::reopen("db/worker", None);

    let client = NetworkClient::new_from_keypair(&network_key);

    let worker = WorkerNode::new(
        id,
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
        parameters.clone(),
        RegistryService::new(Registry::new()),
    );

    worker
        .start(
            primary_key.public().clone(),
            worker_key,
            committee,
            worker_cache,
            client,
            &store,
            TrivialTransactionValidator,
            None,
        )
        .await
        .unwrap();

    worker
}

#[allow(dead_code)]
async fn start_primary() -> PrimaryNode {
    let primary_key = get_key_pair_from_bytes::<AuthorityKeyPair>([1; 128].as_slice()).unwrap();
    let network_key = get_key_pair_from_bytes::<NetworkKeyPair>([1; 64].as_slice()).unwrap();

    let committee = generate_committee();
    //println!("{:?}", committee);

    let worker_cache = generate_workers();
    //println!("{:?}", worker_cache);

    let parameters = Parameters::default();
    //println!("{:?}", parameters);

    let store = NodeStorage::reopen("db/primary", None);

    let client = NetworkClient::new_from_keypair(&network_key);

    let (_tx_transaction_confirmation, _rx_transaction_confirmation) = channel(100);

    let primary = PrimaryNode::new(parameters, RegistryService::new(Registry::new()));

    primary
        .start(
            primary_key,
            network_key,
            committee,
            ChainIdentifier::unknown(),
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown),
            worker_cache,
            client,
            &store,
            SimpleExecutionState::new(_tx_transaction_confirmation),
        )
        .await
        .unwrap();

    primary
}

#[allow(dead_code)]
fn generate_committee() -> Committee {
    CommitteeBuilder::new(0)
        .add_authority(
            get_key_pair_from_bytes::<AuthorityKeyPair>([1; 128].as_slice())
                .unwrap()
                .public()
                .clone(),
            100,
            Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
            get_key_pair_from_bytes::<NetworkKeyPair>([1; 64].as_slice())
                .unwrap()
                .public()
                .clone(),
            String::from("/ip4/127.0.0.1/udp/0"),
        )
        .add_authority(
            get_key_pair_from_bytes::<AuthorityKeyPair>([2; 128].as_slice())
                .unwrap()
                .public()
                .clone(),
            100,
            Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
            get_key_pair_from_bytes::<NetworkKeyPair>([2; 64].as_slice())
                .unwrap()
                .public()
                .clone(),
            String::from("/ip4/127.0.0.1/udp/0"),
        )
        .add_authority(
            get_key_pair_from_bytes::<AuthorityKeyPair>([3; 128].as_slice())
                .unwrap()
                .public()
                .clone(),
            100,
            Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
            get_key_pair_from_bytes::<NetworkKeyPair>([3; 64].as_slice())
                .unwrap()
                .public()
                .clone(),
            String::from("/ip4/127.0.0.1/udp/0"),
        )
        .build()
}

#[allow(dead_code)]
fn generate_workers() -> WorkerCache {
    WorkerCache {
        workers: BTreeMap::from([
            (
                get_key_pair_from_bytes::<AuthorityKeyPair>([1; 128].as_slice())
                    .unwrap()
                    .public()
                    .clone(),
                WorkerIndex(BTreeMap::from([(
                    0,
                    WorkerInfo {
                        name: get_key_pair_from_bytes::<NetworkKeyPair>([1; 64].as_slice())
                            .unwrap()
                            .public()
                            .clone(),
                        transactions: Multiaddr::from_str("/ip4/127.0.0.1/tcp/0/http").unwrap(),
                        worker_address: Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
                    },
                )])),
            ),
            (
                get_key_pair_from_bytes::<AuthorityKeyPair>([2; 128].as_slice())
                    .unwrap()
                    .public()
                    .clone(),
                WorkerIndex(BTreeMap::from([(
                    0,
                    WorkerInfo {
                        name: get_key_pair_from_bytes::<NetworkKeyPair>([2; 64].as_slice())
                            .unwrap()
                            .public()
                            .clone(),
                        transactions: Multiaddr::from_str("/ip4/127.0.0.1/tcp/0/http").unwrap(),
                        worker_address: Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
                    },
                )])),
            ),
            (
                get_key_pair_from_bytes::<AuthorityKeyPair>([3; 128].as_slice())
                    .unwrap()
                    .public()
                    .clone(),
                WorkerIndex(BTreeMap::from([(
                    0,
                    WorkerInfo {
                        name: get_key_pair_from_bytes::<NetworkKeyPair>([3; 64].as_slice())
                            .unwrap()
                            .public()
                            .clone(),
                        transactions: Multiaddr::from_str("/ip4/127.0.0.1/tcp/0/http").unwrap(),
                        worker_address: Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
                    },
                )])),
            ),
        ]),
        epoch: 0,
    }
}
