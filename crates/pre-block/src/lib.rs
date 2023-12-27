// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use serde::{Serialize, Deserialize};

pub type Transaction = Vec<u8>;
pub type PublicKey = tezos_crypto_rs::hash::PublicKeyBls;
pub type CertificateDigest = [u8; 32];
pub type BatchDigest = [u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreBlock {
    index: u64,
    batches: Vec<Vec<Transaction>>,
}

impl PreBlock {
    pub fn new(index: u64, batches: Vec<Vec<Transaction>>) -> Self {
        Self { index, batches }
    }

    pub fn index(&self) -> u64 {
        self.index
    }

    pub fn into_transactions(self) -> Vec<Transaction> {
        self.batches.into_iter().flatten().collect()
    }

    pub fn is_leader(&self) -> bool {
        true
    }

    pub fn verify(&self, config: &DsnConfig, store: &impl PreBlockStore) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn commit(&self, store: &mut impl PreBlockStore) {
        store.set_index(self.index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DsnConfig {
    pub epoch: u64,
    pub authorities: Vec<PublicKey>,
}

impl DsnConfig {
    pub fn new(epoch: u64, authorities: Vec<PublicKey>) -> Self {
        Self { epoch, authorities }
    }
}

pub trait PreBlockStore {
    fn has_certificate(&self, pre_block_index: u64, digest: &CertificateDigest) -> bool;
    fn mem_certificate(&mut self, pre_block_index: u64, digest: &CertificateDigest);
    fn get_index(&self) -> Option<u64>;
    fn set_index(&mut self, index: u64);
}
