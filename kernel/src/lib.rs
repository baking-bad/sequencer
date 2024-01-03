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

use storage::{read_authorities, read_head, write_authorities, write_block, write_head, Store};

const LEVELS_PER_EPOCH: u32 = 100;

pub fn apply_pre_block<Host: Runtime>(host: &mut Host, pre_block: PreBlock) {
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
    let pre_block: PreBlock = bcs::from_bytes(contents).expect("Failed to parse consensus output");

    let epoch = 0; // (level % LEVELS_PER_EPOCH) as u64;
    let authorities = read_authorities(host, epoch);
    let config = DsnConfig::new(epoch, authorities);

    {
        let mut store = Store::new(host);
        match pre_block.verify(&config, &store) {
            Ok(()) => {
                pre_block.commit(&mut store);
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
    let config: DsnConfig = bcs::from_bytes(contents).expect("Failed to parse authorities");
    write_authorities(host, config.epoch, &config.authorities);
}

pub fn kernel_loop<Host: Runtime>(host: &mut Host) {
    let smart_rollup_address = host.reveal_metadata().address();
    let mut chunked_message: Vec<u8> = Vec::new();
    loop {
        match host.read_input().expect("Failed to read inbox") {
            Some(message) => {
                let bytes = message.as_ref();
                match InboxMessage::<MichelsonBytes>::parse(bytes).expect("Failed to parse message")
                {
                    (_, InboxMessage::External(payload)) => {
                        match ExternalMessageFrame::parse(payload) {
                            Ok(ExternalMessageFrame::Targetted { address, contents })
                                if *address.hash() == smart_rollup_address =>
                            {
                                match contents {
                                    [0u8, chunk @ ..] => chunked_message.extend_from_slice(chunk),
                                    [1u8, chunk @ ..] => {
                                        chunked_message.extend_from_slice(chunk);
                                        process_external_message(
                                            host,
                                            &chunked_message,
                                            message.level,
                                        );
                                        chunked_message.clear();
                                    }
                                    _ => panic!("Unexpected message tag"),
                                }
                            }
                            _ => { /* not for us */ }
                        }
                    }
                    (_, InboxMessage::Internal(msg)) => {
                        match msg {
                            InternalInboxMessage::Transfer(transfer) => {
                                process_internal_message(host, &transfer.payload.0);
                            }
                            _ => { /* system messages */ }
                        }
                    }
                }
            }
            _ => break,
        }
    }
}

tezos_smart_rollup_entrypoint::kernel_entry!(kernel_loop);
