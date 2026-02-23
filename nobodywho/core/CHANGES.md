# WorkerGuard Consolidation

## Problem

`ChatHandle` and `ChatHandleAsync` each stored a redundant `Arc<AtomicBool>` field called
`should_stop` — purely so `stop_generation()` could reach the stop flag. The same flag was
already stored inside `WorkerGuard` (used by its `Drop` impl). This meant every chat handle
kept two `Arc` clones of the same flag alive.

Additionally, every place that sent a message to the worker repeated the same boilerplate:

```rust
if let Some(ref msg_tx) = self.guard.msg_tx {
    let _ = msg_tx.send(msg);
}
```

This pattern appeared 6 times across `ChatHandle` + `ChatHandleAsync`, plus once each in
`EncoderAsync` and `CrossEncoderAsync`.

A related issue: `WorkerGuard::Drop` called `handle.join()`, which blocked the dropping
thread waiting for the worker to exit. If the worker was blocked in `output_tx.blocking_send()`
(waiting for a consumer that would never arrive), both threads deadlocked. An earlier
5-second timeout with thread detach was a bandaid for this.

## Solution

### 1. Enrich `WorkerGuard<T>` (`llm.rs`)

Added two helper methods:

```rust
pub(crate) fn send(&self, msg: T) -> bool {
    self.msg_tx.as_ref().map_or(false, |tx| tx.send(msg).is_ok())
}

pub(crate) fn stop(&self) {
    if let Some(ref flag) = self.should_stop {
        flag.store(true, Ordering::Relaxed);
    }
}
```

### 2. Remove redundant `should_stop` from handles (`chat.rs`)

`ChatHandle` and `ChatHandleAsync` no longer store their own `Arc<AtomicBool>`. The flag
lives exclusively inside `WorkerGuard`. Both constructors pass `should_stop` directly into
`WorkerGuard::new()` instead of cloning it into the handle.

### 3. Update all call sites

All 8 message-send sites now use `self.guard.send(msg)`, and both `stop_generation()`
implementations now use `self.guard.stop()`. Same cleanup applied to `EncoderAsync` and
`CrossEncoderAsync`.

### 4. Fix the Drop deadlock

#### Root cause

During inference the worker calls `output_tx.blocking_send(token)` to stream each token
to the caller. `blocking_send` on a bounded tokio channel blocks the calling thread when
the channel is full — or more precisely, when the channel is full **and the receiver is
still alive**.

#### Why it was especially bad in Python

Two separate aggravating factors, one per binding class.

**`Chat` (sync) — GIL held across `join()`**

PyO3 runs `Drop` while the Python GIL is held. With `join()` in the drop, the Python
thread (holding the GIL) blocked waiting for the worker. The worker's `blocking_send`
is a tokio operation; if the `TokenStream` Python object had not yet been garbage
collected, `output_rx` was still alive, and if the 4096-token buffer filled up, nobody
could drain it — the main thread was stuck on `join()`. Deadlock.

**`ChatAsync` (async) — tokio runtime contention**

The `ChatAsync::drop` in the Python bindings already released the GIL before dropping
the handle (`py.detach(|| drop(handle))`), with a comment explaining it was meant to let
pyo3-async-runtimes tokio tasks acquire the GIL for their cleanup and thereby unblock
`join()`. This was a partial fix: it handled the case where GIL acquisition was the
blocker. But if the underlying issue was `blocking_send` stalling because the
`output_rx` consumer was not being polled (e.g., the tokio task driving
`TokenStreamAsync` was not scheduled), releasing the GIL had no effect and the join
still hung.

---

The deadlock manifested when:

1. The caller dropped `ChatHandle` while a generation was in progress (e.g. the
   `TokenStream` was abandoned mid-stream, or the tokio runtime was shutting down and
   nobody was polling the receiver).
2. `WorkerGuard::drop` set `should_stop = true`, closed `msg_tx`, then called
   `handle.join()` — blocking the dropping thread.
3. The worker was blocked inside `blocking_send`, waiting for the receiver to consume a
   token. Because the dropping thread was now blocked on `join()`, it could not run the
   async executor that would drain the channel.
4. Both threads waited on each other indefinitely.

The previous 5-second timeout + detach was a bandaid: it gave up waiting and detached the
thread, unblocking the drop. It did not fix the root cause.

#### Why the detach works

The two signals issued *before* releasing the thread handle are sufficient for the worker
to exit on its own:

- `should_stop = true` — the write loop checks this flag at the top of every iteration.
  After the current `blocking_send` unblocks (or errors), the loop exits.
- `msg_tx` closed — when the write loop finishes and the worker returns to its outer
  `while let Ok(msg) = msg_rx.recv()` loop, `recv()` immediately returns `Err` and the
  thread exits.

The deadlock broke specifically because `blocking_send` would eventually return `Err`
once the receiver (`output_rx`) was itself dropped — which happens as soon as the call
stack that held it unwinds. By not blocking on `join()`, the dropping thread unwinds
immediately, the `TokenStream` (holding `output_rx`) is dropped, the channel closes, and
`blocking_send` in the worker returns `Err`. The worker then sees `should_stop = true`,
exits the write loop, and terminates cleanly in the background.

```rust
drop(self.join_handle.take()); // detach — signals already sent above
```
