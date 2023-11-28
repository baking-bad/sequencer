// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::StoreResult;
use store::rocks::{open_cf, MetricConf};
use store::{reopen, rocks::DBMap, rocks::ReadWriteOptions, Map};
use sui_macros::fail_point;
use types::Header;

pub type ProposerKey = u32;

pub const LAST_PROPOSAL_KEY: ProposerKey = 0;

/// The storage for the proposer
#[derive(Clone)]
pub struct ProposerStore {
    /// Holds the Last Header that was proposed by the Proposer.
    last_proposed: DBMap<ProposerKey, Header>,
}

impl ProposerStore {
    pub fn new(last_proposed: DBMap<ProposerKey, Header>) -> ProposerStore {
        Self { last_proposed }
    }

    pub fn new_for_tests() -> ProposerStore {
        const LAST_PROPOSED_CF: &str = "last_proposed";
        let rocksdb = open_cf(
            tempfile::tempdir().unwrap(),
            None,
            MetricConf::default(),
            &[LAST_PROPOSED_CF],
        )
        .expect("Cannot open database");
        let last_proposed_map = reopen!(&rocksdb, LAST_PROPOSED_CF;<ProposerKey, Header>);
        ProposerStore::new(last_proposed_map)
    }

    /// Inserts a proposed header into the store
    #[allow(clippy::let_and_return)]
    pub fn write_last_proposed(&self, header: &Header) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let result = self.last_proposed.insert(&LAST_PROPOSAL_KEY, header);

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Get the last header
    pub fn get_last_proposed(&self) -> StoreResult<Option<Header>> {
        self.last_proposed.get(&LAST_PROPOSAL_KEY)
    }
}
