use std::{future::Future, sync::OnceLock};

pub use tokio::{
    sync::Semaphore,
    sync::mpsc::{Receiver, Sender, channel},
    task::JoinHandle,
};

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static BLOCKING_LIMITER: OnceLock<Semaphore> = OnceLock::new();
const MAX_CONCURRENT_BLOCKING_LOADS: usize = 1;

fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("codux-runtime")
            .build()
            .expect("failed to create Codux async runtime")
    })
}

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(future)
}

pub fn spawn_blocking<F, R>(function: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    runtime().spawn_blocking(function)
}

pub async fn run_limited_blocking<F, R>(function: F) -> Result<R, tokio::task::JoinError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let permit = BLOCKING_LIMITER
        .get_or_init(|| Semaphore::new(MAX_CONCURRENT_BLOCKING_LOADS))
        .acquire()
        .await
        .expect("Codux blocking limiter closed");
    let result = spawn_blocking(function).await;
    drop(permit);
    result
}

pub fn block_on<F>(future: F) -> F::Output
where
    F: Future + Send,
    F::Output: Send,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        drop(handle);
        std::thread::scope(|scope| {
            scope
                .spawn(|| runtime().block_on(future))
                .join()
                .expect("Codux async runtime worker panicked")
        })
    } else {
        runtime().block_on(future)
    }
}
