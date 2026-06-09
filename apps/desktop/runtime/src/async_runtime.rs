use std::{
    any::Any,
    cmp::Ordering as CmpOrdering,
    collections::BinaryHeap,
    future::Future,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub use tokio::{
    sync::mpsc::{Receiver, Sender, channel},
    sync::{Notify, Semaphore, oneshot},
    task::JoinHandle,
};

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static BLOCKING_LIMITER: OnceLock<Semaphore> = OnceLock::new();
static PRIORITY_BLOCKING_QUEUE: OnceLock<Arc<PriorityBlockingQueue>> = OnceLock::new();
const MAX_CONCURRENT_BLOCKING_LOADS: usize = 1;
pub const BLOCKING_PRIORITY_BACKGROUND: u64 = 0;
pub const BLOCKING_PRIORITY_NORMAL: u64 = 1_000;
pub const BLOCKING_PRIORITY_FOREGROUND: u64 = 1_000_000;

struct PriorityBlockingQueue {
    jobs: Mutex<BinaryHeap<PriorityBlockingJob>>,
    notify: Notify,
    sequence: AtomicU64,
    started: AtomicBool,
    running: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockingQueueStatus {
    pub queued: usize,
    pub running: usize,
}

struct PriorityBlockingJob {
    priority: u64,
    sequence: u64,
    queued_at: std::time::Instant,
    run: Box<dyn FnOnce() -> Box<dyn Any + Send> + Send>,
    result: oneshot::Sender<Result<Box<dyn Any + Send>, tokio::task::JoinError>>,
}

impl PartialEq for PriorityBlockingJob {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for PriorityBlockingJob {}

impl PartialOrd for PriorityBlockingJob {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityBlockingJob {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

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
    run_limited_blocking_with_priority(BLOCKING_PRIORITY_NORMAL, function).await
}

pub async fn run_limited_blocking_with_priority<F, R>(
    priority: u64,
    function: F,
) -> Result<R, tokio::task::JoinError>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let queue = priority_blocking_queue();
    let (result, receiver) = oneshot::channel();
    let sequence = queue.sequence.fetch_add(1, Ordering::Relaxed);
    queue
        .jobs
        .lock()
        .expect("Codux priority blocking queue poisoned")
        .push(PriorityBlockingJob {
            priority,
            sequence,
            queued_at: std::time::Instant::now(),
            run: Box::new(move || Box::new(function()) as Box<dyn Any + Send>),
            result,
        });
    queue.notify.notify_one();
    let boxed = receiver
        .await
        .expect("Codux priority blocking worker stopped")?;
    Ok(*boxed
        .downcast::<R>()
        .expect("Codux priority blocking result type mismatch"))
}

pub fn blocking_queue_status() -> BlockingQueueStatus {
    let Some(queue) = PRIORITY_BLOCKING_QUEUE.get() else {
        return BlockingQueueStatus::default();
    };
    let queued = queue
        .jobs
        .lock()
        .expect("Codux priority blocking queue poisoned")
        .len();
    BlockingQueueStatus {
        queued,
        running: queue.running.load(Ordering::Relaxed) as usize,
    }
}

pub async fn run_semaphore_limited_blocking<F, R>(function: F) -> Result<R, tokio::task::JoinError>
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

fn priority_blocking_queue() -> Arc<PriorityBlockingQueue> {
    let queue = PRIORITY_BLOCKING_QUEUE
        .get_or_init(|| {
            Arc::new(PriorityBlockingQueue {
                jobs: Mutex::new(BinaryHeap::new()),
                notify: Notify::new(),
                sequence: AtomicU64::new(0),
                started: AtomicBool::new(false),
                running: AtomicU64::new(0),
            })
        })
        .clone();
    if !queue.started.swap(true, Ordering::AcqRel) {
        start_priority_blocking_worker(queue.clone());
    }
    queue
}

fn start_priority_blocking_worker(queue: Arc<PriorityBlockingQueue>) {
    runtime().spawn(async move {
        loop {
            let job = loop {
                if let Some(job) = queue
                    .jobs
                    .lock()
                    .expect("Codux priority blocking queue poisoned")
                    .pop()
                {
                    break job;
                }
                queue.notify.notified().await;
            };
            crate::runtime_trace::runtime_trace(
                "blocking-queue",
                &format!(
                    "worker_start priority={} sequence={} queue_wait_ms={}",
                    job.priority,
                    job.sequence,
                    job.queued_at.elapsed().as_millis()
                ),
            );
            let started_at = std::time::Instant::now();
            queue.running.fetch_add(1, Ordering::Relaxed);
            let result = runtime().spawn_blocking(job.run).await;
            queue.running.fetch_sub(1, Ordering::Relaxed);
            crate::runtime_trace::runtime_trace(
                "blocking-queue",
                &format!(
                    "worker_done priority={} sequence={} elapsed_ms={}",
                    job.priority,
                    job.sequence,
                    started_at.elapsed().as_millis()
                ),
            );
            let _ = job.result.send(result);
        }
    });
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
