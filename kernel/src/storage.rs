// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use pre_block::{Digest, PreBlockStore, PublicKey};
use tezos_crypto_rs::blake2b::digest_256;
use tezos_smart_rollup_host::{
    path::{concat, OwnedPath, Path, RefPath},
    runtime::Runtime,
};

const HEAD_PATH: RefPath = RefPath::assert_from(b"/head");
const BLOCKS_PATH: RefPath = RefPath::assert_from(b"/blocks");
const CHUNKS_PATH: RefPath = RefPath::assert_from(b"/chunks");
const INDEX_PATH: RefPath = RefPath::assert_from(b"/index");
const AUTHORITIES_PATH: RefPath = RefPath::assert_from(b"/authorities");
const CERTIFICATES_PATH: RefPath = RefPath::assert_from(b"/certificates");

fn certificate_path(digest: &Digest) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!("/{}", hex::encode(digest.as_ref()))).unwrap();
    concat(&CERTIFICATES_PATH, &suffix).unwrap()
}

fn write_u64_be(host: &mut impl Runtime, path: &impl Path, value: u64) {
    host.store_write_all(path, &value.to_be_bytes()).unwrap();
}

fn read_u64_be(host: &impl Runtime, path: &impl Path) -> Option<u64> {
    if host.store_has(path).unwrap().is_some() {
        let bytes = host.store_read_all(path).unwrap();
        Some(u64::from_be_bytes(
            bytes.try_into().expect("Expected 8 bytes"),
        ))
    } else {
        None
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
    fn get_certificate_index(&self, digest: &Digest) -> Option<u64> {
        read_u64_be(self.host, &certificate_path(digest))
    }

    fn set_certificate_index(&mut self, digest: &Digest, index: u64) {
        write_u64_be(self.host, &certificate_path(digest), index);
    }

    fn get_latest_index(&self) -> Option<u64> {
        read_u64_be(self.host, &INDEX_PATH)
    }

    fn set_latest_index(&mut self, index: u64) {
        write_u64_be(self.host, &INDEX_PATH, index)
    }
}

fn authorities_path(epoch: u64) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!("/{}", epoch)).unwrap();
    concat(&AUTHORITIES_PATH, &suffix).unwrap()
}

pub fn read_authorities<Host: Runtime>(host: &Host, epoch: u64) -> Option<Vec<PublicKey>> {
    let path = authorities_path(epoch);
    if host.store_has(&path).unwrap().is_some() {
        let bytes = host
            .store_read_all(&path)
            .expect("Failed to read authorities");
        Some(bcs::from_bytes(&bytes).expect("Failed to parse authorities"))
    } else {
        None
    }
}

pub fn write_authorities<Host: Runtime>(host: &mut Host, epoch: u64, authorities: &[PublicKey]) {
    let bytes = bcs::to_bytes(authorities).unwrap();
    host.store_write_all(&authorities_path(epoch), &bytes)
        .expect("Failed to write authorities");
}

pub fn read_head<Host: Runtime>(host: &Host) -> u32 {
    if host.store_has(&HEAD_PATH).unwrap().is_some() {
        let bytes = host.store_read_all(&HEAD_PATH).unwrap();
        u32::from_be_bytes(bytes.try_into().expect("Expected 4 bytes"))
    } else {
        0
    }
}

pub fn write_head<Host: Runtime>(host: &mut Host, level: u32) {
    host.store_write_all(&HEAD_PATH, &level.to_be_bytes())
        .unwrap();
}

fn block_path(level: u32) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!("/{}", level)).unwrap();
    concat(&BLOCKS_PATH, &suffix).unwrap()
}

pub fn write_block<Host: Runtime>(host: &mut Host, level: u32, block: Vec<Vec<u8>>) {
    let payload = bcs::to_bytes(&block).unwrap();
    host.store_write_all(&block_path(level), &payload).unwrap();
}

fn chunk_path(hash: &[u8]) -> OwnedPath {
    let suffix = OwnedPath::try_from(format!("/{}", hex::encode(hash))).unwrap();
    concat(&CHUNKS_PATH, &suffix).unwrap()
}

pub fn write_chunk(host: &mut impl Runtime, data: &[u8]) -> Vec<u8> {
    let hash = digest_256(data).expect("Failed to get chunk digest");
    host.store_write_all(&chunk_path(&hash), data).unwrap();
    hash
}

pub fn read_chunk(host: &impl Runtime, hash: &[u8]) -> Option<Vec<u8>> {
    let path = chunk_path(hash);
    if host.store_has(&path).unwrap().is_some() {
        let bytes = host.store_read_all(&path).unwrap();
        Some(bytes)
    } else {
        None
    }
}
