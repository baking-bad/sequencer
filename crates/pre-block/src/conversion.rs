use std::collections::BTreeSet;

use crate::{exporter, PreBlock, Certificate, CertificateHeader, SystemMessage, RandomnessRound};
use crate::exporter::system_message::Message as ExporterMessage;

impl From<exporter::SystemMessage> for SystemMessage {
    fn from(msg: exporter::SystemMessage) -> Self {
        match msg.message.unwrap() {
            ExporterMessage::DkgConfirmation(value) => Self::DkgConfirmation(value),
            ExporterMessage::DkgMessage(value) => Self::DkgConfirmation(value),
            ExporterMessage::RandomnessSignature(value) => Self::RandomnessSignature(RandomnessRound(value.randomness_round), value.bytes),
        }
    }
}

impl From<exporter::Header> for CertificateHeader {
    fn from(header: exporter::Header) -> Self {
        Self {
            author: header.author,
            round: header.round,
            epoch: header.epoch,
            created_at: header.created_at,
            payload: header.payload_info
                .into_iter()
                .map(|info| (info.digest.try_into().unwrap(), (info.worker_id, info.created_at)))
                .collect(),
            system_messages: header.system_messages
                .into_iter()
                .map(|msg| msg.into())
                .collect(),
            parents: BTreeSet::from_iter(
                header
                    .parents
                    .into_iter()
                    .map(|digest| digest.try_into().unwrap())
            )
        }
    }
}

impl From<exporter::Certificate> for Certificate {
    fn from(cert: exporter::Certificate) -> Self {
        Self {
            header: (*cert.header.0.unwrap()).into(),
            signers: cert.signers,
            signature:cert.signature,
        }
    }
}

impl From<exporter::SubDag> for PreBlock {
    fn from(sub_dag: exporter::SubDag) -> Self {
        Self {
            batches: sub_dag.payloads
                .into_iter()
                .map(|payload| payload.batches)
                .map(|batches| batches.into_iter().map(|batch| batch.transactions).collect())
                .collect(),
            index: sub_dag.id,
            leader: (*sub_dag.leader.0.unwrap()).into(),
            certificates: sub_dag.certificates
                .into_iter()
                .map(|cert| cert.into())
                .collect(),
        }
    }
}
