//! CPU topology detection for choosing llama.cpp thread counts.
//!
//! `std::thread::available_parallelism()` counts *logical* CPUs: SMT siblings on x86 and
//! efficiency cores on Apple silicon. ggml synchronizes every graph node on a spin
//! barrier, so the slowest thread paces the whole operation — running one thread per
//! logical CPU makes two threads fight over a single core's vector units, or parks work
//! on an E-core that everyone else then waits for.
//!
//! llama.cpp's own tooling avoids this by defaulting to physical cores
//! (`common_cpu_get_num_physical_cores()` in `common/common.cpp`), but that helper is not
//! reachable from Rust: `llama-cpp-sys-2`'s bindgen allowlist only covers `ggml_*`,
//! `gguf_*`, `llama_*` and `mtmd_*`, so `common_*` symbols are never bound. This module
//! is a port of that function — same platform order, same fallbacks — so our default
//! matches what `llama-cli` would pick on the same host.

use tracing::{info, warn};

/// Environment variable that overrides the detected thread count.
const N_THREADS_ENV: &str = "NOBODYWHO_N_THREADS";

/// Number of threads to use for llama.cpp inference.
///
/// Prefers physical cores (performance cores on Apple silicon). Set
/// `NOBODYWHO_N_THREADS` to override. Never returns 0, and never exceeds the logical CPU
/// count.
pub fn inference_thread_count() -> u32 {
    let explicit = std::env::var(N_THREADS_ENV)
        .ok()
        .and_then(|value| parse_count(&value));
    let logical = logical_core_count();
    let physical = physical_core_count();
    let n_threads = resolve(explicit, physical, logical);

    if physical.is_none() && explicit.is_none() {
        warn!(
            logical,
            n_threads,
            "Could not read CPU topology; guessing the physical core count. Set \
             NOBODYWHO_N_THREADS if inference is slower than expected."
        );
    }
    info!(
        n_threads,
        explicit, physical, logical, "Selected inference thread count"
    );
    n_threads
}

/// Pick a thread count from the detected topology.
///
/// Precedence: explicit override, then physical cores, then llama.cpp's last-ditch
/// heuristic. Always clamped into `1..=logical`.
///
/// That heuristic — keep every CPU on hosts with 4 or fewer, otherwise halve — is
/// upstream's verbatim (`n_threads <= 4 ? n_threads : n_threads / 2`, the tail of
/// `common_cpu_get_num_physical_cores()`), and it assumes the host is x86 with SMT, so
/// halving lands on one thread per physical core. It is *wrong* on a non-SMT host whose
/// topology we failed to read — an ARM server or a mobile device with restricted sysfs —
/// where it under-provisions by 2x. We keep it anyway because the platforms that reach
/// this branch at all (BSD and other unhandled OSes, plus Windows when
/// `GetLogicalProcessorInformationEx` fails) skew x86-with-SMT, and because diverging from
/// upstream here would be trading a measured default for a guess. Detection failure is
/// logged at `warn` so it can be recognised in the field, and `NOBODYWHO_N_THREADS`
/// overrides it.
fn resolve(explicit: Option<u32>, physical: Option<u32>, logical: u32) -> u32 {
    let logical = logical.max(1);
    let chosen = explicit.or(physical).unwrap_or(if logical <= 4 {
        logical
    } else {
        logical / 2
    });
    chosen.clamp(1, logical)
}

/// Parse a thread count from an environment variable. Rejects zero and garbage.
fn parse_count(value: &str) -> Option<u32> {
    match value.trim().parse::<u32>() {
        Ok(0) | Err(_) => None,
        Ok(count) => Some(count),
    }
}

/// Number of logical CPUs, falling back to `GGML_DEFAULT_N_THREADS`.
fn logical_core_count() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(4)
}

/// Count distinct sibling groups. Each entry is the raw contents of one CPU's
/// `thread_siblings` file, so SMT siblings share an identical string and collapse into a
/// single group.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn count_sibling_groups(siblings: &[String]) -> Option<u32> {
    let unique: std::collections::HashSet<&str> =
        siblings.iter().map(|entry| entry.trim()).collect();
    (!unique.is_empty()).then(|| unique.len() as u32)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn physical_core_count() -> Option<u32> {
    // Enumerate /sys/devices/system/cpu/cpuN/topology/thread_siblings until a CPU is
    // missing; the number of distinct sibling masks is the number of physical cores.
    // `thread_siblings` is a hex mask and `thread_siblings_list` the human-readable form —
    // either works for equality comparison, so accept whichever is present.
    let mut siblings = Vec::new();
    for cpu in 0.. {
        let directory = format!("/sys/devices/system/cpu/cpu{cpu}/topology");
        if !std::path::Path::new(&directory).exists() {
            break;
        }
        let entry = std::fs::read_to_string(format!("{directory}/thread_siblings"))
            .or_else(|_| std::fs::read_to_string(format!("{directory}/thread_siblings_list")));
        match entry {
            Ok(mask) => siblings.push(mask),
            // A CPU can be offline (topology files unreadable) while later ones are
            // online, so keep scanning rather than bailing out.
            Err(error) => tracing::debug!(cpu, %error, "Could not read CPU sibling mask"),
        }
    }
    count_sibling_groups(&siblings)
}

#[cfg(target_vendor = "apple")]
fn physical_core_count() -> Option<u32> {
    fn sysctl_i32(name: &[u8]) -> Option<u32> {
        let mut value = 0i32;
        let mut size = std::mem::size_of::<i32>();
        let result = unsafe {
            libc::sysctlbyname(
                name.as_ptr().cast(),
                (&raw mut value).cast::<std::ffi::c_void>(),
                &raw mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 || value <= 0 {
            return None;
        }
        Some(value as u32)
    }

    // `hw.perflevel0.physicalcpu` is the performance-core count on Apple silicon; it does
    // not exist on Intel Macs, where `hw.physicalcpu` already excludes SMT siblings.
    sysctl_i32(b"hw.perflevel0.physicalcpu\0").or_else(|| sysctl_i32(b"hw.physicalcpu\0"))
}

#[cfg(target_os = "windows")]
fn physical_core_count() -> Option<u32> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    // First call with a null buffer to learn the required size, then walk the returned
    // blob: the records are variable-length, each carrying its own `Size`.
    let mut size = 0u32;
    if unsafe {
        GetLogicalProcessorInformationEx(RelationProcessorCore, std::ptr::null_mut(), &raw mut size)
    } != 0
        || size == 0
    {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    if unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            buffer.as_mut_ptr().cast(),
            &raw mut size,
        )
    } == 0
    {
        return None;
    }

    let mut cores = 0u32;
    let mut offset = 0usize;
    while offset + std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() <= size as usize {
        let record = unsafe {
            &*buffer
                .as_ptr()
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
        };
        if record.Size == 0 {
            break;
        }
        if record.Relationship == RelationProcessorCore {
            cores += 1;
        }
        offset += record.Size as usize;
    }

    (cores > 0).then_some(cores)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "windows"
)))]
fn physical_core_count() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_override_wins_over_detected_topology() {
        assert_eq!(resolve(Some(3), Some(8), 16), 3);
    }

    #[test]
    fn physical_count_is_preferred_over_logical() {
        assert_eq!(resolve(None, Some(8), 16), 8);
    }

    #[test]
    fn falls_back_to_half_the_logical_count() {
        assert_eq!(resolve(None, None, 16), 8);
    }

    #[test]
    fn small_hosts_keep_every_logical_cpu() {
        assert_eq!(resolve(None, None, 4), 4);
        assert_eq!(resolve(None, None, 1), 1);
    }

    #[test]
    fn counts_are_clamped_to_the_logical_cpu_count() {
        assert_eq!(resolve(Some(999), None, 8), 8);
        // A bogus topology report must not oversubscribe either.
        assert_eq!(resolve(None, Some(64), 8), 8);
    }

    #[test]
    fn never_returns_zero_threads() {
        assert_eq!(resolve(None, Some(0), 0), 1);
    }

    #[test]
    fn parse_count_rejects_zero_and_garbage() {
        assert_eq!(parse_count("8"), Some(8));
        assert_eq!(parse_count(" 8 "), Some(8));
        assert_eq!(parse_count("0"), None);
        assert_eq!(parse_count("-1"), None);
        assert_eq!(parse_count("eight"), None);
        assert_eq!(parse_count(""), None);
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn sibling_groups_collapse_smt_pairs() {
        let siblings = ["0,8", "1,9", "2,10", "8,0"].map(String::from);
        assert_eq!(count_sibling_groups(&siblings), Some(3));
        assert_eq!(count_sibling_groups(&[]), None);
    }

    #[test]
    fn detects_a_usable_thread_count_on_this_host() {
        let logical = logical_core_count();
        let n_threads = inference_thread_count();
        println!(
            "logical={logical} physical={:?} n_threads={n_threads}",
            physical_core_count()
        );
        assert!(n_threads >= 1);
        assert!(n_threads <= logical);
    }
}
