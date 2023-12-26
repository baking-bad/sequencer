// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use pre_block::{CertificateDigest, PublicKey, PreBlockStore};
use tezos_smart_rollup_host::{
    path::{concat, OwnedPath, RefPath, Path},
    runtime::Runtime,
};

const HEAD_PATH: RefPath = RefPath::assert_from(b"/head");
const BLOCKS_PATH: RefPath = RefPath::assert_from(b"/blocks");
const INDEX_PATH: RefPath = RefPath::assert_from(b"/index");
const TIMESTAMP_PATH: RefPath = RefPath::assert_from(b"/timestamp");
const AUTHORITIES_PATH: RefPath = RefPath::assert_from(b"/authorities");
const CERTIFICATES_PATH: RefPath = RefPath::assert_from(b"/certificates");

fn certificate_path(pre_block_index: u64, digest: &CertificateDigest) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!(
        "{}/{}",
        pre_block_index,
        hex::encode(digest.as_ref())
    ))
    .unwrap();
    concat(&CERTIFICATES_PATH, &suffix).unwrap()
}

fn write_u64_be(host: &mut impl Runtime, path: &impl Path, value: u64) {
    host.store_write_all(path, &value.to_be_bytes())
        .unwrap();
}

fn read_u64_be(host: &impl Runtime, path: &impl Path) -> Option<u64> {
    match host.store_read_all(path) {
        Ok(bytes) => Some(u64::from_be_bytes(
            bytes.try_into().expect("Expected 8 bytes"),
        )),
        Err(_) => None,
    }
}

#[derive(Debug)]
pub struct Store<'cs, Host: Runtime> {
    host: &'cs mut Host,
}

impl<'cs, Host: Runtime> Store<'cs, Host> {
    pub fn new(host: &'cs mut Host) -> Self {
        Self { host }
    }
}

impl<'cs, Host: Runtime> PreBlockStore for Store<'cs, Host> {
    fn has_certificate(&self, pre_block_index: u64, digest: &CertificateDigest) -> bool {
        self.host.store_has(&certificate_path(pre_block_index, digest))
            .unwrap()
            .is_some()
    }

    fn mem_certificate(&mut self, pre_block_index: u64, digest: &CertificateDigest) {
        self.host.store_write_all(&certificate_path(pre_block_index, digest), &[0u8])
            .unwrap();
    }

    fn get_index(&self) -> Option<u64> {
        read_u64_be(self.host, &INDEX_PATH)
    }

    fn set_index(&mut self, index: u64) {
        write_u64_be(self.host, &INDEX_PATH, index)
    }
}

pub fn write_timestamp(host: &mut impl Runtime, timestamp: u64) {
    write_u64_be(host, &TIMESTAMP_PATH, timestamp)
}

fn authorities_path(epoch: u64) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!("{}", epoch)).unwrap();
    concat(&AUTHORITIES_PATH, &suffix).unwrap()
}

pub fn read_authorities<Host: Runtime>(host: &Host, epoch: u64) -> Vec<PublicKey> {
    let bytes = host
        .store_read_all(&authorities_path(epoch))
        .expect("Failed to read authorities");
    serde_json_wasm::from_slice(&bytes).expect("Failed to parse authorities")
}

pub fn write_authorities<Host: Runtime>(host: &mut Host, epoch: u64, authorities: &[PublicKey]) {
    let bytes = serde_json_wasm::to_vec(authorities).unwrap();
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

pub fn write_block<Host: Runtime>(host: &mut Host, level: u32, block: Vec<Vec<u8>>) {
    let payload = serde_json_wasm::to_vec(&block).unwrap();
    host.store_write_all(&block_path(level), &payload).unwrap();
}
