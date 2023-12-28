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
    #[test]
    fn test_certificate_digest_compatibility() {

    }

    #[test]
    fn test_batch_digest_compatibility() {
        
    }
}