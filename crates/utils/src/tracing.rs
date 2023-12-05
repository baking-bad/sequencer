// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;
use std::io::{stderr, Write};
use tracing::metadata::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, reload, EnvFilter, Layer};

pub fn setup_tracing(tracing_level: &str, network_tracing_level: &str) -> WorkerGuard {
    let (nb_output, worker_guard) = tracing_appender::non_blocking(stderr());

    let (log_filter, _) = reload::Layer::new(
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(
                format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level},quinn={network_tracing_level}"))));

    let fmt_layer = fmt::layer()
        .with_writer(nb_output)
        .with_filter(log_filter)
        .boxed();

    let mut layers = Vec::new();
    layers.push(fmt_layer);

    let subscriber = tracing_subscriber::registry().with(layers);
    ::tracing::subscriber::set_global_default(subscriber)
        .expect("unable to initialize tracing");

    set_panic_hook(false);

    worker_guard
}

pub fn setup_tracing_for_tests() {
    static LOGGER: Lazy<()> = Lazy::new(|| {
        let subscriber = ::tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with_file(true)
            .with_line_number(true)
            .with_test_writer()
            .finish();

        ::tracing::subscriber::set_global_default(subscriber)
            .expect("unable to initialize tracing for tests");
    });

    Lazy::force(&LOGGER);
}

// NOTE: this function is copied from tracing's panic_hook example
fn set_panic_hook(crash_on_panic: bool) {
    let default_panic_handler = std::panic::take_hook();

    // Set a panic hook that records the panic as a `tracing` event at the
    // `ERROR` verbosity level.
    //
    // If we are currently in a span when the panic occurred, the logged event
    // will include the current span, allowing the context in which the panic
    // occurred to be recorded.
    std::panic::set_hook(Box::new(move |panic| {
        // If the panic has a source location, record it as structured fields.
        if let Some(location) = panic.location() {
            // On nightly Rust, where the `PanicInfo` type also exposes a
            // `message()` method returning just the message, we could record
            // just the message instead of the entire `fmt::Display`
            // implementation, avoiding the duplicated location
            tracing::error!(
                message = %panic,
                panic.file = location.file(),
                panic.line = location.line(),
                panic.column = location.column(),
            );
        } else {
            tracing::error!(message = %panic);
        }

        default_panic_handler(panic);

        // We're panicking so we can't do anything about the flush failing
        let _ = std::io::stderr().flush();
        let _ = std::io::stdout().flush();

        if crash_on_panic {
            // Kill the process
            std::process::exit(12);
        }
    }));
}