// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use digest::Blake2b256;
use serde::{Serialize, Deserialize};
use validator::{validate_certificate_signature, validate_certificate_chain, validate_certificate_batches};

mod conversion;
mod exporter;
mod validator;
mod digest;
mod bls_min_sig;

pub type Transaction = Vec<u8>;
pub type Batch = Vec<Transaction>;
pub type PublicKey = Vec<u8>; // 96 bytes
pub type AggregateSignature = Vec<u8>; // 48 bytes
pub type Digest = [u8; 32];

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RandomnessRound(pub u64);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum SystemMessage {
    DkgMessage(Vec<u8>),
    DkgConfirmation(Vec<u8>),
    RandomnessSignature(RandomnessRound, Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CertificateHeader {
    pub author: u32,
    pub round: u64,
    pub epoch: u64,
    pub created_at: u64,
    pub payload: Vec<(Digest, (u32, u64))>,
    pub system_messages: Vec<SystemMessage>,
    pub parents: BTreeSet<Digest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    header: CertificateHeader,
    signers: Vec<u8>,
    signature: AggregateSignature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreBlock {
    index: u64,
    leader: Certificate,
    certificates: Vec<Certificate>,
    batches: Vec<Vec<Vec<Transaction>>>,
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
            anyhow::bail!("Non-sequential index");
        }

        // TODO: check that leader is actually leader — or is it implied by consensus?
        validate_certificate_signature(&self.leader, config)?;

        let mut digests: BTreeSet::<Digest> = self.certificates
            .iter()
            .map(|cert| cert.digest())
            .collect();

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
