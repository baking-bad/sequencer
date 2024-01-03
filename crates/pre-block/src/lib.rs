// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use digest::Blake2b256;
use serde::{Deserialize, Serialize};
use validator::{
    validate_certificate_batches, validate_certificate_chain, validate_certificate_signature,
};

#[cfg(any(test, feature = "conversions"))]
pub mod conversion;

#[cfg(any(test, feature = "conversions"))]
pub mod fixture;

pub mod bls_min_sig;
pub mod digest;
pub mod validator;

pub type Transaction = Vec<u8>;
pub type Batch = Vec<Transaction>;
/// BLS12-381 G2 element — 96 bytes
pub type PublicKey = Vec<u8>;
/// BLS12-381 G1 element — 48 bytes
pub type AggregateSignature = Vec<u8>;
/// Blake2B 256 bit
pub type Digest = [u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CertificateHeader {
    pub author: u16,
    pub round: u64,
    pub epoch: u64,
    pub created_at: u64,
    pub payload: Vec<(Digest, (u32, u64))>,
    pub system_messages: Vec<()>, // not used
    pub parents: BTreeSet<Digest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    pub header: CertificateHeader,
    pub signers: Vec<u8>,
    pub signature: AggregateSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreBlock {
    pub index: u64,
    pub leader: Certificate,
    pub certificates: Vec<Certificate>,
    pub batches: Vec<Vec<Batch>>,
}

impl PreBlock {
    pub fn index(&self) -> u64 {
        self.index
    }

    pub fn into_transactions(self) -> Vec<Transaction> {
        self.batches.into_iter().flatten().flatten().collect()
    }

    pub fn verify(&self, config: &DsnConfig, store: &impl PreBlockStore) -> anyhow::Result<()> {
        let prev_index = store.get_latest_index();
        if prev_index.map(|x| x + 1).unwrap_or(0) != self.index {
            // NOTE that index is not enforced by any signature, so technically one can craft a
            // pre-block with a mismatched (sub dag) index.
            // The validation would fail either way, because of the parents check.
            anyhow::bail!("Non-sequential index");
        }

        // TODO: check that leader is actually leader — or is it implied by consensus?
        validate_certificate_signature(&self.leader, config)?;

        let digests: BTreeSet<Digest> =
            self.certificates.iter().map(|cert| cert.digest()).collect();

        validate_certificate_chain(&self.leader, self.index, store, &digests)?;

        for (idx, cert) in self.certificates.iter().enumerate() {
            validate_certificate_chain(cert, self.index, store, &digests)?;
            validate_certificate_batches(cert, self.batches.get(idx).unwrap())?;
        }

        Ok(())
    }

    pub fn commit(&self, store: &mut impl PreBlockStore) {
        for cert in self.certificates.iter() {
            // TODO: cache digests
            store.set_certificate_index(&cert.digest(), self.index);
        }
        store.set_latest_index(self.index)
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

    pub fn quorum_threshold(&self) -> usize {
        self.authorities.len() * 2 / 3 + 1
    }
}

pub trait PreBlockStore {
    fn get_certificate_index(&self, digest: &Digest) -> Option<u64>;
    fn set_certificate_index(&mut self, digest: &Digest, index: u64);
    fn get_latest_index(&self) -> Option<u64>;
    fn set_latest_index(&mut self, index: u64);
}
