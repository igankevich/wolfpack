use std::sync::LazyLock;

use parking_lot::Mutex;
use parking_lot::MutexGuard;

#[must_use]
pub fn prevent_concurrency(bucket: &str) -> MutexGuard<()> {
    for (name, mutex) in BUCKETS.iter() {
        if name == &bucket {
            return mutex.lock();
        }
    }
    panic!("no such concurrency bucket: {:?}", bucket);
}

static BUCKETS: LazyLock<Vec<(&'static str, Mutex<()>)>> =
    LazyLock::new(|| vec![("freebsd-pkg", Mutex::new(()))]);
