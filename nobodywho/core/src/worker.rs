use core::fmt;
use std::{
    future::Future,
    io::ErrorKind,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
};

use futures::future::BoxFuture;

/// A worker that asynchronously processes CPU-blocking tasks.
///
/// This is similar to [`tokio::task::spawn_blocking`], but doesn't require
/// that a Tokio runtime is running (which is important for foreign languages'
/// `async` runtimes).
pub enum AsyncWorker {
    /// A native OS thread.
    ///
    /// This is fundamentally what we must use in languages that don't have
    /// their own thread pools such as Godot or React Native.
    Thread {
        tx: Option<tokio::sync::mpsc::Sender<Task>>,
        join_handle: Option<std::thread::JoinHandle<()>>,
    },

    /// A custom worker.
    ///
    /// This is meant to be used for sending the task to a language-specific
    /// worker thread pool, which usually works better with their `async`
    /// support.
    Custom {
        spawn_blocking: Box<dyn Fn(Task) -> BoxFuture<'static, ()> + Send + Sync + 'static>,
    },

    /// A worker that runs blockingly on the current thread.
    ///
    /// This is used on e.g. Emscripten, which doesn't have worker threads
    /// that we can offload to (at least not without using WASM workers /
    /// pthreads, which aren't guaranteed available).
    Blocking,
}

/// A boxed closure.
//
// Once allocators stabilize, we could probably optimize this with a local
// allocator (but it's probably not worth it either way).
pub type Task = Box<dyn FnOnce() + Send + 'static>;

impl AsyncWorker {
    /// Spawn a new worker thread.
    ///
    /// Falls back to a blocking worker if that's unsupported by the platform.
    #[allow(clippy::new_without_default, reason = "does expensive work")]
    pub fn new() -> Self {
        // We only have a single worker thread, and can never execute more
        // than one task at a time, so there's no reason to fill up this
        // buffer more than necessary. Additionally, having only space for a
        // single task means that cancellation works better.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Task>(1);

        let res = std::thread::Builder::new()
            .name("nobodywho worker thread".into())
            .spawn(move || {
                while let Some(task) = rx.blocking_recv() {
                    task();
                }
            });

        match res {
            Ok(join_handle) => Self::Thread {
                tx: Some(tx),
                join_handle: Some(join_handle),
            },
            Err(e) if e.kind() == ErrorKind::Unsupported => Self::Blocking,
            Err(e) => panic!("failed spawning worker thread: {e}"),
        }
    }

    /// Create a new worker with a custom `spawn_blocking` function.
    pub fn with_custom<F: Future<Output = ()> + Send + 'static>(
        spawn_blocking: impl Fn(Task) -> F + Send + Sync + 'static,
    ) -> Self {
        Self::Custom {
            spawn_blocking: Box::new(move |task| Box::pin(spawn_blocking(task))),
        }
    }

    /// Run the closure on the worker thread, and wait for it asynchronously.
    ///
    /// On WASM / Emscripten (and similar targets that don't have threads),
    /// this runs the closure synchronously.
    ///
    /// For this reason, this function should only be used for piecewise tasks
    /// like the generation of one token (e.g. don't generate all tokens at
    /// once inside the closure).
    pub async fn run<R: Send + 'static>(&self, task: impl FnOnce() -> R + Send + 'static) -> R {
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        // Re-enter the current span when running the queued closure.
        let span = tracing::Span::current();
        let task: Task = Box::new(move || {
            let _enter = span.enter();
            let result = catch_unwind(AssertUnwindSafe(task));
            // Ignore error from sending, this likely means that the
            // `result_tx.await` was cancelled.
            let _ = result_tx.send(result);
        });

        match self {
            Self::Thread { tx, .. } => tx
                .as_ref()
                .expect("dropped")
                .send(task)
                .await
                .expect("worker thread has stopped"),
            Self::Custom { spawn_blocking } => spawn_blocking(task).await,
            Self::Blocking => task(),
        }

        match result_rx.await.expect("worker failed to send back result") {
            Ok(value) => value,
            Err(panic) => resume_unwind(panic),
        }
    }
}

impl Drop for AsyncWorker {
    fn drop(&mut self) {
        if let Self::Thread { tx, join_handle } = self {
            // Tell the worker thread to stop.
            drop(tx.take().expect("double-dropped"));

            // Wait for it to actually stop.
            //
            // NOTE: This blocks waiting for any cancelled tasks to finish,
            // which is suboptimal, but it's also not nice to end up with
            // tasks still executing in the background after the worker is
            // gone (especially since those tasks may be holding locks etc.,
            // which could make debugging very confusing).
            //
            // The proper way to propagate cancellation is probably to set an
            // explicit flag that the thread can check to see whether it needs
            // to cancel.
            let join_handle = join_handle.take().expect("double-dropped");
            if let Err(e) = join_handle.join() {
                // Re-raise any panics that might have happened. We don't
                // expect there to be any of these, they should be handled
                // with `catch_unwind` in `run`, but let's make sure.
                resume_unwind(e);
            }
        }
    }
}

impl fmt::Debug for AsyncWorker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsyncWorker::Thread { .. } => write!(f, "AsyncWorker::Thread {{ .. }}"),
            AsyncWorker::Custom { .. } => write!(f, "AsyncWorker::Custom {{ .. }}"),
            AsyncWorker::Blocking => write!(f, "AsyncWorker::Blocking"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    #[test]
    fn worker_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AsyncWorker>();
    }

    fn all_workers() -> Vec<(&'static str, AsyncWorker)> {
        vec![
            ("Thread", AsyncWorker::new()),
            (
                "Custom",
                AsyncWorker::with_custom(|task| async move {
                    task();
                }),
            ),
            ("Blocking", AsyncWorker::Blocking),
        ]
    }

    #[test]
    fn runs_task_without_tokio_runtime() {
        for (name, worker) in all_workers() {
            let value = futures::executor::block_on(worker.run(|| 40 + 2));
            assert_eq!(value, 42, "variant: {name}");
        }
    }

    #[tokio::test]
    async fn runs_task_with_tokio_runtime() {
        for (name, worker) in all_workers() {
            let value = worker.run(|| String::from("hello")).await;
            assert_eq!(value, "hello", "variant: {name}");
        }
    }

    #[test]
    fn panic_is_propagated_to_caller_with_payload() {
        for (name, worker) in all_workers() {
            let result = catch_unwind(AssertUnwindSafe(|| {
                futures::executor::block_on(worker.run(|| -> () { panic!("boom") }))
            }));
            let payload = result.expect_err("panic should propagate to caller");
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"boom"),
                "variant: {name}"
            );
        }
    }

    #[test]
    fn is_usable_after_task_panic() {
        for (name, worker) in all_workers() {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                futures::executor::block_on(worker.run(|| -> () { panic!("boom") }))
            }));
            assert_eq!(
                futures::executor::block_on(worker.run(|| 7)),
                7,
                "variant: {name}"
            );
            // Dropping must not re-raise the already-handled panic.
            drop(worker);
        }
    }

    #[test]
    fn sequential_tasks_run_in_order() {
        for (name, worker) in all_workers() {
            let log = Arc::new(Mutex::new(Vec::new()));
            for i in 0..10 {
                let log = log.clone();
                futures::executor::block_on(worker.run(move || log.lock().unwrap().push(i)));
            }
            assert_eq!(
                *log.lock().unwrap(),
                (0..10).collect::<Vec<_>>(),
                "variant: {name}"
            );
        }
    }

    #[test]
    fn drop_without_running_any_task() {
        for (_name, worker) in all_workers() {
            drop(worker);
        }
    }

    // ── Thread-specific tests ───────────────────────────────────────

    #[tokio::test]
    #[cfg(not(target_os = "emscripten"))]
    async fn concurrent_callers_are_serialized() {
        let worker = Arc::new(AsyncWorker::new());
        let running = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_running = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let handles: Vec<_> = (0..16)
            .map(|i| {
                let worker = worker.clone();
                let running = running.clone();
                let max_running = max_running.clone();
                tokio::spawn(async move {
                    worker
                        .run(move || {
                            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                            max_running.fetch_max(now, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(2));
                            running.fetch_sub(1, Ordering::SeqCst);
                            i
                        })
                        .await
                })
            })
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }
        assert_eq!(results, (0..16).collect::<Vec<_>>());
        assert_eq!(max_running.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    #[cfg(not(target_os = "emscripten"))]
    async fn cancelled_run_still_completes_task_and_worker_keeps_working() {
        let worker = AsyncWorker::new();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        {
            let done = done.clone();
            let fut = worker.run(move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                done.fetch_add(1, Ordering::SeqCst);
            });
            // Poll until the task has been handed to the worker, then cancel.
            tokio::select! {
                _ = fut => unreachable!("task is blocked on release_rx"),
                _ = tokio::task::spawn_blocking(move || started_rx.recv().unwrap()) => {}
            }
        }

        release_tx.send(()).unwrap();
        // The next task queues behind the cancelled one, so once it returns
        // the cancelled task must have finished.
        assert_eq!(worker.run(|| 1).await, 1);
        assert_eq!(done.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    #[cfg(not(target_os = "emscripten"))]
    async fn drop_waits_for_in_flight_task() {
        let worker = AsyncWorker::new();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        {
            let done = done.clone();
            let fut = worker.run(move || {
                started_tx.send(()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(50));
                done.fetch_add(1, Ordering::SeqCst);
            });
            tokio::select! {
                _ = fut => unreachable!("task sleeps longer than it takes to start"),
                _ = tokio::task::spawn_blocking(move || started_rx.recv().unwrap()) => {}
            }
        }

        // The `run` future was cancelled, but the task is still running on the
        // worker thread. Dropping the worker must join it.
        drop(worker);
        assert_eq!(done.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn custom_spawn_is_actually_async() {
        // Verify the spawn function receives a Future, not just a closure
        // that runs synchronously — the whole point of Custom is that the
        // host language can dispatch to its own async runtime.
        let spawned_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let spawned_flag2 = spawned_flag.clone();

        let worker = AsyncWorker::with_custom(move |task| {
            let spawned_flag = spawned_flag2.clone();
            async move {
                spawned_flag.store(true, Ordering::SeqCst);
                task();
            }
        });

        // The spawn function isn't called until we await the future.
        assert!(!spawned_flag.load(Ordering::SeqCst));
        let value = futures::executor::block_on(worker.run(|| 99));
        assert!(spawned_flag.load(Ordering::SeqCst));
        assert_eq!(value, 99);
    }

    #[test]
    #[cfg(not(target_os = "emscripten"))]
    fn custom_with_spawn_blocking_runs_task() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let worker = AsyncWorker::with_custom(|task| async move {
            tokio::task::spawn_blocking(task).await.unwrap()
        });
        let value = rt.block_on(worker.run(|| 40 + 2));
        assert_eq!(value, 42);
    }

    #[test]
    #[cfg(not(target_os = "emscripten"))]
    fn custom_with_spawn_blocking_panic_is_propagated() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let worker = AsyncWorker::with_custom(|task| async move {
            tokio::task::spawn_blocking(task).await.unwrap()
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            rt.block_on(worker.run(|| -> () { panic!("boom") }))
        }));
        let payload = result.expect_err("panic should propagate to caller");
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"boom"));
    }

    #[test]
    #[cfg(not(target_os = "emscripten"))]
    fn custom_with_spawn_blocking_is_usable_after_panic() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let worker = AsyncWorker::with_custom(|task| async move {
            tokio::task::spawn_blocking(task).await.unwrap()
        });
        let _ = catch_unwind(AssertUnwindSafe(|| {
            rt.block_on(worker.run(|| -> () { panic!("boom") }))
        }));
        assert_eq!(rt.block_on(worker.run(|| 7)), 7);
    }

    #[test]
    #[cfg(not(target_os = "emscripten"))]
    fn custom_with_spawn_blocking_runs_on_different_thread() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let caller_thread = std::thread::current().id();
        let worker = AsyncWorker::with_custom(|task| async move {
            tokio::task::spawn_blocking(task).await.unwrap()
        });
        let task_thread = rt.block_on(worker.run(move || std::thread::current().id()));
        assert_ne!(
            task_thread, caller_thread,
            "spawn_blocking must offload to a different OS thread"
        );
    }

    #[test]
    #[cfg(not(target_os = "emscripten"))]
    fn custom_with_spawn_blocking_allows_concurrency() {
        // Unlike the Thread variant (single worker thread → serialized),
        // spawn_blocking dispatches to a thread pool so tasks can overlap.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let worker = Arc::new(AsyncWorker::with_custom(|task| async move {
            tokio::task::spawn_blocking(task).await.unwrap()
        }));
        let running = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let max_running = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let handles: Vec<_> = (0..32)
            .map(|i| {
                let worker = worker.clone();
                let running = running.clone();
                let max_running = max_running.clone();
                rt.spawn(async move {
                    worker
                        .run(move || {
                            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                            max_running.fetch_max(now, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(20));
                            running.fetch_sub(1, Ordering::SeqCst);
                            i
                        })
                        .await
                })
            })
            .collect();

        rt.block_on(async {
            let mut results = Vec::new();
            for handle in handles {
                results.push(handle.await.unwrap());
            }
            results.sort();
            assert_eq!(results, (0..32).collect::<Vec<_>>());
            // At least some overlap across the thread pool.
            assert!(
                max_running.load(Ordering::SeqCst) > 1,
                "spawn_blocking thread pool should provide concurrency"
            );
        });
    }
}
