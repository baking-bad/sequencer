// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{MultiSigPublicKey, ThresholdUnit, WeightUnit};
use crate::{
    base_types::SuiAddress,
    crypto::{
        get_key_pair, get_key_pair_from_rng, DefaultHash, Ed25519SuiSignature, PublicKey,
        Signature, SuiKeyPair, SuiSignatureInner,
    },
    multisig::{as_indices, MultiSig, MAX_SIGNER_IN_MULTISIG},
    multisig_legacy::{bitmap_to_u16, MultiSigLegacy, MultiSigPublicKeyLegacy},
    signature::{AuthenticatorTrait, GenericSignature, VerifyParams},
    utils::keys,
};
use fastcrypto::{
    ed25519::{Ed25519KeyPair, Ed25519PrivateKey},
    encoding::{Base64, Encoding},
    hash::HashFunction,
    secp256k1::{Secp256k1KeyPair, Secp256k1PrivateKey},
    traits::ToFromBytes,
};
use once_cell::sync::OnceCell;
use rand::{rngs::StdRng, SeedableRng};
use roaring::RoaringBitmap;
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use std::str::FromStr;

#[test]
fn multisig_scenarios() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let addr = SuiAddress::from(&multisig_pk);
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);
    let sig3 = Signature::new_secure(&msg, &keys[2]);

    // Any 2 of 3 signatures verifies ok.
    let multi_sig1 =
        MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk.clone()).unwrap();
    assert!(multi_sig1
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_ok());

    let multi_sig2 =
        MultiSig::combine(vec![sig1.clone(), sig3.clone()], multisig_pk.clone()).unwrap();
    assert!(multi_sig2
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_ok());

    let multi_sig3 =
        MultiSig::combine(vec![sig2.clone(), sig3.clone()], multisig_pk.clone()).unwrap();
    assert!(multi_sig3
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_ok());

    // 1 of 3 signature verify fails.
    let multi_sig4 = MultiSig::combine(vec![sig2.clone()], multisig_pk).unwrap();
    assert!(multi_sig4
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());

    // Incorrect address fails.
    let kp4: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair().1);
    let pk4 = kp4.public();
    let multisig_pk_1 = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone(), pk4],
        vec![1, 1, 1, 1],
        1,
    )
    .unwrap();
    let multisig5 = MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk_1).unwrap();
    assert!(multisig5
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());

    // Create a MultiSig pubkey of pk1 (weight = 1), pk2 (weight = 2), pk3 (weight = 3), threshold 3.
    let multisig_pk_2 = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 2, 3],
        3,
    )
    .unwrap();
    let multisig_pk_legacy_2 = MultiSigPublicKeyLegacy::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 2, 3],
        3,
    )
    .unwrap();

    // Address parsing remain the same.
    let addr_2 = SuiAddress::from(&multisig_pk_2);
    let addr_legacy_2 = SuiAddress::from(&multisig_pk_legacy_2);
    assert_eq!(addr_2, addr_legacy_2);

    // sig1 and sig2 (3 of 6) verifies ok.
    let multi_sig_6 =
        MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_6
        .verify_authenticator(&msg, addr_2, None, &VerifyParams::default())
        .is_ok());

    // providing the same sig twice fails.
    assert!(MultiSig::combine(vec![sig1.clone(), sig1.clone()], multisig_pk_2.clone()).is_err());

    // Change position for sig2 and sig1 is not ok with plain bitmap.
    let multi_sig_7 =
        MultiSig::combine(vec![sig2.clone(), sig1.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_7
        .verify_authenticator(&msg, addr_2, None, &VerifyParams::default())
        .is_err());

    // Change position for sig2 and sig1 is not ok with legacy using roaring bitmap.
    let multi_sig_legacy_7 =
        MultiSigLegacy::combine(vec![sig2.clone(), sig1.clone()], multisig_pk_legacy_2).unwrap();
    assert!(multi_sig_legacy_7
        .verify_authenticator(&msg, addr_2, None, &VerifyParams::default())
        .is_err());

    // sig3 itself (3 of 6) verifies ok.
    let multi_sig_8 = MultiSig::combine(vec![sig3.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_8
        .verify_authenticator(&msg, addr_2, None, &VerifyParams::default())
        .is_ok());

    // sig2 itself (2 of 6) verifies fail.
    let multi_sig_9 = MultiSig::combine(vec![sig2.clone()], multisig_pk_2.clone()).unwrap();
    assert!(multi_sig_9
        .verify_authenticator(&msg, addr_2, None, &VerifyParams::default())
        .is_err());

    // A bad sig in the multisig fails, even though sig2 and sig3 verifies and weights meets threshold.
    let bad_sig = Signature::new_secure(
        &IntentMessage::new(
            Intent::sui_transaction(),
            PersonalMessage {
                message: "Bad message".as_bytes().to_vec(),
            },
        ),
        &keys[0],
    );
    let multi_sig_9 = MultiSig::combine(vec![bad_sig, sig2, sig3], multisig_pk_2).unwrap();
    assert!(multi_sig_9
        .verify_authenticator(&msg, addr_2, None, &VerifyParams::default())
        .is_err());

    // Wrong bitmap verifies fail.
    let multi_sig_10 = MultiSig {
        sigs: vec![sig1.to_compressed().unwrap()], // sig1 has index 0
        bitmap: 1,
        multisig_pk: MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![1, 2, 3], 3).unwrap(),
        bytes: OnceCell::new(),
    };
    assert!(multi_sig_10
        .verify_authenticator(&msg, addr_2, None, &VerifyParams::default())
        .is_err());
}

#[test]
fn test_combine_sigs() {
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair().1);
    let kp3: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair().1);

    let pk1 = kp1.public();
    let pk2 = kp2.public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2], vec![1, 1], 2).unwrap();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &kp1);
    let sig2 = Signature::new_secure(&msg, &kp2);
    let sig3 = Signature::new_secure(&msg, &kp3);

    // MultiSigPublicKey contains only 2 public key but 3 signatures are passed, fails to combine.
    assert!(MultiSig::combine(vec![sig1, sig2, sig3], multisig_pk.clone()).is_err());

    // Cannot create malformed MultiSig.
    assert!(MultiSig::combine(vec![], multisig_pk).is_err());
}
#[test]
fn test_serde_roundtrip() {
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    for kp in keys() {
        let pk = kp.public();
        let multisig_pk = MultiSigPublicKey::new(vec![pk], vec![1], 1).unwrap();
        let sig = Signature::new_secure(&msg, &kp);
        let multisig = MultiSig::combine(vec![sig], multisig_pk).unwrap();
        let plain_bytes = bcs::to_bytes(&multisig).unwrap();

        let generic_sig = GenericSignature::MultiSig(multisig);
        let generic_sig_bytes = generic_sig.as_bytes();
        let generic_sig_roundtrip = GenericSignature::from_bytes(generic_sig_bytes).unwrap();
        assert_eq!(generic_sig, generic_sig_roundtrip);

        // A MultiSig flag 0x03 is appended before the bcs serialized bytes.
        assert_eq!(plain_bytes.len() + 1, generic_sig_bytes.len());
        assert_eq!(generic_sig_bytes.first().unwrap(), &0x03);
    }

    // Malformed multisig cannot be deserialized
    let multisig_pk = MultiSigPublicKey {
        pk_map: vec![(keys()[0].public(), 1)],
        threshold: 1,
    };
    let multisig = MultiSig {
        sigs: vec![], // No sigs
        bitmap: 0,
        multisig_pk,
        bytes: OnceCell::new(),
    };

    let generic_sig = GenericSignature::MultiSig(multisig);
    let generic_sig_bytes = generic_sig.as_bytes();
    assert!(GenericSignature::from_bytes(generic_sig_bytes).is_err());

    // Malformed multisig_pk cannot be deserialized
    let multisig_pk_1 = MultiSigPublicKey {
        pk_map: vec![],
        threshold: 0,
    };

    let multisig_1 = MultiSig {
        sigs: vec![],
        bitmap: 0,
        multisig_pk: multisig_pk_1,
        bytes: OnceCell::new(),
    };

    let generic_sig_1 = GenericSignature::MultiSig(multisig_1);
    let generic_sig_bytes = generic_sig_1.as_bytes();
    assert!(GenericSignature::from_bytes(generic_sig_bytes).is_err());

    // Single sig serialization unchanged.
    let sig = Ed25519SuiSignature::default();
    let single_sig = GenericSignature::Signature(sig.clone().into());
    let single_sig_bytes = single_sig.as_bytes();
    let single_sig_roundtrip = GenericSignature::from_bytes(single_sig_bytes).unwrap();
    assert_eq!(single_sig, single_sig_roundtrip);
    assert_eq!(single_sig_bytes.len(), Ed25519SuiSignature::LENGTH);
    assert_eq!(
        single_sig_bytes.first().unwrap(),
        &Ed25519SuiSignature::SCHEME.flag()
    );
    assert_eq!(sig.as_bytes().len(), single_sig_bytes.len());
}

#[test]
fn single_sig_port_works() {
    let kp: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let addr = SuiAddress::from(&kp.public());
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig = Signature::new_secure(&msg, &kp);
    assert!(sig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_ok());
}

#[test]
fn test_multisig_pk_failure() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    // Fails on weight 0.
    assert!(MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![0, 1, 1],
        2
    )
    .is_err());

    // Fails on threshold 0.
    assert!(MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![0, 1, 1],
        0
    )
    .is_err());

    // Fails on incorrect array length.
    assert!(
        MultiSigPublicKey::new(vec![pk1.clone(), pk2.clone(), pk3.clone()], vec![1], 2).is_err()
    );

    // Fails on empty array length.
    assert!(MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![], 2).is_err());
}

#[test]
fn test_multisig_address() {
    // Pin an hardcoded multisig address generation here. If this fails, the address
    // generation logic may have changed. If this is intended, update the hardcoded value below.
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let threshold: ThresholdUnit = 2;
    let w1: WeightUnit = 1;
    let w2: WeightUnit = 2;
    let w3: WeightUnit = 3;

    let multisig_pk =
        MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![w1, w2, w3], threshold).unwrap();
    let address: SuiAddress = (&multisig_pk).into();
    assert_eq!(
        SuiAddress::from_str("0xe35c69eb504de34afdbd9f307fb3ca152646c92d549fea00065d26fc422109ea")
            .unwrap(),
        address
    );
}

#[test]
fn test_max_sig() {
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let mut seed = StdRng::from_seed([0; 32]);
    let mut keys = Vec::new();
    let mut sigs = Vec::new();
    let mut pks = Vec::new();

    for _ in 0..11 {
        let k = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1);
        sigs.push(Signature::new_secure(&msg, &k));
        pks.push(k.public());
        keys.push(k);
    }

    // multisig_pk with larger that max number of pks fails.
    assert!(MultiSigPublicKey::new(
        pks.clone(),
        vec![WeightUnit::MAX; MAX_SIGNER_IN_MULTISIG + 1],
        ThresholdUnit::MAX
    )
    .is_err());

    // multisig_pk with unreachable threshold fails.
    assert!(MultiSigPublicKey::new(pks.clone()[..5].to_vec(), vec![3; 5], 16).is_err());

    // multisig_pk with max weights for each pk and max reachable threshold is ok.
    let high_threshold_pk = MultiSigPublicKey::new(
        pks.clone()[..10].to_vec(),
        vec![WeightUnit::MAX; MAX_SIGNER_IN_MULTISIG],
        (WeightUnit::MAX as ThresholdUnit) * (MAX_SIGNER_IN_MULTISIG as ThresholdUnit),
    )
    .unwrap();
    let address: SuiAddress = (&high_threshold_pk).into();

    // But max threshold cannot be met, fails to verify.
    sigs.remove(10);
    sigs.remove(0);
    let multisig = MultiSig::combine(sigs, high_threshold_pk).unwrap();
    assert!(multisig
        .verify_authenticator(&msg, address, None, &VerifyParams::default())
        .is_err());

    // multisig_pk with max weights for each pk with threshold is 1x max weight verifies ok.
    let low_threshold_pk = MultiSigPublicKey::new(
        pks.clone()[..10].to_vec(),
        vec![WeightUnit::MAX; 10],
        WeightUnit::MAX.into(),
    )
    .unwrap();
    let address: SuiAddress = (&low_threshold_pk).into();
    let sig = Signature::new_secure(&msg, &keys[0]);
    let multisig = MultiSig::combine(vec![sig; 1], low_threshold_pk).unwrap();
    assert!(multisig
        .verify_authenticator(&msg, address, None, &VerifyParams::default())
        .is_ok());
}

#[test]
fn multisig_serde_test() {
    let k1 = SuiKeyPair::Ed25519(Ed25519KeyPair::from(
        Ed25519PrivateKey::from_bytes(&[
            59, 148, 11, 85, 134, 130, 61, 253, 2, 174, 59, 70, 27, 180, 51, 107, 94, 203, 174,
            253, 102, 39, 170, 146, 46, 252, 4, 143, 236, 12, 136, 28,
        ])
        .unwrap(),
    ));
    let pk1 = k1.public();
    assert_eq!(
        pk1.as_ref(),
        [
            90, 226, 32, 180, 178, 246, 94, 151, 124, 18, 237, 230, 21, 121, 255, 81, 112, 182,
            194, 44, 0, 97, 104, 195, 123, 94, 124, 97, 175, 1, 128, 131
        ]
    );

    let k2 = SuiKeyPair::Secp256k1(Secp256k1KeyPair::from(
        Secp256k1PrivateKey::from_bytes(&[
            59, 148, 11, 85, 134, 130, 61, 253, 2, 174, 59, 70, 27, 180, 51, 107, 94, 203, 174,
            253, 102, 39, 170, 146, 46, 252, 4, 143, 236, 12, 136, 28,
        ])
        .unwrap(),
    ));
    let pk2 = k2.public();
    assert_eq!(
        pk2.as_ref(),
        [
            2, 29, 21, 35, 7, 198, 183, 43, 14, 208, 65, 139, 14, 112, 205, 128, 231, 245, 41, 91,
            141, 134, 245, 114, 45, 63, 82, 19, 251, 210, 57, 79, 54
        ]
    );

    let k3 = SuiKeyPair::Ed25519(Ed25519KeyPair::from(
        Ed25519PrivateKey::from_bytes(&[0; 32]).unwrap(),
    ));
    let pk3 = k3.public();
    assert_eq!(
        pk3.as_ref(),
        [
            59, 106, 39, 188, 206, 182, 164, 45, 98, 163, 168, 208, 42, 111, 13, 115, 101, 50, 21,
            119, 29, 226, 67, 166, 58, 192, 72, 161, 139, 89, 218, 41
        ]
    );

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![1, 2, 3], 3).unwrap();
    let addr = SuiAddress::from(&multisig_pk);

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &k1);
    let sig2 = Signature::new_secure(&msg, &k2);

    let multi_sig = MultiSig::combine(vec![sig1, sig2], multisig_pk).unwrap();
    assert_eq!(Base64::encode(multi_sig.as_bytes()), "AwIAvlJnUP0iJFZL+QTxkKC9FHZGwCa5I4TITHS/QDQ12q1sYW6SMt2Yp3PSNzsAay0Fp2MPVohqyyA02UtdQ2RNAQGH0eLk4ifl9h1I8Uc+4QlRYfJC21dUbP8aFaaRqiM/f32TKKg/4PSsGf9lFTGwKsHJYIMkDoqKwI8Xqr+3apQzAwADAFriILSy9l6XfBLt5hV5/1FwtsIsAGFow3tefGGvAYCDAQECHRUjB8a3Kw7QQYsOcM2A5/UpW42G9XItP1IT+9I5TzYCADtqJ7zOtqQtYqOo0CpvDXNlMhV3HeJDpjrASKGLWdopAwMA");

    assert!(multi_sig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_ok());

    assert_eq!(
        addr,
        SuiAddress::from_str("0x37b048598ca569756146f4e8ea41666c657406db154a31f11bb5c1cbaf0b98d7")
            .unwrap()
    );
}

#[test]
fn multisig_legacy_serde_test() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig0 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[2]);

    let multisig_pk_legacy =
        MultiSigPublicKeyLegacy::new(vec![pk1, pk2, pk3], vec![1, 2, 3], 3).unwrap();
    let addr: SuiAddress = (&multisig_pk_legacy).into();

    let multi_sig_legacy = MultiSigLegacy::combine(vec![sig0, sig2], multisig_pk_legacy).unwrap();

    let binding = GenericSignature::MultiSigLegacy(multi_sig_legacy);
    let serialized_multisig = binding.as_ref();
    let deserialized_multisig = GenericSignature::from_bytes(serialized_multisig).unwrap();
    assert!(deserialized_multisig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_ok());
}

#[test]
fn test_to_from_indices() {
    assert!(as_indices(0b11111111110).is_err());
    assert_eq!(as_indices(0b0000010110).unwrap(), vec![1, 2, 4]);
    assert_eq!(
        as_indices(0b1111111111).unwrap(),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    );

    let mut bitmap = RoaringBitmap::new();
    bitmap.insert(1);
    bitmap.insert(2);
    bitmap.insert(4);
    assert_eq!(bitmap_to_u16(bitmap.clone()).unwrap(), 0b0000010110);
    bitmap.insert(11);
    assert!(bitmap_to_u16(bitmap).is_err());
}

#[test]
fn multisig_invalid_instance() {
    let keys = keys();
    let pk1 = keys[0].public();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);

    let public_keys_and_weights: Vec<(PublicKey, WeightUnit)> = vec![(pk1, 0)];

    let invalid_multisig_pk = MultiSigPublicKey::construct(public_keys_and_weights, u16::MIN);

    let addr = SuiAddress::from(&invalid_multisig_pk);

    let invalid_multisig =
        MultiSig::new(vec![sig1.to_compressed().unwrap()], 3, invalid_multisig_pk);

    assert!(invalid_multisig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());
}

#[test]
fn multisig_invalid_bitmap_instance() {
    let keys = keys();
    let pk1 = keys[0].public();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);

    let public_keys_and_weights: Vec<(PublicKey, WeightUnit)> = vec![(pk1, 1)];

    let invalid_multisig_pk = MultiSigPublicKey::construct(public_keys_and_weights, 1);

    let addr = SuiAddress::from(&invalid_multisig_pk);

    // Trying to pass invalid bitmap [2, 7, 9]
    let invalid_multisig = MultiSig::new(
        vec![sig1.to_compressed().unwrap()],
        644,
        invalid_multisig_pk,
    );

    assert!(invalid_multisig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());
}

#[test]
fn multisig_empty_invalid_instance() {
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    let invalid_multisig_pk = MultiSigPublicKey::construct(vec![], u16::MIN);

    let addr = SuiAddress::from(&invalid_multisig_pk);

    let invalid_multisig = MultiSig::new(vec![], 3, invalid_multisig_pk);

    assert!(invalid_multisig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());
}

#[test]
fn multisig_pass_same_publickey() {
    let keys = keys();
    let pk1 = keys[0].public();

    // It should be impossible to create such instance.
    assert!(
        MultiSigPublicKey::new(vec![pk1.clone(), pk1.clone(), pk1], vec![1, 2, 3], 4,).is_err()
    );
}

#[test]
fn multisig_user_authenticator_epoch() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![1, 1, 1], 2).unwrap();
    let addr = SuiAddress::from(&multisig_pk);
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);

    let multi_sig1 = MultiSig::combine(vec![sig1, sig2], multisig_pk).unwrap();

    // EpochId is set to 'Some(1)' value.
    assert!(multi_sig1
        .verify_authenticator(&msg, addr, Some(1), &VerifyParams::default())
        .is_ok());
}

#[test]
fn multisig_combine_invalid_multisig_publickey() {
    let mut seed = StdRng::from_seed([0; 32]);
    let mut keys = Vec::new();
    let mut public_keys_and_weights = Vec::<(PublicKey, WeightUnit)>::new();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    // Create invalid number of public keys.
    for _ in 0..11 {
        let k = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1);
        public_keys_and_weights.push((k.public(), 1));
        keys.push(k);
    }

    let invalid_multisig_pk = MultiSigPublicKey::construct(public_keys_and_weights, 2);

    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);

    assert!(MultiSig::combine(vec![sig1, sig2], invalid_multisig_pk).is_err());
}

#[test]
fn multisig_invalid_number_of_publickeys() {
    let mut seed = StdRng::from_seed([0; 32]);
    let mut keys = Vec::new();
    let mut public_keys_and_weights = Vec::<(PublicKey, WeightUnit)>::new();

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    // Create invalid number of public keys.
    for _ in 0..11 {
        let k = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1);
        public_keys_and_weights.push((k.public(), 1));
        keys.push(k);
    }

    let invalid_multisig_pk = MultiSigPublicKey::construct(public_keys_and_weights, 2);

    let addr = SuiAddress::from(&invalid_multisig_pk);

    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);

    let invalid_multisig = MultiSig::new(
        vec![sig1.to_compressed().unwrap(), sig2.to_compressed().unwrap()],
        3,
        invalid_multisig_pk,
    );

    assert!(invalid_multisig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());
}

#[test]
fn multisig_invalid_publickey_ed25519_signature() {
    let keys = keys();
    let pk1_ed25519 = keys[0].public();
    let pk2_secp256k1 = keys[1].public();
    let pk3_secp256r1 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(
        vec![pk2_secp256k1, pk1_ed25519, pk3_secp256r1],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let addr = SuiAddress::from(&multisig_pk);
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig_ed25519 = Signature::new_secure(&msg, &keys[0]);
    let sig_secp256k1 = Signature::new_secure(&msg, &keys[1]);

    // Change position for signatures is not ok with plain bitmap
    let multi_sig = MultiSig::combine(vec![sig_ed25519, sig_secp256k1], multisig_pk).unwrap();

    // Since sig_ed25519 on the place of sig_secp256k1, should throw an error
    assert!(multi_sig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());
}

#[test]
fn multisig_invalid_publickey_secp256r1_signature() {
    let keys = keys();
    let pk1_ed25519 = keys[0].public();
    let pk2_secp256k1 = keys[1].public();
    let pk3_secp256r1 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(
        vec![pk2_secp256k1, pk1_ed25519, pk3_secp256r1],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let addr = SuiAddress::from(&multisig_pk);
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig_ed25519 = Signature::new_secure(&msg, &keys[0]);
    let sig_secp256r1 = Signature::new_secure(&msg, &keys[2]);

    // Change position for signatures is not ok with plain bitmap
    let multi_sig = MultiSig::combine(vec![sig_secp256r1, sig_ed25519], multisig_pk).unwrap();

    // Since sig_secp256r1 on the place of sig_secp256k1, should throw an error
    assert!(multi_sig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_err());
}

#[test]
fn multisig_get_pk() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2], vec![1, 1], 2).unwrap();
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);

    let multi_sig = MultiSig::combine(vec![sig1, sig2], multisig_pk.clone()).unwrap();

    assert!(multi_sig.get_pk().clone() == multisig_pk);
}

#[test]
fn multisig_get_sigs() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2], vec![1, 1], 2).unwrap();
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);

    let multi_sig = MultiSig::combine(vec![sig1.clone(), sig2.clone()], multisig_pk).unwrap();

    assert!(
        *multi_sig.get_sigs() == vec![sig1.to_compressed().unwrap(), sig2.to_compressed().unwrap()]
    );
}

#[test]
fn multisig_get_indices() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![1, 1, 1], 2).unwrap();
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );
    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);
    let sig3 = Signature::new_secure(&msg, &keys[2]);

    let multi_sig1 =
        MultiSig::combine(vec![sig2.clone(), sig3.clone()], multisig_pk.clone()).unwrap();

    let multi_sig2 = MultiSig::combine(
        vec![sig1.clone(), sig2.clone(), sig3.clone()],
        multisig_pk.clone(),
    )
    .unwrap();

    let invalid_multisig = MultiSig::combine(vec![sig3, sig2, sig1], multisig_pk).unwrap();

    // Indexes of public keys in multisig public key instance according to the combined sigs.
    assert!(multi_sig1.get_indices().unwrap() == vec![1, 2]);
    assert!(multi_sig2.get_indices().unwrap() == vec![0, 1, 2]);
    assert!(invalid_multisig.get_indices().unwrap() == vec![0, 1, 2]);
}

#[test]
fn multisig_new_hashed_signature() {
    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2, pk3], vec![1, 1, 1], 2).unwrap();
    let addr = SuiAddress::from(&multisig_pk);
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    // Hashing message.
    let mut hasher = DefaultHash::default();
    hasher.update(&bcs::to_bytes(&msg).unwrap());
    let hashed_msg = &hasher.finalize().digest;

    let data_slice: &[u8] = &[
        139, 154, 166, 246, 8, 240, 82, 222, 250, 76, 251, 120, 251, 183, 196, 193, 221, 35, 104,
        163, 77, 17, 102, 70, 39, 119, 168, 24, 30, 124, 91, 181,
    ];

    // To avoid changes in the hash functions, compare it to hardcoded data slice.
    assert!(hashed_msg == data_slice);

    let sig2 = Signature::new_hashed(hashed_msg, &keys[1]);
    let sig3 = Signature::new_hashed(hashed_msg, &keys[2]);

    let multi_sig = MultiSig::combine(vec![sig2, sig3], multisig_pk).unwrap();

    assert!(multi_sig
        .verify_authenticator(&msg, addr, None, &VerifyParams::default())
        .is_ok());
}
