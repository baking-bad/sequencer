// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

mod aggregators;
mod certificate_fetcher;
mod certifier;
pub mod consensus;
mod primary;
mod proposer;
mod state_handler;
mod synchronizer;

mod metrics;

pub use crate::{
    metrics::PrimaryChannelMetrics,
    primary::{Primary, CHANNEL_CAPACITY, NUM_SHUTDOWN_RECEIVERS},
};
