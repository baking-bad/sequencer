// // SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
// //
// // SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use crate::{Certificate, DsnConfig, PreBlockStore, Digest, Batch, digest::Blake2b256, bls_min_sig::aggregate_verify, PublicKey};

pub fn validate_certificate_signature(cert: &Certificate, config: &DsnConfig) -> anyhow::Result<()> {
    if config.epoch != cert.header.epoch {
        anyhow::bail!("Incorrect epoch");
    }

    if cert.signers.iter().any(|x| (*x as usize) >= config.authorities.len()) {
        anyhow::bail!("Unknown authority");
    }

    if cert.signers.len() < config.quorum_threshold() {
        anyhow::bail!("Quorum is not met");
    }

    let digest = cert.digest();
    let keys: Vec<&PublicKey> = cert.signers.iter().map(|i| {
        config.authorities.get(*i as usize).unwrap()
    }).collect();

    aggregate_verify(&cert.signature, digest, keys.as_slice())
}

pub fn validate_certificate_chain(
    cert: &Certificate,
    index: u64,
    store: &impl PreBlockStore,
    neighbors: &BTreeSet<Digest>
) -> anyhow::Result<()> {
    for parent in cert.header.parents.iter() {
        if neighbors.contains(parent) {
            continue;
        }

        match store.get_certificate_index(parent) {
            Some(prev_index) if prev_index + 1 != index => {
                anyhow::bail!("Parent certificate is not from a preceding sub dag")
            },
            None => {
                anyhow::bail!("Parent certificate cannot be not found");
            },
            _ => (),
        }
    }

    Ok(())
}

pub fn validate_certificate_batches(cert: &Certificate, batches: &[Batch]) -> anyhow::Result<()> {
    for (idx, (digest, _)) in cert.header.payload.iter().enumerate() {
        let actual = batches.get(idx).unwrap().digest();
        if &actual != digest {
            anyhow::bail!("Invalid batch content (digest mismatch)");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use narwhal_crypto::traits::ToFromBytes;
    use narwhal_test_utils::{latest_protocol_version, CommitteeFixture};
    use narwhal_types::{VoteAPI, CertificateV2, CertificateAPI};

    use crate::{Certificate, CertificateHeader, DsnConfig};

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
            _ => unreachable!()
        };

        // Convert to pre-block certificate
        let cert = Certificate {
            header: CertificateHeader::default(),
            signers: CertificateAPI::signed_authorities(&narwhal_cert)
                .iter()
                .map(|x| x as u8)
                .collect(),
            signature: narwhal_cert.aggregated_signature().unwrap().0.to_vec(),
        };

        let config = DsnConfig::new(
            0,
            fixture
                .authorities()
                .map(|auth| auth.public_key().as_bytes().to_vec())
                .collect()
        );

        validate_certificate_signature(&cert, &config).unwrap();

    }
}