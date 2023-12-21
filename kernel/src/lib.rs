// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use fastcrypto::{hash::Hash, traits::ToFromBytes};
use narwhal_config::{Committee, CommitteeBuilder, Epoch};
use narwhal_crypto::{NetworkPublicKey, PublicKey};
use narwhal_types::{
    BatchAPI, Certificate, CertificateAPI, CertificateDigest, ConsensusOutput, Header,
    SequenceNumber,
};
use tezos_smart_rollup_encoding::{
    inbox::{ExternalMessageFrame, InboxMessage, InternalInboxMessage},
    michelson::MichelsonBytes,
};
use tezos_smart_rollup_host::runtime::Runtime;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DsnConfig {
    pub authorities: Vec<PublicKey>,
    pub epoch: Epoch,
}

impl From<DsnConfig> for Committee {
    fn from(value: DsnConfig) -> Self {
        let mut builder = CommitteeBuilder::new(value.epoch);
        for protocol_key in value.authorities {
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
}

fn remember_certificate<Host: Runtime>(
    host: &mut Host,
    sub_dag_index: SequenceNumber,
    digest: &CertificateDigest,
) {
    todo!()
}

fn is_known_certificate<Host: Runtime>(
    host: &Host,
    sub_dag_index: SequenceNumber,
    digest: &CertificateDigest,
) -> bool {
    todo!();
}

fn read_dsn_config<Host: Runtime>(host: &Host) -> DsnConfig {
    todo!()
}

fn write_dsn_config<Host: Runtime>(host: &mut Host, config: DsnConfig) {
    todo!()
}

fn read_last_sub_dag_index<Host: Runtime>(host: &Host) -> SequenceNumber {
    todo!()
}

fn write_last_sub_dag_index<Host: Runtime>(host: &mut Host, sub_dag_index: SequenceNumber) {
    todo!()
}

fn verify_certificate<Host: Runtime>(host: &Host, certificate: &Certificate) -> anyhow::Result<()> {
    todo!()
    // Ensure the header is from the correct epoch.
    // ensure!(
    //     self.epoch() == committee.epoch(),
    //     DagError::InvalidEpoch {
    //         expected: committee.epoch(),
    //         received: self.epoch()
    //     }
    // );

    // // Genesis certificates are always valid.
    // let use_header_v2 = matches!(self.header, Header::V2(_));
    // if self.round() == 0 && Self::genesis(committee, use_header_v2).contains(&self) {
    //     return Ok(Certificate::V2(self));
    // }

    // // Save signature verifications when the header is invalid.
    // self.header.validate(committee, worker_cache)?;

    // let (weight, pks) = self.signed_by(committee);

    // ensure!(
    //     weight >= committee.quorum_threshold(),
    //     DagError::CertificateRequiresQuorum
    // );

    // let verified_cert = self.verify_signature(pks)?;
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

fn store_consensus_output<Host: Runtime>(host: &mut Host, output: &ConsensusOutput) {
    write_last_sub_dag_index(host, output.sub_dag.sub_dag_index);
    for certificate in output.sub_dag.certificates.iter() {
        remember_certificate(host, output.sub_dag.sub_dag_index, &certificate.digest());
    }
}

fn verify_consensus_output<Host: Runtime>(
    host: &Host,
    output: &ConsensusOutput,
) -> anyhow::Result<()> {
    let last_sub_dag_index = read_last_sub_dag_index(host);
    if last_sub_dag_index + 1 != output.sub_dag.sub_dag_index {
        anyhow::bail!("Unexpected sub dag index");
    }

    verify_certificate(host, &output.sub_dag.leader)?;

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

    for certificate in output.sub_dag.certificates.iter() {
        verify_certificate_chain(host, certificate, &known_digests, last_sub_dag_index)?;
    }

    Ok(())
}

fn execute_transaction(payload: &[u8]) {
    todo!()
}

fn process_external_message<Host: Runtime>(host: &mut Host, contents: &[u8]) {
    let output: ConsensusOutput =
        serde_json::from_slice(contents).expect("Failed to parse consensus output");

    if verify_consensus_output(host, &output).is_err() {
        // skip sub dag
        return;
    }

    store_consensus_output(host, &output);

    for batch_list in output.batches {
        for batch in batch_list {
            for tx in batch.transactions() {
                execute_transaction(tx);
            }
        }
    }
}

fn process_internal_message<Host: Runtime>(host: &mut Host, contents: &[u8]) {
    let config: DsnConfig = serde_json::from_slice(contents).expect("Failed to parse DSN config");
    write_dsn_config(host, config);
}

pub fn kernel_run<Host: Runtime>(host: &mut Host) {
    let smart_rollup_address = host.reveal_metadata().address();
    let mut chunked_message: Vec<u8> = Vec::new();
    loop {
        match host.read_input().expect("Failed to read inbox") {
            Some(message) => {
                let bytes = message.as_ref();
                match InboxMessage::<MichelsonBytes>::parse(bytes).expect("Failed to parse message")
                {
                    (_, InboxMessage::External(payload)) => {
                        match ExternalMessageFrame::parse(payload) {
                            Ok(ExternalMessageFrame::Targetted { address, contents })
                                if *address.hash() == smart_rollup_address =>
                            {
                                match contents {
                                    [0u8, chunk @ ..] => chunked_message.extend_from_slice(chunk),
                                    [1u8, chunk @ ..] => {
                                        chunked_message.extend_from_slice(chunk);
                                        process_external_message(host, &chunked_message);
                                        chunked_message.clear();
                                    }
                                    _ => panic!("Unexpected message tag"),
                                }
                            }
                            _ => { /* not for us */ }
                        }
                    }
                    (_, InboxMessage::Internal(msg)) => {
                        match msg {
                            InternalInboxMessage::Transfer(transfer) => {
                                process_internal_message(host, &transfer.payload.0);
                            }
                            _ => { /* system messages */ }
                        }
                    }
                }
            }
            _ => break,
        }
    }
}

tezos_smart_rollup_entrypoint::kernel_entry!(kernel_run);
