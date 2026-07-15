use crate::errors::MemoryDetectionError;

#[derive(Clone, Copy, Debug)]
pub(crate) struct HostMemory {
    pub available_bytes: u64,
    pub total_bytes: u64,
}

pub(crate) fn available() -> Result<HostMemory, MemoryDetectionError> {
    let memory = platform_memory()?;
    if memory.available_bytes == 0 || memory.total_bytes == 0 {
        return Err(MemoryDetectionError::InvalidData {
            origin: "operating system".to_string(),
            reason: "reported zero available or total memory".to_string(),
        });
    }
    Ok(memory)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn platform_memory() -> Result<HostMemory, MemoryDetectionError> {
    use std::path::{Path, PathBuf};

    const MEMINFO: &str = "/proc/meminfo";
    const CGROUP: &str = "/proc/self/cgroup";
    const MOUNTINFO: &str = "/proc/self/mountinfo";

    #[derive(Clone, Copy)]
    enum CgroupVersion {
        V1,
        V2,
    }

    struct CgroupMembership {
        path: PathBuf,
        version: CgroupVersion,
    }

    struct CgroupMount {
        mount_point: PathBuf,
        root: PathBuf,
    }

    fn read(path: &Path) -> Result<String, MemoryDetectionError> {
        std::fs::read_to_string(path).map_err(|source| MemoryDetectionError::ReadFile {
            path: path.to_path_buf(),
            source,
        })
    }

    fn invalid(source: &str, reason: impl Into<String>) -> MemoryDetectionError {
        MemoryDetectionError::InvalidData {
            origin: source.to_string(),
            reason: reason.into(),
        }
    }

    fn parse_kib(content: &str, key: &str) -> Result<u64, MemoryDetectionError> {
        let line = content
            .lines()
            .find(|line| line.starts_with(key))
            .ok_or_else(|| invalid(MEMINFO, format!("missing {key}")))?;
        let value = line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| invalid(MEMINFO, format!("missing value for {key}")))?
            .parse::<u64>()
            .map_err(|error| invalid(MEMINFO, format!("invalid {key}: {error}")))?;
        value
            .checked_mul(1024)
            .ok_or_else(|| invalid(MEMINFO, format!("{key} overflows bytes")))
    }

    fn parse_membership(content: &str) -> Result<Option<CgroupMembership>, MemoryDetectionError> {
        let mut v2 = None;
        for line in content.lines() {
            let mut fields = line.splitn(3, ':');
            let hierarchy = fields
                .next()
                .ok_or_else(|| invalid(CGROUP, "missing hierarchy ID"))?;
            let controllers = fields
                .next()
                .ok_or_else(|| invalid(CGROUP, "missing controllers"))?;
            let path = fields
                .next()
                .ok_or_else(|| invalid(CGROUP, "missing cgroup path"))?;

            if controllers
                .split(',')
                .any(|controller| controller == "memory")
            {
                return Ok(Some(CgroupMembership {
                    path: PathBuf::from(path),
                    version: CgroupVersion::V1,
                }));
            }
            if hierarchy == "0" && controllers.is_empty() {
                v2 = Some(CgroupMembership {
                    path: PathBuf::from(path),
                    version: CgroupVersion::V2,
                });
            }
        }
        Ok(v2)
    }

    fn mount_path(value: &str) -> PathBuf {
        PathBuf::from(
            value
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\012", "\n")
                .replace("\\134", "\\"),
        )
    }

    fn find_mount(
        content: &str,
        version: CgroupVersion,
    ) -> Result<Option<CgroupMount>, MemoryDetectionError> {
        for line in content.lines() {
            let (mount, filesystem) = line
                .split_once(" - ")
                .ok_or_else(|| invalid(MOUNTINFO, "missing filesystem separator"))?;
            let mount_fields = mount.split_whitespace().collect::<Vec<_>>();
            let filesystem_fields = filesystem.split_whitespace().collect::<Vec<_>>();
            if mount_fields.len() < 5 || filesystem_fields.len() < 3 {
                return Err(invalid(MOUNTINFO, "incomplete mount entry"));
            }

            let is_match = match version {
                CgroupVersion::V2 => filesystem_fields[0] == "cgroup2",
                CgroupVersion::V1 => {
                    filesystem_fields[0] == "cgroup"
                        && filesystem_fields[2]
                            .split(',')
                            .any(|option| option == "memory")
                }
            };
            if is_match {
                return Ok(Some(CgroupMount {
                    root: mount_path(mount_fields[3]),
                    mount_point: mount_path(mount_fields[4]),
                }));
            }
        }
        Ok(None)
    }

    fn parse_bytes(path: &Path, content: &str) -> Result<u64, MemoryDetectionError> {
        content.trim().parse::<u64>().map_err(|error| {
            invalid(
                &path.display().to_string(),
                format!("invalid byte count: {error}"),
            )
        })
    }

    fn cgroup_memory(host: HostMemory) -> Result<Option<HostMemory>, MemoryDetectionError> {
        let Some(membership) = parse_membership(&read(Path::new(CGROUP))?)? else {
            return Ok(None);
        };
        let mount = find_mount(&read(Path::new(MOUNTINFO))?, membership.version)?
            .ok_or_else(|| invalid(MOUNTINFO, "could not find the process memory cgroup mount"))?;
        let relative = membership
            .path
            .strip_prefix(&mount.root)
            .or_else(|_| membership.path.strip_prefix("/"))
            .map_err(|_| {
                invalid(
                    CGROUP,
                    format!("cgroup path {} is not absolute", membership.path.display()),
                )
            })?;
        let mut directory = mount.mount_point.join(relative);
        let (limit_name, usage_name, unlimited) = match membership.version {
            CgroupVersion::V1 => ("memory.limit_in_bytes", "memory.usage_in_bytes", None),
            CgroupVersion::V2 => ("memory.max", "memory.current", Some("max")),
        };

        let first_limit = directory.join(limit_name);
        if matches!(membership.version, CgroupVersion::V2) && !first_limit.exists() {
            return Ok(None);
        }

        let mut effective_total = host.total_bytes;
        let mut effective_available = host.available_bytes;
        loop {
            let limit_path = directory.join(limit_name);
            let usage_path = directory.join(usage_name);
            let limit_content = read(&limit_path)?;
            if unlimited != Some(limit_content.trim()) {
                let limit = parse_bytes(&limit_path, &limit_content)?;
                let usage = parse_bytes(&usage_path, &read(&usage_path)?)?;
                effective_total = effective_total.min(limit);
                effective_available = effective_available.min(limit.saturating_sub(usage));
            }

            if directory == mount.mount_point {
                break;
            }
            directory = directory.parent().map(Path::to_path_buf).ok_or_else(|| {
                invalid(MOUNTINFO, "cgroup directory did not reach its mount point")
            })?;
        }

        Ok(Some(HostMemory {
            available_bytes: effective_available,
            total_bytes: effective_total,
        }))
    }

    let meminfo = read(Path::new(MEMINFO))?;
    let host = HostMemory {
        available_bytes: parse_kib(&meminfo, "MemAvailable:")?,
        total_bytes: parse_kib(&meminfo, "MemTotal:")?,
    };
    Ok(cgroup_memory(host)?.unwrap_or(host))
}

#[cfg(target_vendor = "apple")]
fn platform_memory() -> Result<HostMemory, MemoryDetectionError> {
    use std::ffi::c_void;

    fn total_memory() -> Result<u64, MemoryDetectionError> {
        let mut total_bytes = 0u64;
        let mut total_size = std::mem::size_of::<u64>();
        let result = unsafe {
            libc::sysctlbyname(
                b"hw.memsize\0".as_ptr().cast(),
                (&raw mut total_bytes).cast::<c_void>(),
                &raw mut total_size,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 {
            return Err(MemoryDetectionError::SystemCall {
                operation: "sysctlbyname(hw.memsize)",
                source: std::io::Error::last_os_error(),
            });
        }
        Ok(total_bytes)
    }

    #[cfg(target_os = "macos")]
    fn available_memory() -> Result<u64, MemoryDetectionError> {
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page_size <= 0 {
            return Err(MemoryDetectionError::SystemCall {
                operation: "sysconf(_SC_PAGESIZE)",
                source: std::io::Error::last_os_error(),
            });
        }

        let mut statistics = unsafe { std::mem::zeroed::<libc::vm_statistics64>() };
        let mut count = libc::HOST_VM_INFO64_COUNT;
        #[allow(deprecated)]
        let host = unsafe { libc::mach_host_self() };
        let result = unsafe {
            libc::host_statistics64(
                host,
                libc::HOST_VM_INFO64,
                (&raw mut statistics).cast(),
                &raw mut count,
            )
        };
        if result != libc::KERN_SUCCESS {
            return Err(MemoryDetectionError::InvalidData {
                origin: "host_statistics64".to_string(),
                reason: format!("returned Mach error {result}"),
            });
        }

        Ok(u64::from(statistics.active_count)
            .saturating_add(u64::from(statistics.inactive_count))
            .saturating_add(u64::from(statistics.free_count))
            .saturating_mul(page_size as u64))
    }

    #[cfg(not(target_os = "macos"))]
    fn available_memory() -> Result<u64, MemoryDetectionError> {
        unsafe extern "C" {
            fn os_proc_available_memory() -> libc::size_t;
        }

        Ok(unsafe { os_proc_available_memory() } as u64)
    }

    Ok(HostMemory {
        available_bytes: available_memory()?,
        total_bytes: total_memory()?,
    })
}

#[cfg(target_os = "windows")]
fn platform_memory() -> Result<HostMemory, MemoryDetectionError> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    if unsafe { GlobalMemoryStatusEx(&raw mut status) } == 0 {
        return Err(MemoryDetectionError::SystemCall {
            operation: "GlobalMemoryStatusEx",
            source: std::io::Error::last_os_error(),
        });
    }

    Ok(HostMemory {
        available_bytes: status.ullAvailPhys,
        total_bytes: status.ullTotalPhys,
    })
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "windows"
)))]
fn platform_memory() -> Result<HostMemory, MemoryDetectionError> {
    Err(MemoryDetectionError::UnsupportedPlatform {
        platform: std::env::consts::OS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_available_host_memory() {
        let memory = available().unwrap();
        assert!(memory.available_bytes > 0);
        assert!(memory.total_bytes >= memory.available_bytes);
    }
}
