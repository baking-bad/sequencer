use std::collections::BTreeMap;
use std::str::FromStr;

use config::Parameters;
use config::WorkerCache;
use config::WorkerIndex;
use config::WorkerInfo;
use config::{Committee, CommitteeBuilder};
use crypto::NetworkKeyPair;
use fastcrypto::traits::KeyPair as _;
use node::primary_node::PrimaryNode;
use network::client::NetworkClient;
use storage::NodeStorage;
use sui_types::multiaddr::Multiaddr;
use sui_types::crypto::{get_key_pair_from_bytes, AuthorityKeyPair};

#[tokio::main]
async fn main() {
    println!("Bonjour, epta!");

    let primary_key = get_key_pair_from_bytes::<AuthorityKeyPair>([1; 128].as_slice()).unwrap().1;
    let network_key = get_key_pair_from_bytes::<NetworkKeyPair>([1; 64].as_slice()).unwrap().1;

    let committee = generate_committee();
    //println!("{:?}", committee);
    
    let workers = generate_workers();
    //println!("{:?}", workers);
    
    let parameters = Parameters::default();
    //println!("{:?}", parameters);

    let store = NodeStorage::reopen("db", None);
    
    let client = NetworkClient::new_from_keypair(&network_key);
}

fn generate_committee() -> Committee {
    CommitteeBuilder::new(0)
        .add_authority(
            get_key_pair_from_bytes::<AuthorityKeyPair>([1; 128].as_slice()).unwrap().1.public().clone(),
            100,
            Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
            get_key_pair_from_bytes::<NetworkKeyPair>([1; 64].as_slice()).unwrap().1.public().clone(),
            String::from("/ip4/127.0.0.1/udp/0"),
        )
        .add_authority(
            get_key_pair_from_bytes::<AuthorityKeyPair>([2; 128].as_slice()).unwrap().1.public().clone(),
            100,
            Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
            get_key_pair_from_bytes::<NetworkKeyPair>([2; 64].as_slice()).unwrap().1.public().clone(),
            String::from("/ip4/127.0.0.1/udp/0"),
        )
        .add_authority(
            get_key_pair_from_bytes::<AuthorityKeyPair>([3; 128].as_slice()).unwrap().1.public().clone(),
            100,
            Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
            get_key_pair_from_bytes::<NetworkKeyPair>([3; 64].as_slice()).unwrap().1.public().clone(),
            String::from("/ip4/127.0.0.1/udp/0"),
        )
        .build()
}

fn generate_workers() -> WorkerCache {
    WorkerCache {
        workers: BTreeMap::from([
            (
                get_key_pair_from_bytes::<AuthorityKeyPair>([1; 128].as_slice()).unwrap().1.public().clone(),
                WorkerIndex(BTreeMap::from([
                    (
                        0,
                        WorkerInfo {
                            name: get_key_pair_from_bytes::<NetworkKeyPair>([1; 64].as_slice()).unwrap().1.public().clone(),
                            transactions: Multiaddr::from_str("/ip4/127.0.0.1/tcp/0/http").unwrap(),
                            worker_address: Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
                        },
                    )
                ]))
            ),
            (
                get_key_pair_from_bytes::<AuthorityKeyPair>([2; 128].as_slice()).unwrap().1.public().clone(),
                WorkerIndex(BTreeMap::from([
                    (
                        0,
                        WorkerInfo {
                            name: get_key_pair_from_bytes::<NetworkKeyPair>([2; 64].as_slice()).unwrap().1.public().clone(),
                            transactions: Multiaddr::from_str("/ip4/127.0.0.1/tcp/0/http").unwrap(),
                            worker_address: Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
                        },
                    )
                ]))
            ),
            (
                get_key_pair_from_bytes::<AuthorityKeyPair>([3; 128].as_slice()).unwrap().1.public().clone(),
                WorkerIndex(BTreeMap::from([
                    (
                        0,
                        WorkerInfo {
                            name: get_key_pair_from_bytes::<NetworkKeyPair>([3; 64].as_slice()).unwrap().1.public().clone(),
                            transactions: Multiaddr::from_str("/ip4/127.0.0.1/tcp/0/http").unwrap(),
                            worker_address: Multiaddr::from_str("/ip4/127.0.0.1/udp/0").unwrap(),
                        },
                    )
                ]))
            ),
        ]),
        epoch: 0,
    }
}