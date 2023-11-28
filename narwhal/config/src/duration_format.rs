// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Allow us to deserialize Duration values in a more human friendly format
//! (e.x in json files). The deserialization supports to time units:
//! * milliseconds
//! * seconds
//!
//! To identify milliseconds then a string of the following format should be
//! provided: Nms, for example "20ms", or "2_000ms".
//!
//! To identify seconds, then the following format should be used:
//! Ns, for example "20s", or "10_000s".
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::Duration;

pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    if let Some(milis) = s.strip_suffix("ms") {
        return milis
            .replace('_', "")
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|e| serde::de::Error::custom(e.to_string()));
    } else if let Some(seconds) = s.strip_suffix('s') {
        return seconds
            .replace('_', "")
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|e| serde::de::Error::custom(e.to_string()));
    }

    Err(serde::de::Error::custom(format!(
        "Wrong format detected: {s}. It should be number in milliseconds, e.x 10ms"
    )))
}

pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    format!("{}ms", duration.as_millis()).serialize(serializer)
}
