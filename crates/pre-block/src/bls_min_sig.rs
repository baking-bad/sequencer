// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use blst::{min_sig, BLST_ERROR};
use serde::{Serialize, Deserialize};

use crate::{AggregateSignature, PublicKey, Digest};

pub const DST_G1: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash)]
pub struct Intent {
    pub scope: u8,
    pub version: u8,
    pub app_id: u8,
}

impl Intent {
    pub fn narwal_header_digest_v0() -> Self {
        Self {
            scope: 6,
            version: 0,
            app_id: 1,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Hash, Deserialize)]
pub struct IntentMessage<T> {
    pub intent: Intent,
    pub value: T,
}

pub fn aggregate_verify(signature: &AggregateSignature, value: Digest, keys: &[&PublicKey]) -> anyhow::Result<()> {
    let sig = min_sig::Signature::from_bytes(signature.as_slice())
        .expect("Signature cast");

    let pks: Vec<min_sig::PublicKey> = keys
        .iter()
        .map(|key| min_sig::PublicKey::from_bytes(key.as_slice()).expect("PublicKey cast"))
        .collect();

    let message = bcs::to_bytes(&IntentMessage { intent: Intent::narwal_header_digest_v0(), value })
        .expect("Message serialization should not fail");
  
    let res = sig.fast_aggregate_verify(
        true,
        message.as_slice(),
        DST_G1,
        pks.iter().collect::<Vec<_>>().as_slice()
    );

    if res != BLST_ERROR::BLST_SUCCESS {
        anyhow::bail!("Aggregate signature is invalid: {:?}", res);
    }

    Ok(())
}
