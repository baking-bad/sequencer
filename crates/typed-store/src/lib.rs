// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod traits;
pub use traits::Map;
pub mod metrics;
pub mod rocks;
pub mod sally;
pub mod test_db;
pub use metrics::DBMetrics;

#[non_exhaustive]
#[derive(Error, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Ord, PartialOrd)]
pub enum TypedStoreError {
    #[error("rocksdb error: {0}")]
    RocksDBError(String),
    #[error("(de)serialization error: {0}")]
    SerializationError(String),
    #[error("the column family {0} was not registered with the database")]
    UnregisteredColumn(String),
    #[error("a batch operation can't operate across databases")]
    CrossDBBatch,
    #[error("Metric reporting thread failed with error")]
    MetricsReporting,
    #[error("Transaction should be retried")]
    RetryableTransactionError,
}
