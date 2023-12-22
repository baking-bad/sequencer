// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use fastcrypto::hash::{Digest, HashFunction, Keccak256};
use narwhal_types::{BatchAPI, ConsensusOutput};
use tezos_smart_rollup_host::runtime::Runtime;

use crate::storage::{read_head, write_block, write_head, DIGEST_SIZE};

pub fn apply_consensus_output<Host: Runtime>(host: &mut Host, output: ConsensusOutput) {
    let mut block: Vec<Digest<DIGEST_SIZE>> = Vec::new();

    for batch_list in output.batches {
        for batch in batch_list {
            for tx in batch.transactions() {
                let tx_hash = Keccak256::digest(tx);
                block.push(tx_hash);
            }
        }
    }

    let head = read_head(host);
    write_block(host, head + 1, &block);
    write_head(host, head + 1);
}
