use clap::Parser;
use consensus_client::WorkerClient;
use da_batcher::make_da_batch;
use pre_block::PreBlock;
use rollup_client::RollupClient;
use fastcrypto::hash::{HashFunction, Keccak256};
use log::{info, warn, error};
use tokio::task::JoinHandle;
use std::sync::Arc;
use std::time::{SystemTime, Duration};
use tokio::signal;

mod da_batcher;
mod rollup_client;
mod consensus_client;

#[derive(Clone)]
struct State {
    pub rollup_client: Arc<RollupClient>,
    pub worker_client: Arc<WorkerClient>,
}

impl State {
    pub fn new(rollup_node_url: String, worker_node_url: String) -> Self {
        Self {
            rollup_client: Arc::new(RollupClient::new(rollup_node_url)),
            worker_client: Arc::new(WorkerClient::new(worker_node_url)),
        }
    }
}

async fn broadcast_transaction(mut req: tide::Request<State>) -> tide::Result<String> {
    let tx_payload = hex::decode(req.body_string().await?)?;
    let tx_digest = Keccak256::digest(&tx_payload);
    req.state()
        .worker_client
        .as_ref()
        .clone() // cloning channel is cheap and encouraged
        .send_transaction(tx_payload).await?;
    Ok(hex::encode(tx_digest))
}

async fn get_block_by_level(req: tide::Request<State>) -> tide::Result<tide::Body> {
    let level: u32 = req.param("level")?.parse().unwrap_or(0);
    let block = req
        .state()
        .rollup_client
        .get_block_by_level(level)
        .await?;
    let res: Vec<String> = block
        .iter()
        .map(|tx_digest| hex::encode(tx_digest))
        .collect();
    tide::Body::from_json(&res)
}

async fn run_api_server(rpc_addr: String, rpc_port: u16, rollup_node_url: String, worker_node_url: String) -> anyhow::Result<()> {
    info!("[RPC server] Starting...");

    let rpc_host = format!("http://{}:{}", rpc_addr, rpc_port);
    let mut app = tide::with_state(State::new(rollup_node_url, worker_node_url));
    app.at("/broadcast").post(broadcast_transaction);
    app.at("/blocks/:level").get(get_block_by_level);
    app.listen(rpc_host).await?;
    Ok(())
}

fn now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
}

async fn run_da_task(rollup_node_url: String, primary_node_url: String) -> anyhow::Result<()> {
    info!("[DA task] Starting...");
    
    let rollup_client = RollupClient::new(rollup_node_url.clone());
    let mut connection_attempts = 0;

    let smart_rollup_address = loop {
        match rollup_client.get_rollup_address().await {
            Ok(res) => break res,
            Err(err) => {
                connection_attempts += 1;
                if connection_attempts == 10 {
                    error!("[DA task] Max attempts to connect to SR node: {}", err);
                    return Err(err)
                } else {
                    warn!("[DA task] Attempt #{} {}", connection_attempts, err);
                    tokio::time::sleep(Duration::from_secs(connection_attempts)).await;
                }
            }
        }
    };
    info!("Connected to SR node: {} at {}", smart_rollup_address, rollup_node_url);
    
    // let primary_client = PrimaryClient::new(primary_node_url);

    loop {
        let mut index = rollup_client.get_sub_dag_index().await?;
        let mut synced_at = now();

        // let stream = primary_client.get_sub_dag_stream(sub_dag_index);
        // while let Some(pre_block) = stream.next().await {

        index += 1;

        let pre_block = PreBlock::new(index, vec![vec![vec![1u8]]]);
        if !pre_block.is_leader() {
            continue;
        }

        let current_time = now();
        if synced_at < current_time - 300 {
            let last_advanced_at = rollup_client.get_last_advanced_at().await?;
            if last_advanced_at < current_time - 300 {
                break;
            } else {
                synced_at = current_time;
            }
        }

        let batch = make_da_batch(&pre_block, &smart_rollup_address)?;
        rollup_client.inject_batch(batch).await?;

        // }
    }

    Ok(())
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = String::from("http://localhost:8932"))]
    rollup_node_url: String,

    #[arg(long, default_value_t = String::from("http://localhost:9090"))]
    worker_node_url: String,

    #[arg(long, default_value_t = String::from("http://localhost:9091"))]
    primary_node_url: String,

    #[arg(long, default_value_t = String::from("0.0.0.0"))]
    rpc_addr: String,

    #[arg(long, default_value_t = 8080)]
    rpc_port: u16,
}

async fn flatten(handle: JoinHandle<anyhow::Result<()>>) -> anyhow::Result<()> {
    match handle.await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(err) => Err(anyhow::anyhow!("Failed to join: {}", err)),
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Sequencer node is launching...");

    let args = Args::parse();
    let rollup_node_url = args.rollup_node_url.clone();
    
    let api_server = tokio::spawn(async move {
        tokio::select! {
            _ = signal::ctrl_c() => Ok(()),
            res = run_api_server(
                args.rpc_addr,
                args.rpc_port,
                rollup_node_url,
                args.worker_node_url
            ) => res,
        }
    });

    let da_task = tokio::spawn(async move {
        tokio::select! {
            _ = signal::ctrl_c() => Ok(()),
            res = run_da_task(args.rollup_node_url, args.primary_node_url) => res,
        }
    }); 

    tokio::try_join!(flatten(api_server), flatten(da_task)).unwrap();
}
