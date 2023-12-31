use std::{collections::BTreeSet, num::NonZeroUsize};

use narwhal_test_utils::{latest_protocol_version, CommitteeFixture, fixture_batch_with_transactions};
use narwhal_types::{VoteAPI, CertificateV2, CertificateDigest, Header};
use narwhal_utils::protocol_config::ProtocolConfig;
use pre_block::conversion::convert_batch;

#[derive(Default)]
pub struct NoRng {
    counter: u8
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

pub struct NarwalFixture {
    fixture: CommitteeFixture,
    config: ProtocolConfig,
    round: u64,
    index: u64,
    parents: BTreeSet<CertificateDigest>,
}

impl NarwalFixture {
    pub fn new(committee_size: usize) -> Self {
        let config = latest_protocol_version();
        let fixture = CommitteeFixture::builder()
            .rng(NoRng::default())
            .committee_size(NonZeroUsize::new(committee_size).unwrap())
            .build();
        Self { config, fixture, round: 0, index: 0, parents: BTreeSet::new() }
    }

    pub fn certify(&self, header: Header) -> pre_block::Certificate {
        let committee = self.fixture.committee();
        let mut signatures = Vec::new();

        // 3 Signers satisfies the 2F + 1 signed stake requirement
        for authority in self.fixture.authorities().take(3) {
            let vote = authority.vote(&header);
            signatures.push((vote.author(), vote.signature().clone()));
        }

        match CertificateV2::new_unverified(&committee, header, signatures) {
            Ok(narwhal_types::Certificate::V2(cert)) => cert.into(),
            _ => unreachable!()
        }
    }

    fn round(&mut self, num_txs: u32) -> (Vec<Vec<pre_block::Batch>>, Vec<pre_block::Certificate>) {
        let (round, headers) = self.fixture.headers_round(
            self.round,
            &self.parents,
            &self.config
        );

        self.round = round;
        self.parents = headers
            .iter()
            .map(|header| CertificateDigest::new(header.digest().0))
            .collect();

        let batches = headers
            .iter()
            .map(|_| vec![convert_batch(fixture_batch_with_transactions(num_txs, &self.config))])
            .collect();

        let certificates = headers
            .into_iter()
            .map(|header| self.certify(header))
            .collect();

        (batches, certificates)
    }

    fn leader(&self) -> pre_block::Certificate {
        let mut header = self.fixture.header(&self.config);
        match header {
            Header::V2(ref mut h) => {
                h.round = self.round + 1;
                h.parents = self.parents.clone();
            },
            Header::V1(_) => unimplemented!()
        };
        self.certify(header)
    }

    pub fn next_pre_block(&mut self) -> pre_block::PreBlock {
        let (batches_1, certs_1) = self.round(0);
        let (batches_2, certs_2) = self.round(0);
        let leader = self.leader();
        
        let index = self.index;
        self.index += 1;

        pre_block::PreBlock {
            index,
            leader,
            certificates: [certs_1, certs_2].concat(),
            batches: [batches_1, batches_2].concat(),
        }
    }
}
