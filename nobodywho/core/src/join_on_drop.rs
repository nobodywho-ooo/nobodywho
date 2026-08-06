//! Joins a worker thread when dropped.

/// Wrap a worker's `JoinHandle` so the thread is joined when this drops.
/// Used by handles whose worker must finish before the process (or a host
/// like Godot) tears down underneath it.
pub(crate) struct JoinOnDrop(Option<std::thread::JoinHandle<()>>);

impl JoinOnDrop {
    pub(crate) fn new(handle: std::thread::JoinHandle<()>) -> Self {
        Self(Some(handle))
    }
}

impl Drop for JoinOnDrop {
    fn drop(&mut self) {
        // take(): join() consumes the handle, Drop only gives &mut.
        // A worker panic is already logged; nothing more to do with it here.
        if let Some(join) = self.0.take() {
            let _ = join.join();
        }
    }
}
