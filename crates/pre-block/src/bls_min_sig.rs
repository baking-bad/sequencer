use blst::{min_sig, BLST_ERROR};
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};

use crate::{AggregateSignature, PublicKey, Digest};

pub const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_";

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum AppId {
    Sui = 0,
    Narwhal = 1,
}

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum IntentVersion {
    V0 = 0,
}

#[derive(Serialize_repr, Deserialize_repr, Copy, Clone, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum IntentScope {
    TransactionData = 0,         // Used for a user signature on a transaction data.
    TransactionEffects = 1,      // Used for an authority signature on transaction effects.
    CheckpointSummary = 2,       // Used for an authority signature on a checkpoint summary.
    PersonalMessage = 3,         // Used for a user signature on a personal message.
    SenderSignedTransaction = 4, // Used for an authority signature on a user signed transaction.
    ProofOfPossession = 5, // Used as a signature representing an authority's proof of possession of its authority protocol key.
    HeaderDigest = 6,      // Used for narwhal authority signature on header digest.
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash)]
pub struct Intent {
    pub scope: IntentScope,
    pub version: IntentVersion,
    pub app_id: AppId,
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

    let intent = Intent {
        scope: IntentScope::HeaderDigest,
        version: IntentVersion::V0,
        app_id: AppId::Narwhal,
    };

    let message = bcs::to_bytes(&IntentMessage { intent, value })
        .expect("Message serialization should not fail");

    let res = sig.fast_aggregate_verify(
        true,
        message.as_slice(),
        DST_G2,
        pks.iter().collect::<Vec<_>>().as_slice()
    );

    if res != BLST_ERROR::BLST_SUCCESS {
        anyhow::bail!("Aggregated signature is invalid: {:?}", res);
    }

    Ok(())
}
