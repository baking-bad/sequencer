// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use consensus_client::WorkerClient;
use fixture::{run_da_task_with_mocked_consensus, run_da_task_with_mocked_rollup};
use log::{error, info};
use rollup_client::RollupClient;
use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::task::JoinHandle;

use crate::consensus_client::PrimaryClient;
use crate::da_batcher::publish_pre_blocks;

mod consensus_client;
mod da_batcher;
mod rollup_client;
mod fixture;

#[derive(Clone)]
struct AppState {
    pub rollup_client: Arc<RollupClient>,
    pub worker_client: Arc<WorkerClient>,
}

impl AppState {
    pub fn new(rollup_node_url: String, worker_node_url: String) -> Self {
        Self {
            rollup_client: Arc::new(RollupClient::new(rollup_node_url)),
            worker_client: Arc::new(WorkerClient::new(worker_node_url)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Transaction {
    pub data: String,
}

async fn broadcast_transaction(
    State(state): State<AppState>,
    Json(tx): Json<Transaction>,
) -> Result<Json<String>, StatusCode> {
    info!("Broadcasting tx `{}`", tx.data);
    let tx_payload = hex::decode(tx.data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state
        .worker_client
        .as_ref()
        .clone() // cloning channel is cheap and encouraged
        .send_transaction(tx_payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(format!("OK")))
}

async fn get_block_by_level(
    State(state): State<AppState>,
    Path(level): Path<u32>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let block = state
        .rollup_client
        .get_block_by_level(level)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(txs) = block {
        let res: Vec<String> = txs.iter().map(|tx_digest| hex::encode(tx_digest)).collect();
        Ok(Json(res))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn get_head(State(state): State<AppState>) -> Result<Json<u32>, StatusCode> {
    let head = state
        .rollup_client
        .get_head()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(head))
}

async fn get_authorities(
    State(state): State<AppState>,
    Path(epoch): Path<u64>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let authorities = state
        .rollup_client
        .get_authorities(epoch)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let res: Vec<String> = authorities.into_iter().map(|a| hex::encode(a)).collect();
    Ok(Json(res))
}

async fn run_api_server(
    rpc_addr: String,
    rpc_port: u16,
    rollup_node_url: String,
    worker_node_url: String,
) -> anyhow::Result<()> {
    info!("[RPC server] Starting...");

    let rpc_host = format!("{}:{}", rpc_addr, rpc_port);

    let app = Router::new()
        .route("/broadcast", post(broadcast_transaction))
        .route("/blocks/:level", get(get_block_by_level))
        .route("/authorities/:epoch", get(get_authorities))
        .route("/head", get(get_head))
        .with_state(AppState::new(rollup_node_url, worker_node_url));

    let listener = tokio::net::TcpListener::bind(rpc_host).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn run_da_task_real(
    node_id: u8,
    rollup_node_url: String,
    primary_node_url: String,
) -> anyhow::Result<()> {
    info!("[DA task] Starting...");

    let rollup_client = RollupClient::new(rollup_node_url.clone());
    let smart_rollup_address = rollup_client.connect().await?;

    let mut primary_client = PrimaryClient::new(primary_node_url);

    loop {
        let from_id = rollup_client.get_next_index().await?;
        let (tx, rx) = mpsc::channel();
        info!("[DA task] Starting from index #{}", from_id);

        tokio::select! {
            res = primary_client.subscribe_pre_blocks(from_id - 1, tx) => {
                if let Err(err) = res {
                    error!("[DA fetch] Failed with: {}", err);
                }
            },
            res = publish_pre_blocks(&rollup_client, &smart_rollup_address, node_id, rx) => {
                if let Err(err) = res {
                    error!("[DA publish] Failed with: {}", err);
                }
            },
        };

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn run_da_task(
    node_id: u8,
    rollup_node_url: String,
    primary_node_url: String,
    mock_consensus: bool,
    mock_rollup: bool,
) -> anyhow::Result<()> {
    match (mock_consensus, mock_rollup) {
        (false, false) => run_da_task_real(node_id, rollup_node_url, primary_node_url).await,
        (true, false) => run_da_task_with_mocked_consensus(node_id, rollup_node_url).await,
        (false, true) => run_da_task_with_mocked_rollup(primary_node_url).await,
        (true, true) => unimplemented!()
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = String::from("http://localhost:8932"))]
    rollup_node_url: String,

    #[arg(long, default_value_t = String::from("http://localhost:64013"))]
    worker_node_url: String,

    #[arg(long, default_value_t = String::from("http://localhost:64011"))]
    primary_node_url: String,

    #[arg(long, default_value_t = String::from("0.0.0.0"))]
    rpc_addr: String,

    #[arg(long, default_value_t = 8080)]
    rpc_port: u16,

    #[arg(long, default_value_t = 1)]
    node_id: u8,

    #[arg(long, default_value_t = false)]
    mock_consensus: bool,

    #[arg(long, default_value_t = false)]
    mock_rollup: bool,
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
                args.worker_node_url,
            ) => res,
        }
    });

    let da_task = tokio::spawn(async move {
        tokio::select! {
            _ = signal::ctrl_c() => Ok(()),
            res = run_da_task(
                args.node_id,
                args.rollup_node_url,
                args.primary_node_url,
                args.mock_consensus,
                args.mock_rollup,
            ) => res,
        }
    });

    tokio::try_join!(flatten(api_server), flatten(da_task)).unwrap();
}
