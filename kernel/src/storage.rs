// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use fastcrypto::hash::Digest;
use narwhal_config::Epoch;
use narwhal_crypto::PublicKey;
use narwhal_types::{CertificateDigest, SequenceNumber};
use tezos_smart_rollup_host::{
    path::{concat, OwnedPath, RefPath},
    runtime::Runtime,
};

const SUB_DAG_INDEX_PATH: RefPath = RefPath::assert_from(b"/sub_dag_index");
const CERTIFICATES_PATH: RefPath = RefPath::assert_from(b"/certificates");
const AUTHORITIES_PATH: RefPath = RefPath::assert_from(b"/authorities");
const HEAD_PATH: RefPath = RefPath::assert_from(b"/head");
const BLOCKS_PATH: RefPath = RefPath::assert_from(b"/blocks");

pub const DIGEST_SIZE: usize = 32;

pub fn read_last_sub_dag_index<Host: Runtime>(host: &Host) -> Option<SequenceNumber> {
    match host.store_read_all(&SUB_DAG_INDEX_PATH) {
        Ok(bytes) => Some(SequenceNumber::from_be_bytes(
            bytes.try_into().expect("Expected 8 bytes"),
        )),
        Err(_) => None,
    }
}

pub fn write_last_sub_dag_index<Host: Runtime>(host: &mut Host, sub_dag_index: SequenceNumber) {
    host.store_write_all(&SUB_DAG_INDEX_PATH, &sub_dag_index.to_be_bytes())
        .unwrap();
}

fn certificate_path(sub_dag_index: SequenceNumber, digest: &CertificateDigest) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!(
        "{}/{}",
        sub_dag_index,
        hex::encode(digest.as_ref())
    ))
    .unwrap();
    concat(&CERTIFICATES_PATH, &suffix).unwrap()
}

pub fn remember_certificate<Host: Runtime>(
    host: &mut Host,
    sub_dag_index: SequenceNumber,
    digest: &CertificateDigest,
) {
    host.store_write_all(&certificate_path(sub_dag_index, digest), &[0u8])
        .unwrap();
}

pub fn is_known_certificate<Host: Runtime>(
    host: &Host,
    sub_dag_index: SequenceNumber,
    digest: &CertificateDigest,
) -> bool {
    host.store_has(&certificate_path(sub_dag_index, digest))
        .unwrap()
        .is_some()
}

fn authorities_path(epoch: Epoch) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!("{}", epoch)).unwrap();
    concat(&AUTHORITIES_PATH, &suffix).unwrap()
}

pub fn read_authorities<Host: Runtime>(host: &Host, epoch: Epoch) -> Vec<PublicKey> {
    let bytes = host
        .store_read_all(&authorities_path(epoch))
        .expect("Failed to read authorities");
    serde_json::from_slice(&bytes).expect("Failed to parse authorities")
}

pub fn write_authorities<Host: Runtime>(host: &mut Host, epoch: Epoch, authorities: &[PublicKey]) {
    let bytes = serde_json::to_vec(authorities).unwrap();
    host.store_write_all(&authorities_path(epoch), &bytes)
        .expect("Failed to write authorities");
}

pub fn read_head<Host: Runtime>(host: &Host) -> u32 {
    let bytes = host
        .store_read_all(&HEAD_PATH)
        .unwrap_or_else(|_| vec![0u8, 0u8, 0u8, 0u8]);
    u32::from_be_bytes(bytes.try_into().expect("Expected 4 bytes"))
}

pub fn write_head<Host: Runtime>(host: &mut Host, level: u32) {
    host.store_write_all(&HEAD_PATH, &level.to_be_bytes())
        .unwrap();
}

fn block_path(level: u32) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!("{}", level)).unwrap();
    concat(&BLOCKS_PATH, &suffix).unwrap()
}

pub fn write_block<Host: Runtime>(host: &mut Host, level: u32, block: &[Digest<DIGEST_SIZE>]) {
    let payload = serde_json::to_vec(block).unwrap();
    host.store_write_all(&block_path(level), &payload).unwrap();
}
