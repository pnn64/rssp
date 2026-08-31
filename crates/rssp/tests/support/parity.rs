use std::sync::Arc;

use tokio::runtime::Builder;
use tokio::task::JoinSet;

pub fn run<T, R, F>(tests: Vec<T>, threads: Option<usize>, check: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
{
    let threads = threads
        .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
        .unwrap_or(1)
        .max(1)
        .min(tests.len().max(1));
    let runtime = Builder::new_current_thread()
        .build()
        .expect("Tokio parity runtime should initialize");

    runtime.block_on(run_inner(tests, threads, check))
}

async fn run_inner<T, R, F>(tests: Vec<T>, threads: usize, check: F) -> Vec<R>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
{
    let len = tests.len();
    let check = Arc::new(check);
    let mut pending = tests.into_iter().enumerate();
    let mut running = JoinSet::new();
    let mut results = Vec::with_capacity(len);

    for (index, test) in pending.by_ref().take(threads) {
        let check = Arc::clone(&check);
        running.spawn_blocking(move || (index, check(test)));
    }

    while let Some(result) = running.join_next().await {
        results.push(result.expect("parity worker should not panic"));
        if let Some((index, test)) = pending.next() {
            let check = Arc::clone(&check);
            running.spawn_blocking(move || (index, check(test)));
        }
    }

    results.sort_unstable_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}
