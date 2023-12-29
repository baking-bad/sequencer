use log::info;
use pre_block::{PreBlock, Certificate, CertificateHeader};
use serde::Serialize;
use tezos_data_encoding::enc::BinWriter;
use tezos_smart_rollup_encoding::{inbox::ExternalMessageFrame, smart_rollup::SmartRollupAddress};
use std::{sync::mpsc, time::Duration};

use crate::rollup_client::RollupClient;

pub const MAX_MESSAGE_SIZE: usize = 2048;
// minus endian tag, smart rollup address, external message tag
pub const MAX_MESSAGE_PAYLOAD_SIZE: usize = MAX_MESSAGE_SIZE - 22;
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

        batch.push(output);
    }

    Ok(())
}

// TODO: replace by stream
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

    // let stream = primary_client.get_sub_dag_stream(sub_dag_index);
    // while let Some(pre_block) = stream.next().await {

    loop {
        pre_blocks_tx.send(dummy_pre_block(index))?;
        index += 1;

        tokio::time::sleep(Duration::from_secs(1)).await;
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
     
    loop {
        let inbox_level = rollup_client.get_inbox_level().await?;
        if inbox_level > prev_inbox_level {
            prev_inbox_level = inbox_level;

            if is_leader(inbox_level, node_id) {
                let mut prev_index = rollup_client.get_latest_index().await?;
                let mut batch = DaBatch::new();

                while let Ok(pre_block) = pre_blocks_rx.try_recv() {
                    if pre_block.index() == prev_index + 1 {
                        batch_encode_to(&pre_block, &smart_rollup_address, &mut batch)?;
                        prev_index += 1;
                        if batch.len() > BATCH_SIZE_SOFT_LIMIT {
                            break
                        }
                    } else if pre_block.index() > prev_index + 1 {
                        return Err(anyhow::anyhow!("Missing pre-blocks {0}..{1}", prev_index, pre_block.index()));
                    } else  {
                        info!("[DA publish] skipping pre-block {}", pre_block.index());
                    }
                }

                rollup_client.inject_batch(batch).await?;
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
