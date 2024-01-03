// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use narwhal_exporter::proto as exporter;
use narwhal_types::{CertificateAPI, CertificateV2, HeaderV2};

use crate::{Batch, Certificate, CertificateHeader, PreBlock};

impl From<exporter::Header> for CertificateHeader {
    fn from(header: exporter::Header) -> Self {
        assert!(header.system_messages.is_empty());
        Self {
            author: header.author as u16,
            round: header.round,
            epoch: header.epoch,
            created_at: header.created_at,
            payload: header
                .payload_info
                .into_iter()
                .map(|info| {
                    (
                        info.digest.try_into().unwrap(),
                        (info.worker_id, info.created_at),
                    )
                })
                .collect(),
            system_messages: vec![],
            parents: BTreeSet::from_iter(
                header
                    .parents
                    .into_iter()
                    .map(|digest| digest.try_into().unwrap()),
            ),
        }
    }
}

impl From<exporter::Certificate> for Certificate {
    fn from(cert: exporter::Certificate) -> Self {
        Self {
            header: cert.header.unwrap().into(),
            signers: cert.signers,
            signature: cert.signature,
        }
    }
}

impl From<exporter::SubDag> for PreBlock {
    fn from(sub_dag: exporter::SubDag) -> Self {
        Self {
            batches: sub_dag
                .payloads
                .into_iter()
                .map(|payload| payload.batches)
                .map(|batches| {
                    batches
                        .into_iter()
                        .map(|batch| batch.transactions)
                        .collect()
                })
                .collect(),
            index: sub_dag.id,
            leader: sub_dag.leader.unwrap().into(),
            certificates: sub_dag
                .certificates
                .into_iter()
                .map(|cert| cert.into())
                .collect(),
        }
    }
}

impl From<HeaderV2> for CertificateHeader {
    fn from(narwhal_header: HeaderV2) -> Self {
        CertificateHeader {
            author: narwhal_header.author.0,
            round: narwhal_header.round,
            epoch: narwhal_header.epoch,
            created_at: narwhal_header.created_at,
            payload: narwhal_header
                .payload
                .into_iter()
                .map(|x| (x.0 .0, x.1))
                .collect(),
            system_messages: vec![],
            parents: narwhal_header.parents.into_iter().map(|x| x.0).collect(),
        }
    }
}

impl From<CertificateV2> for Certificate {
    fn from(narwhal_cert: CertificateV2) -> Self {
        let signers = CertificateAPI::signed_authorities(&narwhal_cert)
            .iter()
            .map(|x| x as u8)
            .collect();
        let signature = narwhal_cert.aggregated_signature().unwrap().0.to_vec();
        let narwhal_header = narwhal_cert.header.unwrap_v2();

        Certificate {
            signers,
            signature,
            header: narwhal_header.into(),
        }
    }
}

impl From<narwhal_types::Certificate> for Certificate {
    fn from(value: narwhal_types::Certificate) -> Self {
        match value {
            narwhal_types::Certificate::V2(cert) => cert.into(),
            _ => unreachable!(),
        }
    }
}

pub fn convert_batch(narwhal_batch: narwhal_types::Batch) -> Batch {
    match narwhal_batch {
        narwhal_types::Batch::V2(batch) => batch.transactions,
        _ => unimplemented!(),
    }
}
