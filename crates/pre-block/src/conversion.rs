// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use crate::{exporter, PreBlock, Certificate, CertificateHeader};

impl From<exporter::Header> for CertificateHeader {
    fn from(header: exporter::Header) -> Self {
        assert!(header.system_messages.is_empty());
        Self {
            author: header.author as u16,
            round: header.round,
            epoch: header.epoch,
            created_at: header.created_at,
            payload: header.payload_info
                .into_iter()
                .map(|info| (info.digest.try_into().unwrap(), (info.worker_id, info.created_at)))
                .collect(),
            system_messages: vec![],
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
