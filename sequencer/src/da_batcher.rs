// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use log::info;
use pre_block::{PreBlock, Certificate, CertificateHeader};
use pre_block::fixture::NarwhalFixture;
use serde::Serialize;
use tezos_data_encoding::enc::BinWriter;
use tezos_smart_rollup_encoding::{inbox::ExternalMessageFrame, smart_rollup::SmartRollupAddress};
use std::{sync::mpsc, time::Duration};

use crate::rollup_client::RollupClient;

pub const MAX_MESSAGE_SIZE: usize = 2048;
// minus endian tag, smart rollup address, external message tag
pub const MAX_MESSAGE_PAYLOAD_SIZE: usize = 2020;
pub const BATCH_SIZE_SOFT_LIMIT: usize = 100;

pub type DaBatch = Vec<Vec<u8>>;

pub fn batch_encode_to<T: Serialize>(
    value: &T,
    smart_rollup_address: &SmartRollupAddress,
    batch: &mut DaBatch,
) -> anyhow::Result<()> {
    let payload = bcs::to_bytes(&value)?;
    let num_messages = payload.len().div_ceil(MAX_MESSAGE_PAYLOAD_SIZE);

    for (idx, chunk) in payload.chunks(MAX_MESSAGE_PAYLOAD_SIZE).enumerate() {
        let mut contents = Vec::with_capacity(MAX_MESSAGE_SIZE);
        contents.push(if idx == num_messages - 1 { 1u8 } else { 0u8 });
        contents.extend_from_slice(chunk);

        let message = ExternalMessageFrame::Targetted {
            address: smart_rollup_address.clone(),
            contents,
        };

        let mut output = Vec::with_capacity(MAX_MESSAGE_SIZE);
        message.bin_write(&mut output)?;
        assert!(output.len() <= MAX_MESSAGE_SIZE);

        batch.push(output);
    }

    Ok(())
}

fn dummy_pre_block(index: u64) -> PreBlock {
    PreBlock {
        index,
        leader: Certificate {
            header: CertificateHeader::default(),
            signature: vec![],
            signers: vec![],
        },
        certificates: vec![],
        batches: vec![]
    }
}

pub async fn fetch_pre_blocks(
    prev_index: u64,
    pre_blocks_tx: mpsc::Sender<PreBlock>
) -> anyhow::Result<()> {
    let mut index = prev_index;
    let mut fixture = NarwhalFixture::default();

    // let stream = primary_client.get_sub_dag_stream(sub_dag_index);
    // while let Some(pre_block) = stream.next().await {

    loop {
        // let pre_block = dummy_pre_block(index);
        let pre_block = fixture.next_pre_block(1);
        if pre_block.index() == index {
            info!("[DA fetch] received pre-block #{}", index);
            pre_blocks_tx.send(pre_block)?;
            index += 1;

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    //}
}

pub fn is_leader(level: u32, node_id: u8) -> bool {
    if level % 2 == 0 {
        (level / 2) % (node_id as u32) == 0
    } else {
        false
    }
}

pub async fn publish_pre_blocks(
    rollup_client: &RollupClient,
    smart_rollup_address: &SmartRollupAddress,
    node_id: u8,
    pre_blocks_rx: mpsc::Receiver<PreBlock>
) -> anyhow::Result<()> {
    let mut prev_inbox_level = 0;
    info!("[DA Publish] Latest inbox level is {}", prev_inbox_level);
     
    loop {
        let inbox_level = rollup_client.get_inbox_level().await?;
        if inbox_level > prev_inbox_level {
            prev_inbox_level = inbox_level;
            info!("[DA Publish] New inbox level {}", inbox_level);

            if is_leader(inbox_level, node_id) {                
                let mut index = rollup_client.get_next_index().await?;
                let mut batch = DaBatch::new();
                info!("[DA Publish] Next pre-block index {}", index);

                while let Ok(pre_block) = pre_blocks_rx.try_recv() {
                    info!("[DA publish] Encoding pre-block #{}", pre_block.index());
                    if pre_block.index() == index {
                        batch_encode_to(&pre_block, &smart_rollup_address, &mut batch)?;
                        index += 1;
                        if batch.len() > BATCH_SIZE_SOFT_LIMIT {
                            break
                        }
                    } else if pre_block.index() > index {
                        return Err(anyhow::anyhow!("Missing pre-blocks #{0}..{1}", index, pre_block.index()));
                    } else  {
                        info!("[DA publish] Skipping pre-block #{}", pre_block.index());
                    }
                }

                info!("[DA publish] Sending inbox messages");
                rollup_client.inject_batch(batch).await?;
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
