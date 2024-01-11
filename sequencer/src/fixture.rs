// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use narwhal_config::{Committee, Import};
use pre_block::fixture::{NarwhalFixture, SimpleStore};
use pre_block::{PreBlock, PublicKey, DsnConfig};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use log::{info, error};

use crate::consensus_client::PrimaryClient;
use crate::da_batcher::publish_pre_blocks;
use crate::rollup_client::RollupClient;

pub async fn generate_pre_blocks(
    prev_index: u64,
    pre_blocks_tx: mpsc::Sender<PreBlock>,
) -> anyhow::Result<()> {
    let mut index = prev_index;
    let mut fixture = NarwhalFixture::new(7);

    loop {
        let pre_block = fixture.next_pre_block(1);
        if pre_block.index() == index {
            info!("[DA fetch] received pre-block #{}", index);
            pre_blocks_tx.send(pre_block)?;
            index += 1;

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

pub async fn verify_pre_blocks(
    pre_blocks_rx: mpsc::Receiver<PreBlock>,
) -> anyhow::Result<()> {
    let mut committee_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    committee_path.push("../launcher/defaults/committee.json");

    let mut committee = Committee::import(committee_path.as_os_str().to_str().unwrap())?;
    committee.load();

    let authorities: Vec<PublicKey> = committee
        .authorities()
        .map(|auth| auth.protocol_key_bytes().0.to_vec())
        .collect();

    let config = DsnConfig {
        epoch: 0,
        authorities
    };

    let mut store = SimpleStore {
        latest_index: Some(0),
        certificate_indexes: config
            .genesis()
            .into_iter()
            .map(|digest| (digest, 0u64))
            .collect()
    };

    loop {
        while let Ok(pre_block) = pre_blocks_rx.try_recv() {
            info!("[DA verifier] Pending #{}", pre_block.index());
            pre_block.verify(&config, &mut store)?;
            pre_block.commit(&mut store);
            info!("[DA verifier] OK #{}", pre_block.index());
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn run_da_task_with_mocked_consensus(
    node_id: u8,
    rollup_node_url: String,
) -> anyhow::Result<()> {
    info!("[DA task] Starting...");

    let rollup_client = RollupClient::new(rollup_node_url.clone());
    let smart_rollup_address = rollup_client.connect().await?;

    loop {
        let from_id = rollup_client.get_next_index().await?;
        let (tx, rx) = mpsc::channel();
        info!("[DA task] Starting from index #{}", from_id);

        tokio::select! {
            res = generate_pre_blocks(from_id - 1, tx) => {
                if let Err(err) = res {
                    error!("[DA generate] Failed with: {}", err);
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

pub async fn run_da_task_with_mocked_rollup(
    primary_node_url: String,
) -> anyhow::Result<()> {
    info!("[DA task] Starting...");

    let mut primary_client = PrimaryClient::new(primary_node_url);

    loop {
        let from_id = 1;
        let (tx, rx) = mpsc::channel();
        info!("[DA task] Starting from index #{}", from_id);

        tokio::select! {
            res = primary_client.subscribe_pre_blocks(from_id - 1, tx) => {
                if let Err(err) = res {
                    error!("[DA fetch] Failed with: {}", err);
                }
            },
            res = verify_pre_blocks(rx) => {
                if let Err(err) = res {
                    error!("[DA verify] Failed with: {}", err);
                }
            },
        };

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
