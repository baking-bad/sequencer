use roaring::RoaringBitmap;
use std::collections::HashMap;
use types::{BatchAPI, CertificateAPI, HeaderAPI};

mod exporter {
    tonic::include_proto!("exporter");
}
pub use exporter::{
    exporter_server::{Exporter, ExporterServer},
    system_message, Batch, BatchInfo, Certificate, ExportRequest, Header, Payload,
    RandomnessSignature, SubDag, SystemMessage,
};

impl SubDag {
    pub fn from(
        subdag: &types::CommittedSubDag,
        batches: &HashMap<types::BatchDigest, types::Batch>,
    ) -> Self {
        SubDag {
            id: subdag.sub_dag_index,
            leader: Some(Certificate::from(&subdag.leader)),
            certificates: subdag
                .certificates
                .iter()
                .map(|cert| Certificate::from(cert))
                .collect(),
            payloads: subdag
                .certificates
                .iter()
                .map(|cert| Payload {
                    batches: cert
                        .header()
                        .payload()
                        .iter()
                        .map(|(digest, _)| Batch {
                            transactions: batches
                                .get(digest)
                                .expect("Batches cannot be missed")
                                .transactions()
                                .clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl Certificate {
    pub fn from(cert: &types::Certificate) -> Self {
        match cert {
            types::Certificate::V2(c) => Certificate {
                header: Some(Header::from(&c.header)),
                signature: match &c.signature_verification_state {
                    types::SignatureVerificationState::VerifiedDirectly(bytes)
                    | types::SignatureVerificationState::VerifiedIndirectly(bytes)
                    | types::SignatureVerificationState::Unverified(bytes)
                    | types::SignatureVerificationState::Unsigned(bytes) => Vec::from(bytes.0),
                    types::SignatureVerificationState::Genesis => Vec::new(),
                },
                signers: rb_to_bytes(&c.signed_authorities),
            },
            _ => panic!("CertificateV1 is not expected"),
        }
    }
}

impl Header {
    pub fn from(header: &types::Header) -> Self {
        match header {
            types::Header::V2(h) => Header {
                author: h.author.0.into(),
                round: h.round,
                epoch: h.epoch,
                created_at: h.created_at,
                payload_info: h
                    .payload
                    .iter()
                    .map(|(digest, (worker_id, created_at))| BatchInfo {
                        digest: Vec::from(digest.as_ref()),
                        worker_id: *worker_id,
                        created_at: *created_at,
                    })
                    .collect(),
                system_messages: h
                    .system_messages
                    .iter()
                    .map(|item| match item {
                        types::SystemMessage::DkgConfirmation(bytes) => SystemMessage {
                            message: Some(system_message::Message::DkgConfirmation(bytes.clone())),
                        },
                        types::SystemMessage::DkgMessage(bytes) => SystemMessage {
                            message: Some(system_message::Message::DkgMessage(bytes.clone())),
                        },
                        types::SystemMessage::RandomnessSignature(round, bytes) => SystemMessage {
                            message: Some(system_message::Message::RandomnessSignature(
                                RandomnessSignature {
                                    randomness_round: round.0,
                                    bytes: bytes.clone(),
                                },
                            )),
                        },
                    })
                    .collect(),
                parents: h
                    .parents
                    .iter()
                    .map(|digest| Vec::from(digest.as_ref()))
                    .collect(),
            },
            _ => panic!("HeaderV1 is not expected"),
        }
    }
}

fn rb_to_bytes(rb: &RoaringBitmap) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(rb.serialized_size());
    rb.serialize_into(&mut bytes).unwrap();
    bytes
}
