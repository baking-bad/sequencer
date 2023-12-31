// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use tezos_crypto_rs::blake2b;
use crate::{Certificate, Digest, Batch};

pub trait Blake2b256 {
    fn digest(&self) -> Digest;
}

impl Blake2b256 for Certificate {
    fn digest(&self) -> Digest {
        let payload = bcs::to_bytes(&self.header).expect("Serialization should not fail");
        let digest = blake2b::digest_256(&payload).unwrap();
        digest.try_into().unwrap()
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
    use narwhal_test_utils::latest_protocol_version;
    use narwhal_types::BatchV2;
    use fastcrypto::hash::Hash;

    use super::Blake2b256;

    #[test]
    fn test_batch_digest_compatibility() {
        let txs = vec![vec![1u8]];
        let batch = BatchV2::new(txs.clone(), &latest_protocol_version());

        let expected = batch.digest().0;
        let actual = txs.digest();
        assert_eq!(expected, actual);
    }
}
