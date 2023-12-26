// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use narwhal_config::Epoch;
use narwhal_crypto::PublicKey;
use narwhal_types::ConsensusOutput;
use tezos_smart_rollup_encoding::{
    inbox::{ExternalMessageFrame, InboxMessage, InternalInboxMessage},
    michelson::MichelsonBytes,
};
use tezos_smart_rollup_host::runtime::Runtime;

mod executor;
mod storage;
mod validator;

use executor::apply_consensus_output;
use storage::write_authorities;
use validator::{commit_consensus_output, verify_consensus_output};

const LEVELS_PER_EPOCH: u32 = 100;

fn process_external_message<Host: Runtime>(host: &mut Host, contents: &[u8], level: u32, timestamp: u64) {
    let output: ConsensusOutput =
        serde_json::from_slice(contents).expect("Failed to parse consensus output");

    let epoch = (level % LEVELS_PER_EPOCH) as Epoch;

    if verify_consensus_output(host, &output, epoch).is_err() {
        // Skip sub dag
        return;
    }

    // Regardless of the execution result we mark this sub dag as processed and advance the timestamp
    commit_consensus_output(host, &output, timestamp);

    // Process transaction batches
    apply_consensus_output(host, output);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DsnConfig {
    pub epoch: Epoch,
    pub authorities: Vec<PublicKey>,
}

fn process_internal_message<Host: Runtime>(host: &mut Host, contents: &[u8]) {
    let config: DsnConfig = serde_json::from_slice(contents).expect("Failed to parse authorities");
    write_authorities(host, config.epoch, &config.authorities);
}

pub fn kernel_run<Host: Runtime>(host: &mut Host) {
    let smart_rollup_address = host.reveal_metadata().address();
    let mut chunked_message: Vec<u8> = Vec::new();
    let mut timestamp: u64 = 0;
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
                                            timestamp,
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
                            InternalInboxMessage::InfoPerLevel(info) => {
                                timestamp = info.predecessor_timestamp.as_u64();
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

tezos_smart_rollup_entrypoint::kernel_entry!(kernel_run);
