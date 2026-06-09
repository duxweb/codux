use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

pub trait SerialJob: Send + 'static {
    fn queue_key(&self) -> String;
}

pub struct SerialJobQueue<J>
where
    J: SerialJob,
{
    shared: Arc<QueueShared<J>>,
}

impl<J> Clone for SerialJobQueue<J>
where
    J: SerialJob,
{
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

struct QueueShared<J>
where
    J: SerialJob,
{
    state: Mutex<QueueState<J>>,
    signal: Condvar,
}

struct QueueState<J>
where
    J: SerialJob,
{
    pending_keys: VecDeque<String>,
    pending_jobs: HashMap<String, J>,
    running_keys: HashSet<String>,
    unkeyed_jobs: VecDeque<J>,
}

impl<J> Default for QueueState<J>
where
    J: SerialJob,
{
    fn default() -> Self {
        Self {
            pending_keys: VecDeque::new(),
            pending_jobs: HashMap::new(),
            running_keys: HashSet::new(),
            unkeyed_jobs: VecDeque::new(),
        }
    }
}

impl<J> SerialJobQueue<J>
where
    J: SerialJob,
{
    pub fn new(name: impl Into<String>, handler: impl Fn(J) + Send + 'static) -> Self {
        let shared = Arc::new(QueueShared {
            state: Mutex::new(QueueState::default()),
            signal: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name(name.into())
            .spawn(move || run_worker(worker_shared, handler))
            .expect("failed to spawn serial job queue worker");
        Self { shared }
    }

    pub fn submit(&self, job: J) {
        let key = job.queue_key();
        let Ok(mut state) = self.shared.state.lock() else {
            return;
        };
        if key.trim().is_empty() {
            state.unkeyed_jobs.push_back(job);
            self.shared.signal.notify_one();
            return;
        }
        let is_running = state.running_keys.contains(&key);
        let is_already_pending = state.pending_jobs.contains_key(&key);
        state.pending_jobs.insert(key.clone(), job);
        if !is_running && !is_already_pending {
            state.pending_keys.push_back(key);
            self.shared.signal.notify_one();
        }
    }
}

fn run_worker<J>(shared: Arc<QueueShared<J>>, handler: impl Fn(J) + Send + 'static)
where
    J: SerialJob,
{
    loop {
        let (job, key) = next_job(&shared);
        let _ = catch_unwind(AssertUnwindSafe(|| handler(job)));
        finish_job(&shared, key);
    }
}

fn next_job<J>(shared: &QueueShared<J>) -> (J, Option<String>)
where
    J: SerialJob,
{
    loop {
        let mut state = shared
            .signal
            .wait_while(
                shared
                    .state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()),
                |state| state.unkeyed_jobs.is_empty() && state.pending_keys.is_empty(),
            )
            .unwrap_or_else(|error| error.into_inner());
        if let Some(job) = state.unkeyed_jobs.pop_front() {
            return (job, None);
        }
        while let Some(key) = state.pending_keys.pop_front() {
            let Some(job) = state.pending_jobs.remove(&key) else {
                continue;
            };
            state.running_keys.insert(key.clone());
            return (job, Some(key));
        }
    }
}

fn finish_job<J>(shared: &QueueShared<J>, key: Option<String>)
where
    J: SerialJob,
{
    let Some(key) = key else {
        return;
    };
    let Ok(mut state) = shared.state.lock() else {
        return;
    };
    state.running_keys.remove(&key);
    if state.pending_jobs.contains_key(&key) {
        state.pending_keys.push_back(key);
        shared.signal.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct TestJob {
        key: String,
        value: u8,
    }

    impl SerialJob for TestJob {
        fn queue_key(&self) -> String {
            self.key.clone()
        }
    }

    #[test]
    fn keeps_latest_keyed_job_while_current_job_is_running() {
        let values = Arc::new((Mutex::new(Vec::<u8>::new()), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let handler_values = Arc::clone(&values);
        let handler_release = Arc::clone(&release);
        let queue = SerialJobQueue::new("codux-test-serial-job-worker", move |job: TestJob| {
            let (values, values_signal) = &*handler_values;
            values.lock().unwrap().push(job.value);
            values_signal.notify_all();
            if job.value == 1 {
                let (release, release_signal) = &*handler_release;
                let release = release.lock().unwrap();
                drop(
                    release_signal
                        .wait_while(release, |released| !*released)
                        .unwrap(),
                );
            }
        });

        queue.submit(TestJob {
            key: "repo".to_string(),
            value: 1,
        });
        wait_for_values(&values, 1);

        queue.submit(TestJob {
            key: "repo".to_string(),
            value: 2,
        });
        queue.submit(TestJob {
            key: "repo".to_string(),
            value: 3,
        });

        {
            let (release, release_signal) = &*release;
            *release.lock().unwrap() = true;
            release_signal.notify_all();
        }

        assert_eq!(wait_for_values(&values, 2), vec![1, 3]);
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(values.0.lock().unwrap().as_slice(), &[1, 3]);
    }

    fn wait_for_values(values: &Arc<(Mutex<Vec<u8>>, Condvar)>, expected_len: usize) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let (values, signal) = &**values;
        let mut guard = values.lock().unwrap();
        while guard.len() < expected_len {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for {expected_len} queued values"
            );
            let (next_guard, _) = signal.wait_timeout(guard, remaining).unwrap();
            guard = next_guard;
        }
        guard.clone()
    }
}
