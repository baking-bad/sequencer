// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use pre_block::{DsnConfig, PreBlock};
use tezos_crypto_rs::blake2b::digest_256;
use tezos_smart_rollup_encoding::{
    inbox::{ExternalMessageFrame, InboxMessage, InternalInboxMessage},
    michelson::MichelsonBytes,
};
use tezos_smart_rollup_host::runtime::Runtime;

mod storage;

#[cfg(test)]
mod tests;

use storage::{
    read_authorities, read_chunk, read_head, write_authorities, write_block, write_chunk,
    write_head, Store,
};

const LEVELS_PER_EPOCH: u32 = 100;

fn reconstruct_batch(host: &impl Runtime, header: &[u8]) -> anyhow::Result<Vec<u8>> {
    if header.len() % 32 != 0 {
        anyhow::bail!("DA header size must be multiple of 32 (chunk digest size)");
    }

    let mut batch: Vec<Vec<u8>> = Vec::with_capacity(header.len() / 32);
    for hash in header.chunks(32) {
        match read_chunk(host, hash) {
            Some(chunk) => batch.push(chunk),
            None => anyhow::bail!("Missing chunk {}", hex::encode(hash)),
        }
    }

    Ok(batch.concat())
}

fn apply_pre_block<Host: Runtime>(host: &mut Host, pre_block: PreBlock) {
    let mut block: Vec<Vec<u8>> = Vec::new();
    for tx in pre_block.into_transactions() {
        let tx_hash = digest_256(&tx).unwrap();
        block.push(tx_hash);
    }

    let head = read_head(host);
    write_block(host, head + 1, block);
    write_head(host, head + 1);
}

fn process_external_message<Host: Runtime>(host: &mut Host, contents: &[u8], level: u32) {
    let pre_block: PreBlock = match bcs::from_bytes(contents) {
        Ok(value) => value,
        Err(err) => {
            host.write_debug(&format!("Failed to parse pre-block: {}", err));
            return;
        }
    };
    host.write_debug(&format!("Incoming pre-block #{}", pre_block.index()));

    let epoch = 0; // (level % LEVELS_PER_EPOCH) as u64;
    let authorities = match read_authorities(host, epoch) {
        Some(value) => value,
        None => {
            host.write_debug(&format!("Authorities for epoch #{epoch} not initialized"));
            return;
        }
    };
    let config = DsnConfig::new(epoch, authorities);

    {
        let mut store = Store::new(host);
        match pre_block.verify(&config, &store) {
            Ok(()) => {
                pre_block.commit(&mut store);
                host.write_debug(&format!("Handled pre-block #{}", pre_block.index()));
            }
            Err(err) => {
                host.write_debug(&format!("Skipping pre-block: {}", err));
                return;
            }
        }
    }

    apply_pre_block(host, pre_block);
}

fn process_internal_message<Host: Runtime>(host: &mut Host, contents: &[u8]) {
    let config: DsnConfig = match bcs::from_bytes(contents) {
        Ok(value) => value,
        Err(err) => {
            host.write_debug(&format!("Failed to parse DSN config: {}", err));
            return;
        }
    };
    write_authorities(host, config.epoch, &config.authorities);
    host.write_debug(&format!(
        "Authorities for epoch #{0} initialized",
        config.epoch
    ));
}

pub fn kernel_loop<Host: Runtime>(host: &mut Host) {
    host.write_debug(&format!("Kernel loop started"));
    let smart_rollup_address = host.reveal_metadata().address();
    loop {
        match host.read_input().expect("Failed to read inbox") {
            Some(message) => {
                let bytes = message.as_ref();
                match InboxMessage::<MichelsonBytes>::parse(bytes).ok() {
                    Some((_, InboxMessage::External(payload))) => {
                        match ExternalMessageFrame::parse(payload) {
                            Ok(ExternalMessageFrame::Targetted { address, contents })
                                if *address.hash() == smart_rollup_address =>
                            {
                                host.write_debug(&format!("Incoming external message"));
                                match contents {
                                    [0u8, chunk @ ..] => {
                                        let hash = write_chunk(host, chunk);
                                        host.write_debug(&format!(
                                            "Stashing chunk: {}",
                                            hex::encode(&hash)
                                        ));
                                    }
                                    [1u8, header @ ..] => match reconstruct_batch(host, header) {
                                        Ok(contents) => {
                                            process_external_message(
                                                host,
                                                &contents,
                                                message.level,
                                            );
                                        }
                                        Err(err) => {
                                            host.write_debug(&format!("Invalid batch: {}", err));
                                        }
                                    },
                                    _ => panic!("Unexpected message tag"),
                                }
                            }
                            _ => { /* not for us */ }
                        }
                    }
                    Some((_, InboxMessage::Internal(msg))) => {
                        match msg {
                            InternalInboxMessage::Transfer(transfer) => {
                                host.write_debug(&format!(
                                    "Incoming internal message: {}",
                                    hex::encode(&transfer.payload.0)
                                ));
                                process_internal_message(host, &transfer.payload.0);
                            }
                            _ => { /* system messages */ }
                        }
                    }
                    None => { /* not for us */ }
                }
            }
            _ => break,
        }
    }
    host.write_debug(&format!("Kernel loop exited"));
}

tezos_smart_rollup_entrypoint::kernel_entry!(kernel_loop);
