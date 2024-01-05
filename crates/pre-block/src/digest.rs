// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use crate::{Batch, Certificate, CertificateHeader, Digest};
use tezos_crypto_rs::blake2b;

pub trait Blake2b256 {
    fn digest(&self) -> Digest;
}

impl Blake2b256 for CertificateHeader {
    fn digest(&self) -> Digest {
        let payload = bcs::to_bytes(&self).expect("Serialization should not fail");
        let digest = blake2b::digest_256(&payload).unwrap();
        digest.try_into().unwrap()
    }
}

impl Blake2b256 for Certificate {
    fn digest(&self) -> Digest {
        self.header.digest()
    }
}

impl Blake2b256 for Batch {
    fn digest(&self) -> Digest {
        let digest = blake2b::digest_all(self.iter(), 32).unwrap();
        digest.try_into().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use fastcrypto::hash::Hash;
    use indexmap::IndexMap;
    use narwhal_config::{AuthorityIdentifier, WorkerId};
    use narwhal_test_utils::latest_protocol_version;
    use narwhal_types::{
        BatchDigest, BatchV2, CertificateDigest, HeaderV2, RandomnessRound, SystemMessage,
        TimestampMs,
    };

    use crate::CertificateHeader;

    use super::Blake2b256;

    #[test]
    fn test_batch_digest_compatibility() {
        let txs = vec![vec![1u8], vec![2u8]];
        let batch = BatchV2::new(txs.clone(), &latest_protocol_version());

        let expected = batch.digest().0;
        let actual = txs.digest();
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_cert_digest_compatibility() {
        let mut parents: BTreeSet<CertificateDigest> = BTreeSet::new();
        parents.insert(CertificateDigest::new([4u8; 32]));
        parents.insert(CertificateDigest::new([5u8; 32]));

        let mut payload: IndexMap<BatchDigest, (WorkerId, TimestampMs)> = IndexMap::new();
        payload.insert(BatchDigest::new([6u8; 32]), (7u32, 8u64));
        payload.insert(BatchDigest::new([9u8; 32]), (10u32, 11u64));

        let header_v2 = HeaderV2::new(
            AuthorityIdentifier(1u16),
            2u64,
            3u64,
            payload,
            vec![
                SystemMessage::DkgConfirmation(vec![12u8]),
                SystemMessage::DkgMessage(vec![13u8]),
                SystemMessage::RandomnessSignature(RandomnessRound(123), vec![14u8]),
            ],
            parents,
        );
        let expected_payload = bcs::to_bytes(&header_v2).unwrap();
        let expected = header_v2.digest().0;

        let header: CertificateHeader = header_v2.into();
        let actual_payload = bcs::to_bytes(&header).unwrap();
        let actual = header.digest();

        pretty_assertions::assert_eq!(hex::encode(expected_payload), hex::encode(actual_payload));
        assert_eq!(expected, actual);
    }
}
