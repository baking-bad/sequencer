use clap::Parser;
use fastcrypto::hash::{HashFunction, Keccak256};
use std::sync::Arc;

mod da_batcher;
mod rollup_client;

use rollup_client::RollupClient;

#[derive(Clone)]
struct State {
    pub rollup_client: Arc<RollupClient>,
}

impl State {
    pub fn new(rollup_node_url: String) -> Self {
        Self {
            rollup_client: Arc::new(RollupClient::new(rollup_node_url)),
        }
    }
}

async fn broadcast_transaction(mut req: tide::Request<State>) -> tide::Result<String> {
    // TODO: send to consensus node
    let tx_payload = req.body_string().await?;
    let tx_digest = Keccak256::digest(hex::decode(tx_payload)?);
    Ok(hex::encode(tx_digest))
}

async fn get_block_by_level(req: tide::Request<State>) -> tide::Result<tide::Body> {
    let level: u32 = req.param("level")?.parse().unwrap_or(0);
    let block_bytes = req
        .state()
        .rollup_client
        .store_get(format!("/blocks/{level}"))
        .await?;
    let block: Vec<[u8; 32]> = serde_json::from_slice(&block_bytes)?;
    let res: Vec<String> = block
        .iter()
        .map(|tx_digest| hex::encode(tx_digest))
        .collect();
    tide::Body::from_json(&res)
}

async fn run_api_server(rpc_host: String, rollup_node_url: String) -> tide::Result<()> {
    let mut app = tide::with_state(State::new(rollup_node_url));
    app.at("/broadcast").post(broadcast_transaction);
    app.at("/blocks/:level").get(get_block_by_level);
    app.listen(rpc_host).await?;
    Ok(())
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = String::from("http://localhost:8932"))]
    rollup_node_url: String,

    #[arg(long, default_value_t = String::from("127.0.0.1"))]
    rpc_addr: String,

    #[arg(long, default_value_t = 8080)]
    rpc_port: u16,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = Args::parse();
    let rpc_host = format!("http://{}:{}", args.rpc_addr, args.rpc_port);

    tokio::try_join!(run_api_server(rpc_host, args.rollup_node_url)).unwrap();
}
