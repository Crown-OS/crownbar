//! The bar's async home.
//!
//! bluer and nmrs are both zbus-on-tokio, and every blocking probe — sysfs,
//! i2c, the battery crate — goes to this runtime's blocking pool. The event
//! loop thread does none of it.

use tokio::runtime::{Builder, Runtime};

/// Two workers: these services are IO-bound with effectively no CPU, and a
/// default runtime would spawn one worker per core to idle.
const WORKERS: usize = 2;

pub fn build() -> std::io::Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(WORKERS)
        .thread_name("crownbar-rt")
        .enable_io()
        .enable_time()
        .build()
}
