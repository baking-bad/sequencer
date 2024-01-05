// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use std::collections::BTreeSet;

use narwhal_types::{CertificateAPI, CertificateV2, HeaderV2};

use crate::{Batch, Certificate, CertificateHeader, PreBlock};

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
