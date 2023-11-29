// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(any(msim, fail_points))]
use futures::future::BoxFuture;
#[cfg(any(msim, fail_points))]
use std::collections::HashMap;
#[cfg(any(msim, fail_points))]
use std::future::Future;
#[cfg(any(msim, fail_points))]
use std::sync::Arc;

#[cfg(any(msim, fail_points))]
type FpCallback = dyn Fn() -> Option<BoxFuture<'static, ()>> + Send + Sync + 'static;

#[cfg(any(msim, fail_points))]
type FpMap = HashMap<&'static str, Arc<FpCallback>>;

#[cfg(msim)]
fn with_fp_map<T>(func: impl FnOnce(&mut FpMap) -> T) -> T {
    thread_local! {
        static MAP: std::cell::RefCell<FpMap> = Default::default();
    }

    MAP.with(|val| func(&mut val.borrow_mut()))
}

#[cfg(all(not(msim), fail_points))]
fn with_fp_map<T>(func: impl FnOnce(&mut FpMap) -> T) -> T {
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static MAP: Lazy<Mutex<FpMap>> = Lazy::new(Default::default);
    let mut map = MAP.lock().unwrap();
    func(&mut map)
}

#[cfg(any(msim, fail_points))]
fn get_callback(identifier: &'static str) -> Option<Arc<FpCallback>> {
    with_fp_map(|map| map.get(identifier).cloned())
}

#[cfg(any(msim, fail_points))]
pub fn handle_fail_point(identifier: &'static str) {
    if let Some(callback) = get_callback(identifier) {
        tracing::error!("hit failpoint {}", identifier);
        assert!(
            callback().is_none(),
            "sync failpoint must not return future"
        );
    }
}

#[cfg(any(msim, fail_points))]
pub async fn handle_fail_point_async(identifier: &'static str) {
    if let Some(callback) = get_callback(identifier) {
        tracing::error!("hit async failpoint {}", identifier);
        let fut = callback().expect("async callback must return future");
        fut.await;
    }
}

#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point {
    ($tag: expr) => {
        $crate::handle_fail_point($tag)
    };
}

#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point_async {
    ($tag: expr) => {
        $crate::handle_fail_point_async($tag).await
    };
}

#[cfg(not(any(msim, fail_points)))]
#[macro_export]
macro_rules! fail_point {
    ($tag: expr) => {};
}

#[cfg(not(any(msim, fail_points)))]
#[macro_export]
macro_rules! fail_point_async {
    ($tag: expr) => {};
}