use std::cell::RefCell;

use godot::prelude::*;
use godot::task::spawn;

/// A latched async result, awaitable from GDScript via `wait()`.
///
/// `wait()` returns the `done` signal while pending, or the result once done.
/// Since `await` on a non-Signal value returns it instantly, this works
/// whether you await before or after completion, any number of times.
///
/// The result is stored (latched) on the object so a late awaiter never
/// misses it — the lost-signal race is structurally impossible.
///
/// Internal machinery: no public NobodyWho method returns a task. Every
/// async `#[func]` calls `.bind().wait()` Rust-side and returns the
/// value-or-Signal Variant directly, so GDScript callers just
/// `await chat.foo()` (and must await immediately).
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoTask {
    result: RefCell<Option<Variant>>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoTask {
    /// Fires once on completion. `wait()` is the usual entry point; this is
    /// for connect-style code.
    #[signal]
    fn done(result: Variant);

    /// Await this. Resolves to the result whether completion is past or future.
    /// Also the Rust-side entry point for value-or-Signal returns (see chat.rs).
    #[func]
    pub fn wait(&self) -> Variant {
        match &*self.result.borrow() {
            Some(v) => v.clone(), // latched: instant pass-through
            None => Signal::from_object_signal(&self.to_gd(), "done").to_variant(),
        }
    }

    /// True once the task has resolved.
    #[func]
    fn is_done(&self) -> bool {
        self.result.borrow().is_some()
    }

    /// Non-blocking peek; nil while pending.
    #[func]
    fn result(&self) -> Variant {
        self.result.borrow().clone().unwrap_or_default()
    }
}

/// Run `future` on the Godot executor; return a latched task.
///
/// For blocking work (model load, worker init), drive it via
/// [`on_blocking_thread`] from inside the future.
///
/// Never panic in `future` — gdext swallows it and the GDScript await hangs.
/// Resolve with `null` + `godot_error!` on any failure instead.
pub fn task<F>(future: F) -> Gd<NobodyWhoTask>
where
    F: std::future::Future<Output = Variant> + 'static,
{
    let obj = Gd::from_init_fn(|base| NobodyWhoTask {
        result: RefCell::new(None),
        base,
    });
    let held = obj.clone(); // keep alive until the emit
    spawn(async move {
        let value = future.await;
        // Latch first, then emit, so a re-entrant signal handler sees the result.
        held.bind().result.replace(Some(value.clone()));
        held.signals().done().emit(&value);
    });
    obj
}

/// Run a blocking closure on a dedicated thread; await its result.
///
/// Returns `None` if the closure panicked. Use for core constructors that
/// block (e.g. `ChatHandleAsync::new`). Interim helper — deleted once core
/// gets async constructors (Phase 5).
pub async fn on_blocking_thread<T, F>(f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.await.ok() // None if the sender dropped (closure panicked)
}
