/// mirrors https://github.com/ggml-org/llama.cpp/blob/11b068d06605288ce7917534b46d52b47823dc13/common/common.cpp#L78
///
/// Deviations from upstream: no AIX branch (`powerpc64-ibm-aix` is Rust tier 3), and the tail
/// fallback reads `available_parallelism()` rather than `hardware_concurrency()` so cgroup CPU
/// limits are respected. Per-branch notes are inline below.
use tracing::{info, warn};

/// Fallback thread count, mirroring ggml's `GGML_DEFAULT_N_THREADS`.
const GGML_DEFAULT_N_THREADS: u32 = 4;

/// Thread count for llama.cpp inference.
///
/// `None` autodetects (physical cores, P-cores on Apple); `Some(n)` requests a count. Both
/// are clamped to 1 <= n <= logical.
pub fn inference_thread_count(requested: Option<u32>) -> u32 {
    let logical = logical_core_count();
    let physical = physical_core_count();
    let n_threads = resolve(requested, physical, logical);

    match requested {
        Some(requested) if requested != n_threads => warn!(
            requested,
            n_threads, logical, "Requested thread count is out of range; clamped it"
        ),
        Some(_) => info!(n_threads, logical, "Using the requested thread count"),
        None => {
            if physical.is_none() {
                warn!(
                    logical,
                    n_threads, "Could not read CPU topology; guessing the physical core count"
                );
            }
            info!(
                n_threads,
                physical, logical, "Selected inference thread count"
            );
        }
    }
    n_threads
}

/// Thread count by precedence: explicit request, else detected physical cores, else
/// heuristic. Clamped to `1..=logical`.
///
/// The heuristic — keep all on ≤4 logical CPUs, else halve.
fn resolve(requested: Option<u32>, physical: Option<u32>, logical: u32) -> u32 {
    let logical = logical.max(1);
    let chosen = requested
        .or(physical)
        .unwrap_or(if logical <= 4 { logical } else { logical / 2 });
    chosen.clamp(1, logical)
}

fn logical_core_count() -> u32 {
    std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(GGML_DEFAULT_N_THREADS)
}

/// Distinct sibling-mask count. SMT siblings share an identical `thread_siblings` string,
/// so they collapse into one group.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn count_sibling_groups(siblings: &[String]) -> Option<u32> {
    let unique: std::collections::HashSet<&str> =
        siblings.iter().map(|entry| entry.trim()).collect();
    (!unique.is_empty()).then(|| unique.len() as u32)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn physical_core_count() -> Option<u32> {
    // Count physical cores: enumerate cpuN/topology/thread_siblings until a CPU is missing;
    // distinct masks = physical cores. Only the hex mask is read, as upstream does — falling
    // back to `thread_siblings_list` would compare a list against a mask and count one core
    // twice if the two forms ever mixed.
    let mut siblings = Vec::new();
    for cpu in 0.. {
        let directory = format!("/sys/devices/system/cpu/cpu{cpu}/topology");
        if !std::path::Path::new(&directory).exists() {
            break;
        }
        match std::fs::read_to_string(format!("{directory}/thread_siblings")) {
            Ok(mask) => siblings.push(mask),
            // Upstream stops here. We keep scanning: a mid-range CPU can be offline while
            // later ones aren't, and the directory check above is already our terminator.
            Err(error) => tracing::debug!(cpu, %error, "Could not read CPU sibling mask"),
        }
    }
    count_sibling_groups(&siblings)
}

#[cfg(target_vendor = "apple")]
fn physical_core_count() -> Option<u32> {
    fn sysctl_i32(name: &std::ffi::CStr) -> Option<u32> {
        let mut value = 0i32;
        let mut size = std::mem::size_of::<i32>();
        let result = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
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

    // `hw.perflevel0.physicalcpu` is P-cores on Apple silicon; absent on Intel Macs, where
    // `hw.physicalcpu` collapses SMT.
    sysctl_i32(c"hw.perflevel0.physicalcpu").or_else(|| sysctl_i32(c"hw.physicalcpu"))
}

/// `RelationProcessorCore` from `LOGICAL_PROCESSOR_RELATIONSHIP`. Duplicated as a literal so
/// [`count_core_records`] can be tested on non-Windows hosts; the Windows path asserts it
/// against `windows_sys`'s own constant.
#[cfg(any(target_os = "windows", test))]
const RELATION_PROCESSOR_CORE: i32 = 0;

/// Count `RelationProcessorCore` records in a `GetLogicalProcessorInformationEx` buffer.
///
/// Records are variable-length and each carries its own `Size`, which for a core record is
/// *smaller* than `SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX` (48 vs 80 bytes on x64, because
/// the struct's union is sized for `GROUP_RELATIONSHIP`). So the struct's size cannot gate the
/// walk — doing so drops the final record — and a reference to it cannot be formed over a
/// 48-byte record without reading past it. Only the two fixed leading fields are needed.
///
/// Split out from the FFI so it is testable off-Windows, which is the only platform branch
/// here with real parsing logic.
#[cfg(any(target_os = "windows", test))]
fn count_core_records(buffer: &[u8]) -> u32 {
    /// `Relationship: i32` followed by `Size: u32`.
    const HEADER: usize = 2 * std::mem::size_of::<u32>();

    let mut cores = 0u32;
    let mut offset = 0usize;
    while offset + HEADER <= buffer.len() {
        // Read unaligned: `buffer` is bytes, so a record need not sit on a 4-byte boundary.
        let base = unsafe { buffer.as_ptr().add(offset) };
        let relationship = unsafe { base.cast::<i32>().read_unaligned() };
        let record_size = unsafe {
            base.add(std::mem::size_of::<i32>())
                .cast::<u32>()
                .read_unaligned()
        } as usize;

        // A zero or short `Size` would loop forever; a `Size` past the end means a truncated
        // record we cannot trust.
        if record_size < HEADER || offset + record_size > buffer.len() {
            break;
        }
        // Upstream adds `info->Processor.GroupCount`, which is always 1 on a core record.
        if relationship == RELATION_PROCESSOR_CORE {
            cores += 1;
        }
        offset += record_size;
    }
    cores
}

#[cfg(target_os = "windows")]
fn physical_core_count() -> Option<u32> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore,
    };

    debug_assert_eq!(RELATION_PROCESSOR_CORE, RelationProcessorCore);

    // Null buffer to learn the size, then walk the variable-length records (each carries
    // its own `Size`).
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

    // The second call can shrink `size`; trust it over the buffer length.
    let cores = count_core_records(&buffer[..(size as usize).min(buffer.len())]);

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
        // A bogus topology report must not oversubscribe.
        assert_eq!(resolve(None, Some(64), 8), 8);
    }

    #[test]
    fn never_returns_zero_threads() {
        assert_eq!(resolve(None, Some(0), 0), 1);
    }

    #[test]
    fn a_request_overrides_the_detected_topology() {
        assert_eq!(resolve(Some(3), Some(8), 16), 3);
        // Fewer than physical is legitimate: headroom for rendering or co-tenant models.
        assert_eq!(resolve(Some(1), Some(8), 16), 1);
    }

    #[test]
    fn a_request_cannot_oversubscribe_the_host() {
        assert_eq!(resolve(Some(999), Some(8), 12), 12);
        assert_eq!(resolve(Some(13), None, 12), 12);
    }

    #[test]
    fn a_zero_request_is_treated_as_one_thread() {
        assert_eq!(resolve(Some(0), Some(8), 12), 1);
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn sibling_groups_collapse_smt_pairs() {
        let siblings = ["0,8", "1,9", "2,10", "8,0"].map(String::from);
        assert_eq!(count_sibling_groups(&siblings), Some(3));
        assert_eq!(count_sibling_groups(&[]), None);
    }

    /// One `GetLogicalProcessorInformationEx` record: `Relationship`, `Size`, then padding out
    /// to `size` bytes. A real core record is 48 bytes on x64, well under the 80-byte
    /// `SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX`.
    fn record(relationship: i32, size: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(size);
        bytes.extend_from_slice(&relationship.to_ne_bytes());
        bytes.extend_from_slice(&(size as u32).to_ne_bytes());
        bytes.resize(size, 0);
        bytes
    }

    #[test]
    fn counts_every_core_record_including_the_last() {
        // The bug this guards: gating the walk on `size_of::<SYSTEM_LOGICAL_PROCESSOR_..._EX>()`
        // (80) instead of the 8-byte header drops the final 48-byte record, so an 8-core host
        // reports 7.
        for cores in 1..=32usize {
            let buffer: Vec<u8> = (0..cores)
                .flat_map(|_| record(RELATION_PROCESSOR_CORE, 48))
                .collect();
            assert_eq!(count_core_records(&buffer), cores as u32, "{cores} cores");
        }
    }

    #[test]
    fn skips_records_that_are_not_processor_cores() {
        let mut buffer = record(RELATION_PROCESSOR_CORE, 48);
        buffer.extend(record(1, 64)); // RelationNumaNode, a different length
        buffer.extend(record(RELATION_PROCESSOR_CORE, 48));
        assert_eq!(count_core_records(&buffer), 2);
    }

    #[test]
    fn stops_on_a_truncated_or_degenerate_record() {
        // `Size` running past the buffer: count what was whole, discard the rest.
        let mut truncated = record(RELATION_PROCESSOR_CORE, 48);
        truncated.extend(record(RELATION_PROCESSOR_CORE, 48).into_iter().take(20));
        assert_eq!(count_core_records(&truncated), 1);

        // `Size` of 0 must terminate rather than spin forever.
        let mut zero_size = record(RELATION_PROCESSOR_CORE, 48);
        zero_size.extend(record(RELATION_PROCESSOR_CORE, 0));
        zero_size.resize(zero_size.len() + 48, 0);
        assert_eq!(count_core_records(&zero_size), 1);

        assert_eq!(count_core_records(&[]), 0);
        assert_eq!(count_core_records(&[0u8; 4]), 0); // shorter than one header
    }

    #[test]
    fn walks_records_at_unaligned_offsets() {
        // Record lengths are only guaranteed 4-byte multiples, so a later record can land off
        // an 8-byte boundary; the reads must not assume alignment.
        let mut buffer = record(RELATION_PROCESSOR_CORE, 12);
        buffer.extend(record(RELATION_PROCESSOR_CORE, 48));
        assert_eq!(count_core_records(&buffer), 2);
    }

    #[test]
    fn detects_a_usable_thread_count_on_this_host() {
        let logical = logical_core_count();
        let n_threads = inference_thread_count(None);
        println!(
            "logical={logical} physical={:?} n_threads={n_threads}",
            physical_core_count()
        );
        assert!(n_threads >= 1);
        assert!(n_threads <= logical);
    }

    #[test]
    fn honours_a_request_on_this_host() {
        assert_eq!(inference_thread_count(Some(1)), 1);
        assert_eq!(inference_thread_count(Some(u32::MAX)), logical_core_count());
    }
}
