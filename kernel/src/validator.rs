// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use fastcrypto::{
    hash::{Digest, Hash},
    traits::ToFromBytes,
};
use narwhal_config::{Committee, CommitteeBuilder, Epoch};
use narwhal_crypto::{
    to_intent_message, AggregateSignature, NarwhalAuthorityAggregateSignature, NetworkPublicKey,
    DIGEST_LENGTH,
};
use narwhal_types::{
    Batch, Certificate, CertificateAPI, CertificateDigest, ConsensusOutput, Header, SequenceNumber,
};
use tezos_smart_rollup_host::runtime::Runtime;

use crate::storage::{
    is_known_certificate, read_authorities, read_last_sub_dag_index, remember_certificate,
    write_last_sub_dag_index,
};

fn load_committee<Host: Runtime>(host: &Host, epoch: Epoch) -> Committee {
    let mut builder = CommitteeBuilder::new(epoch);

    let authorities = read_authorities(host, epoch);
    for protocol_key in authorities {
        // We need only epoch and public key for verification.
        // The rest is just filled with dummy values.
        builder = builder.add_authority(
            protocol_key,
            1,
            "127.0.0.1".try_into().unwrap(),
            NetworkPublicKey::from_bytes(&[0u8; 32]).unwrap(),
            String::new(),
        );
    }
    builder.build()
}

fn verify_certificate<Host: Runtime>(
    host: &Host,
    certificate: &Certificate,
    epoch: Epoch,
) -> anyhow::Result<()> {
    if epoch != certificate.epoch() {
        anyhow::bail!("Mismatching epoch");
    }

    // Genesis certificates are always valid
    if certificate.round() == 0 {
        // TODO: verify protocol config
        return Ok(());
    }

    let committee = load_committee(host, epoch);
    let (weight, pks) = certificate.signed_by(&committee);

    if weight < committee.quorum_threshold() {
        anyhow::bail!("Quorum not met");
    }

    let aggregrate_signature_bytes = certificate.aggregated_signature().unwrap();
    let certificate_digest: Digest<DIGEST_LENGTH> = Digest::from(certificate.digest());
    AggregateSignature::try_from(aggregrate_signature_bytes)?
        .verify_secure(&to_intent_message(certificate_digest), &pks[..])?;

    Ok(())
}

fn verify_certificate_chain<Host: Runtime>(
    host: &Host,
    certificate: &Certificate,
    known_digests: &BTreeSet<CertificateDigest>,
    last_sub_dag_index: SequenceNumber,
) -> anyhow::Result<()> {
    let parents = match certificate.header() {
        Header::V2(header) => &header.parents,
        _ => unimplemented!(),
    };

    for parent_digest in parents.iter() {
        if !known_digests.contains(parent_digest) {
            if !is_known_certificate(host, last_sub_dag_index, parent_digest) {
                anyhow::bail!("Failed to verify certificate chain");
            }
        }
    }
    Ok(())
}

fn verify_batches(certificate: &Certificate, batches: &[Batch]) -> anyhow::Result<()> {
    for (index, digest) in certificate
        .header()
        .clone()
        .unwrap_v2()
        .payload
        .keys()
        .enumerate()
    {
        let batch_digest = batches.get(index).unwrap().digest();
        if &batch_digest != digest {
            anyhow::bail!("Unexpected batch digest");
        }
    }
    Ok(())
}

pub fn commit_consensus_output<Host: Runtime>(host: &mut Host, output: &ConsensusOutput) {
    write_last_sub_dag_index(host, output.sub_dag.sub_dag_index);
    for certificate in output.sub_dag.certificates.iter() {
        remember_certificate(host, output.sub_dag.sub_dag_index, &certificate.digest());
    }
}

pub fn verify_consensus_output<Host: Runtime>(
    host: &Host,
    output: &ConsensusOutput,
    epoch: Epoch,
) -> anyhow::Result<()> {
    let last_sub_dag_index = match (read_last_sub_dag_index(host), output.sub_dag.sub_dag_index) {
        (None, 0) => u64::MAX,
        (Some(prev_index), next_index) if prev_index + 1 == next_index => prev_index,
        _ => anyhow::bail!("Unexpected sub dag index"),
    };

    verify_certificate(host, &output.sub_dag.leader, epoch)?;

    let known_digests: BTreeSet<CertificateDigest> = output
        .sub_dag
        .certificates
        .iter()
        .map(|c| c.digest())
        .collect();

    verify_certificate_chain(
        host,
        &output.sub_dag.leader,
        &known_digests,
        last_sub_dag_index,
    )?;

    for (index, certificate) in output.sub_dag.certificates.iter().enumerate() {
        verify_certificate_chain(host, certificate, &known_digests, last_sub_dag_index)?;
        verify_batches(certificate, output.batches.get(index).unwrap())?;
    }

    Ok(())
}
