pub mod api;
mod frb_generated;

/// Enforce the binding for this library (to prevent tree-shaking)
/// https://github.com/flutter/flutter/pull/96225#issuecomment-1319080539
#[no_mangle]
pub extern "C" fn enforce_binding() {}
