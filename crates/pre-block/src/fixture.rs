use std::collections::HashMap;
use std::{collections::BTreeSet, num::NonZeroUsize};

use narwhal_test_utils::{latest_protocol_version, CommitteeFixture};
use narwhal_types::{CertificateDigest, CertificateV2, Header, HeaderV2Builder, VoteAPI};
use narwhal_utils::protocol_config::ProtocolConfig;

use crate::{Batch, Certificate, Digest, PreBlock, PreBlockStore, PublicKey};

pub const COMMITTEE_SIZE: usize = 4;

#[derive(Default)]
pub struct NoRng {
    counter: u8,
}

impl rand::RngCore for NoRng {
    fn next_u32(&mut self) -> u32 {
        self.counter += 1;
        self.counter as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.counter += 1;
        self.counter as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.counter += 1;
        dest.fill(self.counter)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.counter += 1;
        Ok(dest.fill(self.counter))
    }
}

impl rand::CryptoRng for NoRng {}

pub struct NarwhalFixture {
    fixture: CommitteeFixture,
    config: ProtocolConfig,
    round: u64,
    index: u64,
    parents: BTreeSet<CertificateDigest>,
}

impl Default for NarwhalFixture {
    fn default() -> Self {
        Self::new(COMMITTEE_SIZE)
    }
}

#[derive(Debug, Default)]
pub struct SimpleStore {
    pub latest_index: Option<u64>,
    pub certificate_indexes: HashMap<Digest, u64>,
}

impl PreBlockStore for SimpleStore {
    fn get_certificate_index(&self, digest: &Digest) -> Option<u64> {
        self.certificate_indexes.get(digest).map(|x| *x)
    }

    fn set_certificate_index(&mut self, digest: &Digest, index: u64) {
        self.certificate_indexes.insert(digest.clone(), index);
    }

    fn get_latest_index(&self) -> Option<u64> {
        self.latest_index.clone()
    }

    fn set_latest_index(&mut self, index: u64) {
        self.latest_index = Some(index)
    }
}

impl NarwhalFixture {
    pub fn new(committee_size: usize) -> Self {
        let config = latest_protocol_version();
        let fixture = CommitteeFixture::builder()
            .rng(NoRng::default())
            .committee_size(NonZeroUsize::new(committee_size).unwrap())
            .build();
        Self {
            config,
            fixture,
            round: 0,
            index: 0,
            parents: BTreeSet::new(),
        }
    }

    pub fn authorities(&self) -> Vec<PublicKey> {
        self.fixture
            .authorities()
            .map(|auth| auth.public_key().as_ref().to_vec())
            .collect()
    }

    pub fn certify(&self, header: Header) -> Certificate {
        let committee = self.fixture.committee();
        let mut signatures = Vec::new();

        let num_signers = 2 * (committee.size() - 1) / 3 + 1;
        for authority in self.fixture.authorities().take(num_signers) {
            let vote = authority.vote(&header);
            signatures.push((vote.author(), vote.signature().clone()));
        }

        match CertificateV2::new_unverified(&committee, header, signatures) {
            Ok(narwhal_types::Certificate::V2(cert)) => cert.into(),
            Ok(_) => unreachable!(),
            Err(err) => panic!("Failed to create cert: {}", err),
        }
    }

    fn round(&mut self, num_txs: u32) -> (Vec<Vec<Batch>>, Vec<Certificate>) {
        let mut headers: Vec<Header> = Vec::new();
        let mut batches: Vec<Vec<Batch>> = Vec::new();

        for authority in self.fixture.authorities() {
            let txs: Batch = (0..num_txs)
                .into_iter()
                .map(|i| i.to_be_bytes().to_vec())
                .collect();

            let builder = HeaderV2Builder::default();
            let header = builder
                .author(authority.id())
                .round(self.round)
                .epoch(0)
                .parents(self.parents.clone())
                .with_payload_batch(narwhal_types::Batch::new(txs.clone(), &self.config), 0, 0)
                .build()
                .unwrap();

            headers.push(header.into());
            batches.push(vec![txs]);
        }

        self.round += 1;
        self.parents = headers
            .iter()
            .map(|header| CertificateDigest::new(header.digest().0))
            .collect();

        let certificates = headers
            .into_iter()
            .map(|header| self.certify(header))
            .collect();

        (batches, certificates)
    }

    fn leader(&self) -> Certificate {
        let (_, mut headers, _) =
            self.fixture
                .headers_round(self.round, &self.parents, &self.config, 0);

        let idx = (self.round as usize) % headers.len();
        self.certify(headers.remove(idx))
    }

    pub fn next_pre_block(&mut self, num_txs: u32) -> PreBlock {
        let (batches_1, certs_1) = self.round(num_txs);
        let (batches_2, certs_2) = self.round(num_txs);
        let leader = self.leader();

        let index = self.index;
        self.index += 1;

        PreBlock {
            index,
            leader,
            certificates: [certs_1, certs_2].concat(),
            batches: [batches_1, batches_2].concat(),
        }
    }
}
