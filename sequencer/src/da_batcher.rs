use tezos_data_encoding::enc::BinWriter;
use tezos_smart_rollup_encoding::{inbox::ExternalMessageFrame, smart_rollup::SmartRollupAddress};

pub const MAX_MESSAGE_SIZE: usize = 2048;
// minus endian tag, smart rollup address, external message tag
pub const MAX_MESSAGE_PAYLOAD_SIZE: usize = MAX_MESSAGE_SIZE - 22;

pub type DaBatch = Vec<Vec<u8>>;

pub fn make_da_batch(
    payload: &[u8],
    smart_rollup_address: &SmartRollupAddress,
) -> anyhow::Result<DaBatch> {
    if payload.is_empty() {
        return Ok(DaBatch::new());
    }

    let num_messages = payload.len().div_ceil(MAX_MESSAGE_PAYLOAD_SIZE);
    let mut batch = DaBatch::with_capacity(num_messages);

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

    Ok(batch)
}
