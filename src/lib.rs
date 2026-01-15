pub mod cloudflare;
pub mod config;
pub mod errors;
pub mod ip_provider;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static GLOBAL_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    pub fn global_lock() -> MutexGuard<'static, ()> {
        GLOBAL_TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap()
    }
}
