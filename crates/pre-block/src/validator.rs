// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use crate::{
    bls_min_sig::aggregate_verify, digest::Blake2b256, Batch, Certificate, Digest, DsnConfig,
    PreBlockStore, PublicKey,
};

pub fn validate_certificate_signature(
    cert: &Certificate,
    config: &DsnConfig,
) -> anyhow::Result<()> {
    if config.epoch != cert.header.epoch {
        anyhow::bail!("Incorrect epoch");
    }

    if cert
        .signers
        .iter()
        .any(|x| (*x as usize) >= config.authorities.len())
    {
        anyhow::bail!("Unknown authority");
    }

    if cert.signers.len() < config.quorum_threshold() {
        anyhow::bail!("Quorum is not met");
    }

    let digest = cert.digest();
    let keys: Vec<&PublicKey> = cert
        .signers
        .iter()
        .map(|i| config.authorities.get(*i as usize).unwrap())
        .collect();

    aggregate_verify(&cert.signature, digest, keys.as_slice())
}

pub fn validate_certificate_chain(
    cert: &Certificate,
    index: u64,
    store: &impl PreBlockStore,
    neighbors: &BTreeSet<Digest>,
) -> anyhow::Result<()> {
    // We need to ensure the sub dag is:
    //  1) Not overlapping with the previous one
    //  2) Not partially withdrawn
    //
    // In order to do that we need to check
    // that every parent certificate is either:
    //  1) From this sub dag
    //  2) From a known sub dag (previous one)
    for parent in cert.header.parents.iter() {
        if neighbors.contains(parent) {
            continue;
        }

        match store.get_certificate_index(parent) {
            Some(prev_index) if prev_index + 1 != index => {
                anyhow::bail!("Parent certificate is not from a preceding sub dag {}", hex::encode(parent))
            }
            None => {
                anyhow::bail!("Parent certificate cannot be not found {}", hex::encode(parent));
            }
            _ => (),
        }
    }

    Ok(())
}

pub fn validate_certificate_batches(cert: &Certificate, batches: &[Batch]) -> anyhow::Result<()> {
    let digests: BTreeSet<&Digest> = cert.header.payload.iter().map(|x| &x.0).collect();

    for (i, batch) in batches.iter().enumerate() {
        let digest = batch.digest();
        if !digests.contains(&digest) {
            anyhow::bail!(
                "Invalid batch content (digest mismatch), idx = {}, digest = {}",
                i,
                hex::encode(&digest)
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use narwhal_crypto::traits::ToFromBytes;
    use narwhal_test_utils::{latest_protocol_version, CommitteeFixture};
    use narwhal_types::{CertificateV2, VoteAPI};

    use crate::{
        digest::Blake2b256,
        fixture::{NarwhalFixture, SimpleStore},
        Certificate, DsnConfig,
    };

    use super::validate_certificate_signature;

    #[test]
    fn test_aggregate_signature_compatibility() {
        let cert_v2_config = latest_protocol_version();
        let fixture = CommitteeFixture::builder().build();
        let committee = fixture.committee();
        let header = fixture.header(&cert_v2_config);

        let mut signatures = Vec::new();

        // 3 Signers satisfies the 2F + 1 signed stake requirement
        for authority in fixture.authorities().take(3) {
            let vote = authority.vote(&header);
            signatures.push((vote.author(), vote.signature().clone()));
        }

        let narwhal_cert = match CertificateV2::new_unverified(&committee, header, signatures) {
            Ok(narwhal_types::Certificate::V2(cert)) => cert,
            _ => unreachable!(),
        };
        let digest = narwhal_cert.header.digest();

        // Make sure the original cert is valid
        narwhal_cert
            .clone()
            .verify(&committee, &fixture.worker_cache())
            .expect("Valid certificate");

        // Convert certificate
        let cert: Certificate = narwhal_cert.into();

        // Make sure digests are the same
        assert_eq!(cert.digest(), digest.0);

        let config = DsnConfig::new(
            0,
            fixture
                .authorities()
                .map(|auth| auth.public_key().as_bytes().to_vec())
                .collect(),
        );

        validate_certificate_signature(&cert, &config).unwrap();
    }

    #[test]
    fn test_pre_block_verification() {
        let mut fixture = NarwhalFixture::default();
        let mut store = SimpleStore::default();

        let pre_block = fixture.next_pre_block(1);
        let config = DsnConfig {
            epoch: 0,
            authorities: fixture.authorities(),
        };

        pre_block
            .verify(&config, &mut store)
            .expect("Failed to verify");
    }
}
