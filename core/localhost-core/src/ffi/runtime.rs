//! The core's tokio runtime. Swift polls the futures UniFFI hands it on its
//! own threads, so everything that needs tokio (timers, process I/O, spawned
//! tasks) is moved onto this runtime and awaited through its `JoinHandle`.

use std::future::Future;
use std::sync::LazyLock;

use tokio::runtime::{EnterGuard, Runtime};

static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .thread_name("localhost-core")
        .enable_all()
        .build()
        .expect("the core's runtime starts")
});

pub(super) async fn on_runtime<F>(future: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match RUNTIME.spawn(future).await {
        Ok(output) => output,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// Runs blocking work (file I/O, the Keychain) off the caller's thread.
pub(super) async fn blocking<F, T>(work: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    match RUNTIME.spawn_blocking(work).await {
        Ok(output) => output,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// Makes the runtime current for a synchronous call that spawns tasks.
pub(super) fn enter() -> EnterGuard<'static> {
    RUNTIME.enter()
}
