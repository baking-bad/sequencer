// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::await_holding_lock)]

use std::{
    borrow::Borrow,
    collections::{btree_map::Iter, BTreeMap, HashMap, VecDeque},
    marker::PhantomData,
    ops::RangeBounds,
    sync::{Arc, RwLock},
};

use crate::{
    rocks::{be_fix_int_ser, TypedStoreError},
    Map,
};
use bincode::Options;
use collectable::TryExtend;
use ouroboros::self_referencing;
use rand::distributions::{Alphanumeric, DistString};
use rocksdb::Direction;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{RwLockReadGuard, RwLockWriteGuard};

/// An interface to a btree map backed sally database. This is mainly intended
/// for tests and performing benchmark comparisons
#[derive(Clone, Debug)]
pub struct TestDB<K, V> {
    pub rows: Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    pub name: String,
    _phantom: PhantomData<fn(K) -> V>,
}

impl<K, V> TestDB<K, V> {
    pub fn open() -> Self {
        TestDB {
            rows: Arc::new(RwLock::new(BTreeMap::new())),
            name: Alphanumeric.sample_string(&mut rand::thread_rng(), 16),
            _phantom: PhantomData,
        }
    }
    pub fn batch(&self) -> TestDBWriteBatch {
        TestDBWriteBatch::default()
    }
}

#[self_referencing(pub_extras)]
pub struct TestDBIter<'a, K, V> {
    pub rows: RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>,
    #[borrows(mut rows)]
    #[covariant]
    pub iter: Iter<'this, Vec<u8>, Vec<u8>>,
    phantom: PhantomData<(K, V)>,
    pub direction: Direction,
}

#[self_referencing(pub_extras)]
pub struct TestDBKeys<'a, K> {
    rows: RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>,
    #[borrows(mut rows)]
    #[covariant]
    pub iter: Iter<'this, Vec<u8>, Vec<u8>>,
    phantom: PhantomData<K>,
}

#[self_referencing(pub_extras)]
pub struct TestDBValues<'a, V> {
    rows: RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>,
    #[borrows(mut rows)]
    #[covariant]
    pub iter: Iter<'this, Vec<u8>, Vec<u8>>,
    phantom: PhantomData<V>,
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iterator for TestDBIter<'a, K, V> {
    type Item = Result<(K, V), TypedStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut out: Option<Self::Item> = None;
        let config = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding();
        self.with_mut(|fields| {
            let resp = match fields.direction {
                Direction::Forward => fields.iter.next(),
                Direction::Reverse => panic!("Reverse iteration not supported in test db"),
            };
            if let Some((raw_key, raw_value)) = resp {
                let key: K = config.deserialize(raw_key).ok().unwrap();
                let value: V = bcs::from_bytes(raw_value).ok().unwrap();
                out = Some(Ok((key, value)));
            }
        });
        out
    }
}

impl<'a, K: Serialize, V> TestDBIter<'a, K, V> {
    /// Skips all the elements that are smaller than the given key,
    /// and either lands on the key or the first one greater than
    /// the key.
    pub fn skip_to(mut self, key: &K) -> Result<Self, TypedStoreError> {
        self.with_mut(|fields| {
            let serialized_key = be_fix_int_ser(key).expect("serialization failed");
            let mut peekable = fields.iter.peekable();
            let mut peeked = peekable.peek();
            while peeked.is_some() {
                let serialized = be_fix_int_ser(peeked.unwrap()).expect("serialization failed");
                if serialized >= serialized_key {
                    break;
                } else {
                    peekable.next();
                    peeked = peekable.peek();
                }
            }
        });
        Ok(self)
    }

    /// Moves the iterator to the element given or
    /// the one prior to it if it does not exist. If there is
    /// no element prior to it, it returns an empty iterator.
    pub fn skip_prior_to(mut self, key: &K) -> Result<Self, TypedStoreError> {
        self.with_mut(|fields| {
            let serialized_key = be_fix_int_ser(key).expect("serialization failed");
            let mut peekable = fields.iter.peekable();
            let mut peeked = peekable.peek();
            while peeked.is_some() {
                let serialized = be_fix_int_ser(peeked.unwrap()).expect("serialization failed");
                if serialized > serialized_key {
                    break;
                } else {
                    peekable.next();
                    peeked = peekable.peek();
                }
            }
        });
        Ok(self)
    }

    /// Seeks to the last key in the database (at this column family).
    pub fn skip_to_last(mut self) -> Self {
        self.with_mut(|fields| {
            fields.iter.last();
        });
        self
    }

    /// Will make the direction of the iteration reverse and will
    /// create a new `RevIter` to consume. Every call to `next` method
    /// will give the next element from the end.
    pub fn reverse(mut self) -> TestDBRevIter<'a, K, V> {
        self.with_mut(|fields| {
            *fields.direction = Direction::Reverse;
        });
        TestDBRevIter::new(self)
    }
}

/// An iterator with a reverted direction to the original. The `RevIter`
/// is hosting an iteration which is consuming in the opposing direction.
/// It's not possible to do further manipulation (ex re-reverse) to the
/// iterator.
pub struct TestDBRevIter<'a, K, V> {
    iter: TestDBIter<'a, K, V>,
}

impl<'a, K, V> TestDBRevIter<'a, K, V> {
    fn new(iter: TestDBIter<'a, K, V>) -> Self {
        Self { iter }
    }
}

impl<'a, K: DeserializeOwned, V: DeserializeOwned> Iterator for TestDBRevIter<'a, K, V> {
    type Item = Result<(K, V), TypedStoreError>;

    /// Will give the next item backwards
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl<'a, K: DeserializeOwned> Iterator for TestDBKeys<'a, K> {
    type Item = Result<K, TypedStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut out: Option<Self::Item> = None;
        self.with_mut(|fields| {
            let config = bincode::DefaultOptions::new()
                .with_big_endian()
                .with_fixint_encoding();
            if let Some((raw_key, _)) = fields.iter.next() {
                let key: K = config.deserialize(raw_key).ok().unwrap();
                out = Some(Ok(key));
            }
        });
        out
    }
}

impl<'a, V: DeserializeOwned> Iterator for TestDBValues<'a, V> {
    type Item = Result<V, TypedStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut out: Option<Self::Item> = None;
        self.with_mut(|fields| {
            if let Some((_, raw_value)) = fields.iter.next() {
                let value: V = bcs::from_bytes(raw_value).ok().unwrap();
                out = Some(Ok(value));
            }
        });
        out
    }
}

impl<'a, K, V> Map<'a, K, V> for TestDB<K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    type Error = TypedStoreError;
    type Iterator = std::iter::Empty<(K, V)>;
    type SafeIterator = TestDBIter<'a, K, V>;
    type Keys = TestDBKeys<'a, K>;
    type Values = TestDBValues<'a, V>;

    fn contains_key(&self, key: &K) -> Result<bool, Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let locked = self.rows.read().unwrap();
        Ok(locked.contains_key(&raw_key))
    }

    fn get(&self, key: &K) -> Result<Option<V>, Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let locked = self.rows.read().unwrap();
        let res = locked.get(&raw_key);
        Ok(res.map(|raw_value| bcs::from_bytes(raw_value).ok().unwrap()))
    }

    fn get_raw_bytes(&self, key: &K) -> Result<Option<Vec<u8>>, Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let locked = self.rows.read().unwrap();
        let res = locked.get(&raw_key);
        Ok(res.cloned())
    }

    fn insert(&self, key: &K, value: &V) -> Result<(), Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let raw_value = bcs::to_bytes(value)?;
        let mut locked = self.rows.write().unwrap();
        locked.insert(raw_key, raw_value);
        Ok(())
    }

    fn remove(&self, key: &K) -> Result<(), Self::Error> {
        let raw_key = be_fix_int_ser(key)?;
        let mut locked = self.rows.write().unwrap();
        locked.remove(&raw_key);
        Ok(())
    }

    fn unsafe_clear(&self) -> Result<(), Self::Error> {
        let mut locked = self.rows.write().unwrap();
        locked.clear();
        Ok(())
    }

    fn schedule_delete_all(&self) -> Result<(), TypedStoreError> {
        let mut locked = self.rows.write().unwrap();
        locked.clear();
        Ok(())
    }

    fn is_empty(&self) -> bool {
        let locked = self.rows.read().unwrap();
        locked.is_empty()
    }

    fn unbounded_iter(&'a self) -> Self::Iterator {
        unimplemented!("umplemented API");
    }

    fn iter_with_bounds(
        &'a self,
        _lower_bound: Option<K>,
        _upper_bound: Option<K>,
    ) -> Self::Iterator {
        unimplemented!("umplemented API");
    }

    fn range_iter(&'a self, _range: impl RangeBounds<K>) -> Self::Iterator {
        unimplemented!("umplemented API");
    }

    fn safe_iter(&'a self) -> Self::SafeIterator {
        TestDBIterBuilder {
            rows: self.rows.read().unwrap(),
            iter_builder: |rows: &mut RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>| rows.iter(),
            phantom: PhantomData,
            direction: Direction::Forward,
        }
        .build()
    }

    fn keys(&'a self) -> Self::Keys {
        TestDBKeysBuilder {
            rows: self.rows.read().unwrap(),
            iter_builder: |rows: &mut RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>| rows.iter(),
            phantom: PhantomData,
        }
        .build()
    }

    fn values(&'a self) -> Self::Values {
        TestDBValuesBuilder {
            rows: self.rows.read().unwrap(),
            iter_builder: |rows: &mut RwLockReadGuard<'a, BTreeMap<Vec<u8>, Vec<u8>>>| rows.iter(),
            phantom: PhantomData,
        }
        .build()
    }

    fn try_catch_up_with_primary(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<J, K, U, V> TryExtend<(J, U)> for TestDB<K, V>
where
    J: Borrow<K>,
    U: Borrow<V>,
    K: Serialize,
    V: Serialize,
{
    type Error = TypedStoreError;

    fn try_extend<T>(&mut self, iter: &mut T) -> Result<(), Self::Error>
    where
        T: Iterator<Item = (J, U)>,
    {
        let mut wb = self.batch();
        wb.insert_batch(self, iter)?;
        wb.write()
    }

    fn try_extend_from_slice(&mut self, slice: &[(J, U)]) -> Result<(), Self::Error> {
        let slice_of_refs = slice.iter().map(|(k, v)| (k.borrow(), v.borrow()));
        let mut wb = self.batch();
        wb.insert_batch(self, slice_of_refs)?;
        wb.write()
    }
}

pub type DeleteBatchPayload = (
    Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    String,
    Vec<Vec<u8>>,
);
pub type DeleteRangePayload = (
    Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    String,
    (Vec<u8>, Vec<u8>),
);
pub type InsertBatchPayload = (
    Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    String,
    Vec<(Vec<u8>, Vec<u8>)>,
);
type DBAndName = (Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>, String);

pub enum WriteBatchOp {
    DeleteBatch(DeleteBatchPayload),
    DeleteRange(DeleteRangePayload),
    InsertBatch(InsertBatchPayload),
}

#[derive(Default)]
pub struct TestDBWriteBatch {
    pub ops: VecDeque<WriteBatchOp>,
}

#[self_referencing]
pub struct DBLocked {
    db: Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>,
    #[borrows(db)]
    #[covariant]
    db_guard: RwLockWriteGuard<'this, BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl TestDBWriteBatch {
    pub fn write(self) -> Result<(), TypedStoreError> {
        let mut dbs: Vec<DBAndName> = self
            .ops
            .iter()
            .map(|op| match op {
                WriteBatchOp::DeleteBatch((db, name, _)) => (db.clone(), name.clone()),
                WriteBatchOp::DeleteRange((db, name, _)) => (db.clone(), name.clone()),
                WriteBatchOp::InsertBatch((db, name, _)) => (db.clone(), name.clone()),
            })
            .collect();
        dbs.sort_by_key(|(_k, v)| v.clone());
        dbs.dedup_by_key(|(_k, v)| v.clone());
        // lock all databases
        let mut db_locks = HashMap::new();
        dbs.iter().for_each(|(db, name)| {
            if !db_locks.contains_key(name) {
                db_locks.insert(
                    name.clone(),
                    DBLockedBuilder {
                        db: db.clone(),
                        db_guard_builder: |db: &Arc<RwLock<BTreeMap<Vec<u8>, Vec<u8>>>>| {
                            db.write().unwrap()
                        },
                    }
                    .build(),
                );
            }
        });
        self.ops.iter().for_each(|op| match op {
            WriteBatchOp::DeleteBatch((_, id, keys)) => {
                let locked = db_locks.get_mut(id).unwrap();
                locked.with_db_guard_mut(|db| {
                    keys.iter().for_each(|key| {
                        db.remove(key);
                    });
                });
            }
            WriteBatchOp::DeleteRange((_, id, (from, to))) => {
                let locked = db_locks.get_mut(id).unwrap();
                locked.with_db_guard_mut(|db| {
                    db.retain(|k, _| k < from || k >= to);
                });
            }
            WriteBatchOp::InsertBatch((_, id, key_values)) => {
                let locked = db_locks.get_mut(id).unwrap();
                locked.with_db_guard_mut(|db| {
                    key_values.iter().for_each(|(k, v)| {
                        db.insert(k.clone(), v.clone());
                    });
                });
            }
        });
        // unlock in the reverse order
        dbs.iter().rev().for_each(|(_db, id)| {
            if db_locks.contains_key(id) {
                db_locks.remove(id);
            }
        });
        Ok(())
    }
    /// Deletes a set of keys given as an iterator
    pub fn delete_batch<J: Borrow<K>, K: Serialize, V>(
        &mut self,
        db: &TestDB<K, V>,
        purged_vals: impl IntoIterator<Item = J>,
    ) -> Result<(), TypedStoreError> {
        self.ops.push_back(WriteBatchOp::DeleteBatch((
            db.rows.clone(),
            db.name.clone(),
            purged_vals
                .into_iter()
                .map(|key| be_fix_int_ser(&key.borrow()).unwrap())
                .collect(),
        )));
        Ok(())
    }
    /// Deletes a range of keys between `from` (inclusive) and `to` (non-inclusive)
    pub fn delete_range<K: Serialize, V>(
        &mut self,
        db: &TestDB<K, V>,
        from: &K,
        to: &K,
    ) -> Result<(), TypedStoreError> {
        let raw_from = be_fix_int_ser(from).unwrap();
        let raw_to = be_fix_int_ser(to).unwrap();
        self.ops.push_back(WriteBatchOp::DeleteRange((
            db.rows.clone(),
            db.name.clone(),
            (raw_from, raw_to),
        )));
        Ok(())
    }
    /// inserts a range of (key, value) pairs given as an iterator
    pub fn insert_batch<J: Borrow<K>, K: Serialize, U: Borrow<V>, V: Serialize>(
        &mut self,
        db: &TestDB<K, V>,
        new_vals: impl IntoIterator<Item = (J, U)>,
    ) -> Result<(), TypedStoreError> {
        self.ops.push_back(WriteBatchOp::InsertBatch((
            db.rows.clone(),
            db.name.clone(),
            new_vals
                .into_iter()
                .map(|(key, value)| {
                    (
                        be_fix_int_ser(&key.borrow()).unwrap(),
                        bcs::to_bytes(&value.borrow()).unwrap(),
                    )
                })
                .collect(),
        )));
        Ok(())
    }
}
