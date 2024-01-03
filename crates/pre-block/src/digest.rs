// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use tezos_crypto_rs::blake2b;
use crate::{Certificate, Digest, Batch, CertificateHeader};

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

    use narwhal_test_utils::latest_protocol_version;
    use narwhal_types::{BatchV2, HeaderV2, CertificateDigest, BatchDigest, TimestampMs};
    use narwhal_config::{AuthorityIdentifier, WorkerId};
    use fastcrypto::hash::Hash;
    use indexmap::IndexMap;

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
            vec![],
            parents
        );
        let expected = header_v2.digest().0;

        let header: CertificateHeader = header_v2.into();
        let actual = header.digest();

        assert_eq!(expected, actual);
        
    }
}
