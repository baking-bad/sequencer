// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use narwhal_config::{Committee, Import};
use pre_block::fixture::{NarwhalFixture, SimpleStore};
use pre_block::{PreBlock, PublicKey, DsnConfig};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use log::info;

pub async fn generate_pre_blocks(
    prev_index: u64,
    pre_blocks_tx: mpsc::Sender<PreBlock>,
) -> anyhow::Result<()> {
    let mut index = prev_index;
    let mut fixture = NarwhalFixture::default();

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
