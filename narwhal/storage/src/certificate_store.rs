// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use fastcrypto::hash::Hash;
use lru::LruCache;
use parking_lot::Mutex;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::{cmp::Ordering, collections::BTreeMap, iter};
use sui_macros::fail_point;
use tap::Tap;

use crate::StoreResult;
use config::AuthorityIdentifier;
use mysten_common::sync::notify_read::NotifyRead;
use store::{
    rocks::{DBMap, TypedStoreError::RocksDBError},
    Map,
};
use types::{Certificate, CertificateDigest, Round};

#[derive(Clone)]
pub struct CertificateStoreCacheMetrics {
    hit: IntCounter,
    miss: IntCounter,
}

impl CertificateStoreCacheMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            hit: register_int_counter_with_registry!(
                "certificate_store_cache_hit",
                "The number of hits in the cache",
                registry
            )
            .unwrap(),
            miss: register_int_counter_with_registry!(
                "certificate_store_cache_miss",
                "The number of miss in the cache",
                registry
            )
            .unwrap(),
        }
    }
}

/// A cache trait to be used as temporary in-memory store when accessing the underlying
/// certificate_store. Using the cache allows to skip rocksdb access giving us benefits
/// both on less disk access (when value not in db's cache) and also avoiding any additional
/// deserialization costs.
pub trait Cache {
    fn write(&self, certificate: Certificate);
    fn write_all(&self, certificate: Vec<Certificate>);
    fn read(&self, digest: &CertificateDigest) -> Option<Certificate>;

    /// Returns the certificates by performing a look up in the cache. The method is expected to
    /// always return a result for every provided digest (when found will be Some, None otherwise)
    /// and in the same order.
    fn read_all(
        &self,
        digests: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate>)>;

    /// Checks existence of one or more digests.
    fn contains(&self, digest: &CertificateDigest) -> bool;
    fn multi_contains<'a>(&self, digests: impl Iterator<Item = &'a CertificateDigest>)
        -> Vec<bool>;

    fn remove(&self, digest: &CertificateDigest);
    fn remove_all(&self, digests: Vec<CertificateDigest>);
}

/// An LRU cache for the certificate store.
#[derive(Clone)]
pub struct CertificateStoreCache {
    cache: Arc<Mutex<LruCache<CertificateDigest, Certificate>>>,
    metrics: Option<CertificateStoreCacheMetrics>,
}

impl CertificateStoreCache {
    pub fn new(size: NonZeroUsize, metrics: Option<CertificateStoreCacheMetrics>) -> Self {
        Self {
            cache: Arc::new(Mutex::new(LruCache::new(size))),
            metrics,
        }
    }

    fn report_result(&self, is_hit: bool) {
        if let Some(metrics) = self.metrics.as_ref() {
            if is_hit {
                metrics.hit.inc()
            } else {
                metrics.miss.inc()
            }
        }
    }
}

impl Cache for CertificateStoreCache {
    fn write(&self, certificate: Certificate) {
        let mut guard = self.cache.lock();
        guard.put(certificate.digest(), certificate);
    }

    fn write_all(&self, certificate: Vec<Certificate>) {
        let mut guard = self.cache.lock();
        for cert in certificate {
            guard.put(cert.digest(), cert);
        }
    }

    /// Fetches the certificate for the provided digest. This method will update the LRU record
    /// and mark it as "last accessed".
    fn read(&self, digest: &CertificateDigest) -> Option<Certificate> {
        let mut guard = self.cache.lock();
        guard
            .get(digest)
            .cloned()
            .tap(|v| self.report_result(v.is_some()))
    }

    /// Fetches the certificates for the provided digests. This method will update the LRU records
    /// and mark them as "last accessed".
    fn read_all(
        &self,
        digests: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate>)> {
        let mut guard = self.cache.lock();
        digests
            .into_iter()
            .map(move |id| {
                (
                    id,
                    guard
                        .get(&id)
                        .cloned()
                        .tap(|v| self.report_result(v.is_some())),
                )
            })
            .collect()
    }

    // The method does not update the LRU record, thus
    // it will not count as a "last access" for the provided digest.
    fn contains(&self, digest: &CertificateDigest) -> bool {
        let guard = self.cache.lock();
        guard
            .contains(digest)
            .tap(|result| self.report_result(*result))
    }

    fn multi_contains<'a>(
        &self,
        digests: impl Iterator<Item = &'a CertificateDigest>,
    ) -> Vec<bool> {
        let guard = self.cache.lock();
        digests
            .map(|digest| {
                guard
                    .contains(digest)
                    .tap(|result| self.report_result(*result))
            })
            .collect()
    }

    fn remove(&self, digest: &CertificateDigest) {
        let mut guard = self.cache.lock();
        let _ = guard.pop(digest);
    }

    fn remove_all(&self, digests: Vec<CertificateDigest>) {
        let mut guard = self.cache.lock();
        for digest in digests {
            let _ = guard.pop(&digest);
        }
    }
}

/// An implementation that basically disables the caching functionality when used for CertificateStore.
#[derive(Clone)]
struct NoCache {}

impl Cache for NoCache {
    fn write(&self, _certificate: Certificate) {
        // no-op
    }

    fn write_all(&self, _certificate: Vec<Certificate>) {
        // no-op
    }

    fn read(&self, _digest: &CertificateDigest) -> Option<Certificate> {
        None
    }

    fn read_all(
        &self,
        digests: Vec<CertificateDigest>,
    ) -> Vec<(CertificateDigest, Option<Certificate>)> {
        digests.into_iter().map(|digest| (digest, None)).collect()
    }

    fn contains(&self, _digest: &CertificateDigest) -> bool {
        false
    }

    fn multi_contains<'a>(
        &self,
        digests: impl Iterator<Item = &'a CertificateDigest>,
    ) -> Vec<bool> {
        digests.map(|_| false).collect()
    }

    fn remove(&self, _digest: &CertificateDigest) {
        // no-op
    }

    fn remove_all(&self, _digests: Vec<CertificateDigest>) {
        // no-op
    }
}

/// The main storage when we have to deal with certificates. It maintains
/// two storages, one main which saves the certificates by their ids, and a
/// secondary one which acts as an index to allow us fast retrieval based
/// for queries based in certificate rounds.
/// It also offers pub/sub capabilities in write events. By using the
/// `notify_read` someone can wait to hear until a certificate by a specific
/// id has been written in storage.
#[derive(Clone)]
pub struct CertificateStore<T: Cache = CertificateStoreCache> {
    /// Holds the certificates by their digest id
    certificates_by_id: DBMap<CertificateDigest, Certificate>,
    /// A secondary index that keeps the certificate digest ids
    /// by the certificate rounds. Certificate origin is used to produce unique keys.
    /// This helps us to perform range requests based on rounds. We avoid storing again the
    /// certificate here to not waste space. To dereference we use the certificates_by_id storage.
    certificate_id_by_round: DBMap<(Round, AuthorityIdentifier), CertificateDigest>,
    /// A secondary index that keeps the certificate digest ids
    /// by the certificate origins. Certificate rounds are used to produce unique keys.
    /// This helps us to perform range requests based on rounds. We avoid storing again the
    /// certificate here to not waste space. To dereference we use the certificates_by_id storage.
    certificate_id_by_origin: DBMap<(AuthorityIdentifier, Round), CertificateDigest>,
    /// The pub/sub to notify for a write that happened for a certificate digest id
    notify_subscribers: Arc<NotifyRead<CertificateDigest, Certificate>>,
    /// An LRU cache to keep recent certificates
    cache: Arc<T>,
}

impl<T: Cache> CertificateStore<T> {
    pub fn new(
        certificates_by_id: DBMap<CertificateDigest, Certificate>,
        certificate_id_by_round: DBMap<(Round, AuthorityIdentifier), CertificateDigest>,
        certificate_id_by_origin: DBMap<(AuthorityIdentifier, Round), CertificateDigest>,
        certificate_store_cache: T,
    ) -> CertificateStore<T> {
        Self {
            certificates_by_id,
            certificate_id_by_round,
            certificate_id_by_origin,
            notify_subscribers: Arc::new(NotifyRead::new()),
            cache: Arc::new(certificate_store_cache),
        }
    }

    /// Inserts a certificate to the store
    pub fn write(&self, certificate: Certificate) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let mut batch = self.certificates_by_id.batch();

        let id = certificate.digest();

        // write the certificate by its id
        batch.insert_batch(
            &self.certificates_by_id,
            iter::once((id, certificate.clone())),
        )?;

        // Index the certificate id by its round and origin.
        batch.insert_batch(
            &self.certificate_id_by_round,
            iter::once(((certificate.round(), certificate.origin()), id)),
        )?;
        batch.insert_batch(
            &self.certificate_id_by_origin,
            iter::once(((certificate.origin(), certificate.round()), id)),
        )?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.notify_subscribers.notify(&id, &certificate);
        }

        // insert in cache
        self.cache.write(certificate);

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Inserts multiple certificates in the storage. This is an atomic operation.
    /// In the end it notifies any subscribers that are waiting to hear for the
    /// value.
    pub fn write_all(
        &self,
        certificates: impl IntoIterator<Item = Certificate>,
    ) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        let mut batch = self.certificates_by_id.batch();

        let certificates: Vec<_> = certificates
            .into_iter()
            .map(|certificate| (certificate.digest(), certificate))
            .collect();

        // write the certificates by their ids
        batch.insert_batch(&self.certificates_by_id, certificates.clone())?;

        // write the certificates id by their rounds
        let values = certificates.iter().map(|(digest, c)| {
            let key = (c.round(), c.origin());
            let value = digest;
            (key, value)
        });
        batch.insert_batch(&self.certificate_id_by_round, values)?;

        // write the certificates id by their origins
        let values = certificates.iter().map(|(digest, c)| {
            let key = (c.origin(), c.round());
            let value = digest;
            (key, value)
        });
        batch.insert_batch(&self.certificate_id_by_origin, values)?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            for (_id, certificate) in &certificates {
                self.notify_subscribers
                    .notify(&certificate.digest(), certificate);
            }
        }

        self.cache.write_all(
            certificates
                .into_iter()
                .map(|(_, certificate)| certificate)
                .collect(),
        );

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Retrieves a certificate from the store. If not found
    /// then None is returned as result.
    pub fn read(&self, id: CertificateDigest) -> StoreResult<Option<Certificate>> {
        if let Some(certificate) = self.cache.read(&id) {
            return Ok(Some(certificate));
        }

        self.certificates_by_id.get(&id)
    }

    /// Retrieves a certificate from the store by round and authority.
    /// If not found, None is returned as result.
    pub fn read_by_index(
        &self,
        origin: AuthorityIdentifier,
        round: Round,
    ) -> StoreResult<Option<Certificate>> {
        match self.certificate_id_by_origin.get(&(origin, round))? {
            Some(d) => self.read(d),
            None => Ok(None),
        }
    }

    pub fn contains(&self, digest: &CertificateDigest) -> StoreResult<bool> {
        if self.cache.contains(digest) {
            return Ok(true);
        }
        self.certificates_by_id.contains_key(digest)
    }

    pub fn multi_contains<'a>(
        &self,
        digests: impl Iterator<Item = &'a CertificateDigest>,
    ) -> StoreResult<Vec<bool>> {
        // Batch checks into the cache and the certificate store.
        let digests = digests.enumerate().collect::<Vec<_>>();
        let mut found = self.cache.multi_contains(digests.iter().map(|(_, d)| *d));
        let store_lookups = digests
            .iter()
            .zip(found.iter())
            .filter_map(|((i, d), hit)| if *hit { None } else { Some((*i, *d)) })
            .collect::<Vec<_>>();
        let store_found = self
            .certificates_by_id
            .multi_contains_keys(store_lookups.iter().map(|(_, d)| *d))?;
        for ((i, _d), hit) in store_lookups.into_iter().zip(store_found.into_iter()) {
            debug_assert!(!found[i]);
            if hit {
                found[i] = true;
            }
        }
        Ok(found)
    }

    /// Retrieves multiple certificates by their provided ids. The results
    /// are returned in the same sequence as the provided keys.
    pub fn read_all(
        &self,
        ids: impl IntoIterator<Item = CertificateDigest>,
    ) -> StoreResult<Vec<Option<Certificate>>> {
        let mut found = HashMap::new();
        let mut missing = Vec::new();

        // first find whatever we can from our local cache
        let ids: Vec<CertificateDigest> = ids.into_iter().collect();
        for (id, certificate) in self.cache.read_all(ids.clone()) {
            if let Some(certificate) = certificate {
                found.insert(id, certificate.clone());
            } else {
                missing.push(id);
            }
        }

        // then fallback for all the misses on the storage
        let from_store = self.certificates_by_id.multi_get(&missing)?;
        from_store
            .iter()
            .zip(missing)
            .for_each(|(certificate, id)| {
                if let Some(certificate) = certificate {
                    found.insert(id, certificate.clone());
                }
            });

        Ok(ids.into_iter().map(|id| found.get(&id).cloned()).collect())
    }

    /// Waits to get notified until the requested certificate becomes available
    pub async fn notify_read(&self, id: CertificateDigest) -> StoreResult<Certificate> {
        // we register our interest to be notified with the value
        let receiver = self.notify_subscribers.register_one(&id);

        // let's read the value because we might have missed the opportunity
        // to get notified about it
        if let Ok(Some(cert)) = self.read(id) {
            // notify any obligations - and remove the entries
            self.notify_subscribers.notify(&id, &cert);

            // reply directly
            return Ok(cert);
        }

        // now wait to hear back the result
        let result = receiver.await;

        Ok(result)
    }

    /// Deletes a single certificate by its digest.
    pub fn delete(&self, id: CertificateDigest) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");
        // first read the certificate to get the round - we'll need in order
        // to delete the secondary index
        let cert = match self.read(id)? {
            Some(cert) => cert,
            None => return Ok(()),
        };

        let mut batch = self.certificates_by_id.batch();

        // write the certificate by its id
        batch.delete_batch(&self.certificates_by_id, iter::once(id))?;

        // write the certificate index by its round
        let key = (cert.round(), cert.origin());

        batch.delete_batch(&self.certificate_id_by_round, iter::once(key))?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.cache.remove(&id);
        }

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Deletes multiple certificates in an atomic way.
    pub fn delete_all(&self, ids: impl IntoIterator<Item = CertificateDigest>) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");
        // first read the certificates to get their rounds - we'll need in order
        // to delete the secondary index
        let ids: Vec<CertificateDigest> = ids.into_iter().collect();
        let certs = self.read_all(ids.clone())?;
        let keys_by_round = certs
            .into_iter()
            .filter_map(|c| c.map(|cert| (cert.round(), cert.origin())))
            .collect::<Vec<_>>();
        if keys_by_round.is_empty() {
            return Ok(());
        }

        let mut batch = self.certificates_by_id.batch();

        // delete the certificates from the secondary index
        batch.delete_batch(&self.certificate_id_by_round, keys_by_round)?;

        // delete the certificates by its ids
        batch.delete_batch(&self.certificates_by_id, ids.clone())?;

        // execute the batch (atomically) and return the result
        let result = batch.write();

        if result.is_ok() {
            self.cache.remove_all(ids);
        }

        fail_point!("narwhal-store-after-write");
        result
    }

    /// Retrieves all the certificates with round >= the provided round.
    /// The result is returned with certificates sorted in round asc order
    pub fn after_round(&self, round: Round) -> StoreResult<Vec<Certificate>> {
        // Skip to a row at or before the requested round.
        // TODO: Add a more efficient seek method to typed store.
        let mut iter = self.certificate_id_by_round.unbounded_iter();
        if round > 0 {
            iter = iter.skip_to(&(round - 1, AuthorityIdentifier::default()))?;
        }

        let mut digests = Vec::new();
        for ((r, _), d) in iter {
            match r.cmp(&round) {
                Ordering::Equal | Ordering::Greater => {
                    digests.push(d);
                }
                Ordering::Less => {
                    continue;
                }
            }
        }

        // Fetch all those certificates from main storage, return an error if any one is missing.
        self.certificates_by_id
            .multi_get(digests.clone())?
            .into_iter()
            .map(|opt_cert| {
                opt_cert.ok_or_else(|| {
                    RocksDBError(format!(
                        "Certificate with some digests not found, CertificateStore invariant violation: {:?}",
                        digests
                    ))
                })
            })
            .collect()
    }

    /// Retrieves origins with certificates in each round >= the provided round.
    pub fn origins_after_round(
        &self,
        round: Round,
    ) -> StoreResult<BTreeMap<Round, Vec<AuthorityIdentifier>>> {
        // Skip to a row at or before the requested round.
        // TODO: Add a more efficient seek method to typed store.
        let mut iter = self.certificate_id_by_round.unbounded_iter();
        if round > 0 {
            iter = iter.skip_to(&(round - 1, AuthorityIdentifier::default()))?;
        }

        let mut result = BTreeMap::<Round, Vec<AuthorityIdentifier>>::new();
        for ((r, origin), _) in iter {
            if r < round {
                continue;
            }
            result.entry(r).or_default().push(origin);
        }
        Ok(result)
    }

    /// Retrieves the certificates of the last round and the round before that
    pub fn last_two_rounds_certs(&self) -> StoreResult<Vec<Certificate>> {
        // starting from the last element - hence the last round - move backwards until
        // we find certificates of different round.
        let certificates_reverse = self
            .certificate_id_by_round
            .unbounded_iter()
            .skip_to_last()
            .reverse();

        let mut round = 0;
        let mut certificates = Vec::new();

        for (key, digest) in certificates_reverse {
            let (certificate_round, _certificate_origin) = key;

            // We treat zero as special value (round unset) in order to
            // capture the last certificate's round.
            // We are now in a round less than the previous so we want to
            // stop consuming
            if round == 0 {
                round = certificate_round;
            } else if certificate_round < round - 1 {
                break;
            }

            let certificate = self.certificates_by_id.get(&digest)?.ok_or_else(|| {
                RocksDBError(format!(
                    "Certificate with id {} not found in main storage although it should",
                    digest
                ))
            })?;

            certificates.push(certificate);
        }

        Ok(certificates)
    }

    /// Retrieves the last certificate of the given origin.
    /// Returns None if there is no certificate for the origin.
    pub fn last_round(&self, origin: AuthorityIdentifier) -> StoreResult<Option<Certificate>> {
        let key = (origin, Round::MAX);
        if let Some(((name, _round), digest)) = self
            .certificate_id_by_origin
            .unbounded_iter()
            .skip_prior_to(&key)?
            .next()
        {
            if name == origin {
                return self.read(digest);
            }
        }
        Ok(None)
    }

    /// Retrieves the highest round number in the store.
    /// Returns 0 if there is no certificate in the store.
    pub fn highest_round_number(&self) -> Round {
        if let Some(((round, _), _)) = self
            .certificate_id_by_round
            .unbounded_iter()
            .skip_to_last()
            .reverse()
            .next()
        {
            round
        } else {
            0
        }
    }

    /// Retrieves the last round number of the given origin.
    /// Returns None if there is no certificate for the origin.
    pub fn last_round_number(&self, origin: AuthorityIdentifier) -> StoreResult<Option<Round>> {
        let key = (origin, Round::MAX);
        if let Some(((name, round), _)) = self
            .certificate_id_by_origin
            .unbounded_iter()
            .skip_prior_to(&key)?
            .next()
        {
            if name == origin {
                return Ok(Some(round));
            }
        }
        Ok(None)
    }

    /// Retrieves the next round number bigger than the given round for the origin.
    /// Returns None if there is no more local certificate from the origin with bigger round.
    pub fn next_round_number(
        &self,
        origin: AuthorityIdentifier,
        round: Round,
    ) -> StoreResult<Option<Round>> {
        let key = (origin, round + 1);
        if let Some(((name, round), _)) = self
            .certificate_id_by_origin
            .unbounded_iter()
            .skip_to(&key)?
            .next()
        {
            if name == origin {
                return Ok(Some(round));
            }
        }
        Ok(None)
    }

    /// Clears both the main storage of the certificates and the secondary index
    pub fn clear(&self) -> StoreResult<()> {
        fail_point!("narwhal-store-before-write");

        self.certificates_by_id.unsafe_clear()?;
        self.certificate_id_by_round.unsafe_clear()?;
        self.certificate_id_by_origin.unsafe_clear()?;

        fail_point!("narwhal-store-after-write");
        Ok(())
    }

    /// Checks whether the storage is empty. The main storage is
    /// being used to determine this.
    pub fn is_empty(&self) -> bool {
        self.certificates_by_id.is_empty()
    }
}
