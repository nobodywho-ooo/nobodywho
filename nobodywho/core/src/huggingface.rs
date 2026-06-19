//! Download infrastructure and HuggingFace Hub helpers.

use crate::errors::{GetCacheDirError, GetCachedModelsError, HuggingFaceError, LoadModelError};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tracing::{info, warn};

// =========================================================================
// Progress callbacks
// =========================================================================

/// Callback invoked during model downloads with `(downloaded_bytes, total_bytes)`.
///
/// Invoked on each read chunk from the single download thread. If the same callback
/// is shared across concurrent downloads, the closure is responsible for its own
/// synchronization (hence the `Sync` bound).
pub type DownloadProgressCallback = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Default terminal progress bar shown when the user doesn't pass their own callback.
///
/// Renders an `indicatif` bar with spinner, elapsed time, wide bar, binary byte counts,
/// throughput, and ETA. indicatif auto-disables on non-TTY stderr, so this is safe to use
/// unconditionally — GUI bindings (Godot, Flutter mobile) won't see output in production.
/// Detects a new download (model → mmproj transition) by watching for `total` to change,
/// finishes the previous bar, and starts a fresh one.
pub fn default_progress_callback() -> DownloadProgressCallback {
    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} \
         {binary_bytes}/{binary_total_bytes} ({binary_bytes_per_sec}, {eta})",
    )
    .expect("static progress bar template is valid")
    .progress_chars("█▉▊▋▌▍▎▏ ");

    let state: Arc<Mutex<(Option<ProgressBar>, u64)>> = Arc::new(Mutex::new((None, 0)));

    Arc::new(move |downloaded: u64, total: u64| {
        let mut s = state.lock().expect("progress bar mutex poisoned");
        if s.0.is_none() || s.1 != total {
            if let Some(old) = s.0.take() {
                old.finish_and_clear();
            }
            let bar = ProgressBar::new(total);
            bar.set_style(style.clone());
            bar.enable_steady_tick(Duration::from_millis(100));
            s.0 = Some(bar);
            s.1 = total;
        }
        let bar = s.0.as_ref().unwrap();
        bar.set_position(downloaded);
        if downloaded >= total {
            bar.finish();
            s.0 = None;
        }
    })
}

/// Wrap a progress callback so it fires at most ~10 Hz, with a guaranteed
/// final emit on completion.
///
/// Use this when each invocation of the user-provided callback is expensive —
/// typically because it crosses a language boundary (Dart isolate hop, JSI
/// hop, etc.). Without throttling, a fast download (thousands of chunks/sec)
/// would saturate the cross-language bridge. The Python binding does NOT need
/// this since a PyO3 callable invocation is cheap; it forwards every chunk.
///
/// Lock-free: nanoseconds since a process-wide epoch in an `AtomicU64`, with `0`
/// as the never-emitted sentinel. The load/check/store can race, but an extra
/// emit per ~100ms window is harmless and downloads are single-threaded anyway.
pub fn throttled_progress_callback<F>(callback: F) -> DownloadProgressCallback
where
    F: Fn(u64, u64) + Send + Sync + 'static,
{
    static EPOCH: LazyLock<std::time::Instant> = LazyLock::new(std::time::Instant::now);
    const THROTTLE_NS: u64 = 100_000_000;

    let last_emit_ns = AtomicU64::new(0);
    Arc::new(move |downloaded: u64, total: u64| {
        let is_done = total > 0 && downloaded >= total;
        let now_ns = EPOCH.elapsed().as_nanos() as u64;
        let prev = last_emit_ns.load(Ordering::Relaxed);
        let due = prev == 0 || now_ns.saturating_sub(prev) >= THROTTLE_NS;
        if is_done || due {
            last_emit_ns.store(now_ns, Ordering::Relaxed);
            callback(downloaded, total);
        }
    })
}

// =========================================================================
// Download cache directory
// =========================================================================

/// Get the cache directory for downloaded models.
///
/// On Android, the package name is read from `/proc/self/cmdline` and the user ID
/// is derived from the UID (`uid / 100000`). This avoids needing JNI or an Android
/// Context object, which isn't reliably available — Flutter loads native libraries
/// via `dlopen` (not `System.loadLibrary`), so `JNI_OnLoad` is never called.
///
/// On other platforms, uses the `dirs` crate to find the standard cache directory.
pub(crate) fn get_cache_dir() -> Result<PathBuf, GetCacheDirError> {
    let base = get_platform_cache_dir()?;
    Ok(base.join("nobodywho").join("models"))
}

/// Every `.gguf` model in the nobodywho cache, paired with its byte size.
pub fn get_cached_models() -> Result<Vec<(PathBuf, usize)>, GetCachedModelsError> {
    let cache_dir = get_cache_dir()?;
    if !cache_dir.exists() {
        return Ok(Vec::new());
    }

    walkdir::WalkDir::new(&cache_dir)
        .into_iter()
        .filter(|res| match res {
            Ok(e) => {
                e.file_type().is_file()
                    && e.path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("gguf"))
            }
            Err(_) => true,
        })
        .map(|res| {
            let entry = res?;
            let len = entry.metadata()?.len() as usize;
            Ok((entry.into_path(), len))
        })
        .collect()
}

#[cfg(target_os = "android")]
fn get_platform_cache_dir() -> Result<PathBuf, GetCacheDirError> {
    // Read the package name from /proc/self/cmdline. This file contains the process
    // name as a null-terminated string. On Android this is the package name
    // (e.g. "com.example.app"), possibly with a colon suffix for multi-process apps
    // (e.g. "com.example.app:remote").
    let cmdline = std::fs::read("/proc/self/cmdline")?;

    let package_name = cmdline
        .split(|&b| b == 0)
        .next()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(|s| s.split(':').next().unwrap_or(s))
        .ok_or(GetCacheDirError::NoPackageName)?;

    // Derive the Android user ID from the Unix UID. Android assigns UIDs as:
    //   uid = user_id * 100000 + app_id
    // This gives the correct path on multi-user devices (e.g. GrapheneOS work
    // profiles), where /data/data/ is a symlink only valid for user 0.
    let uid = unsafe { libc::getuid() };
    let user_id = uid / 100000;

    Ok(PathBuf::from(format!(
        "/data/user/{user_id}/{package_name}/cache"
    )))
}

#[cfg(not(target_os = "android"))]
fn get_platform_cache_dir() -> Result<PathBuf, GetCacheDirError> {
    dirs::cache_dir().ok_or(GetCacheDirError::NoCacheDir)
}

// =========================================================================
// Generic HTTP download
// =========================================================================

/// Download a file from a URL to a local path, streaming to disk with progress logging.
///
/// Returns early if the file already exists at the target path.
/// Rejects paths containing `..` to prevent path traversal attacks.
pub(crate) fn download_file(
    url: &str,
    target_path: &Path,
    progress: &DownloadProgressCallback,
    headers: &[(String, String)],
) -> Result<(), LoadModelError> {
    for component in target_path.components() {
        if component == std::path::Component::ParentDir {
            return Err(LoadModelError::DownloadError(
                "Path traversal detected: '..' is not allowed in model paths".into(),
            ));
        }
    }

    if target_path.exists() {
        info!("Using cached file: {}", target_path.display());
        return Ok(());
    }

    // Create parent directories
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            LoadModelError::DownloadError(format!(
                "Failed to create cache directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    info!("Downloading {} -> {}", url, target_path.display());

    let mut request = ureq::get(url);
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let response = request.call().map_err(|e| match e {
        ureq::Error::StatusCode(status) => LoadModelError::from_http_status(url, status),
        e => LoadModelError::DownloadError(format!("HTTP request failed: {e}")),
    })?;

    // Content-Length is missing for many text/JSON responses served via chunked
    // transfer (e.g. HuggingFace `.gitattributes`, `config.json`, README.md);
    // we treat that as "unknown size" rather than an error — we still stream
    // the body, just without progress denominator or post-download size check.
    let content_length: u64 = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    if content_length > 0 {
        info!(
            "Download size: {:.1} GB",
            content_length as f64 / 1_073_741_824.0
        );
    } else {
        info!("Download size: unknown (no Content-Length)");
    }

    // Write to a temp file first, then rename — avoids partial files on failure.
    let tmp_path = target_path.with_file_name(format!(
        "{}.{:x}.part",
        target_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        rand::random::<u32>(),
    ));

    let download_result: Result<(), LoadModelError> = (|| {
        let mut file = std::fs::File::create(&tmp_path).map_err(|e| {
            LoadModelError::DownloadError(format!(
                "Failed to create temp file {}: {e}",
                tmp_path.display()
            ))
        })?;

        let body = response.into_body();
        let mut reader = body.into_reader();
        let mut downloaded: u64 = 0;
        let mut last_logged_pct: u64 = 0;
        let mut buf = vec![0u8; 256 * 1024]; // 256 KB chunks

        loop {
            let n = reader.read(&mut buf).map_err(|e| {
                LoadModelError::DownloadError(format!("Read error during download: {e}"))
            })?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| {
                LoadModelError::DownloadError(format!("Write error during download: {e}"))
            })?;
            downloaded += n as u64;

            progress(downloaded, content_length);

            if let Some(pct) = (downloaded * 100).checked_div(content_length) {
                if pct >= last_logged_pct + 5 {
                    info!("Download progress: {pct}% ({downloaded}/{content_length} bytes)");
                    last_logged_pct = pct;
                }
            }
        }
        if content_length > 0 && downloaded != content_length {
            return Err(LoadModelError::DownloadError(format!(
                "Download incomplete: got {downloaded}/{content_length} bytes"
            )));
        }
        Ok(())
    })();

    if download_result.is_err() {
        if let Err(e) = std::fs::remove_file(&tmp_path) {
            warn!("Failed to clean up temp file {}: {e}", tmp_path.display());
        }
        return download_result;
    }

    // Rename temp file to final path
    std::fs::rename(&tmp_path, target_path).map_err(|e| {
        LoadModelError::DownloadError(format!(
            "Failed to rename temp file to {}: {e}",
            target_path.display()
        ))
    })?;

    info!("Download complete: {}", target_path.display());
    Ok(())
}

/// Download a single file from a generic HTTP(S) URL into the cache and return
/// the local path. Cache keyed by the URL's path components.
pub(crate) fn download_model_from_url(
    url: &str,
    progress: &DownloadProgressCallback,
    headers: &[(String, String)],
) -> Result<PathBuf, LoadModelError> {
    let cache_dir = get_cache_dir()?;
    let path_part = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let target_path = cache_dir.join("http").join(path_part);
    download_file(url, &target_path, progress, headers)?;
    Ok(target_path)
}

// =========================================================================
// HuggingFace Hub: single-file download
// =========================================================================

/// HuggingFace Hub URL for resolving a file at a given revision.
pub(crate) fn hf_resolve_url(owner: &str, repo: &str, revision: &str, path: &str) -> String {
    format!("https://huggingface.co/{owner}/{repo}/resolve/{revision}/{path}")
}

/// Download a single file from a HuggingFace Hub repo and return the local path.
///
/// If the file is already cached locally, the cached path is returned without downloading.
pub(crate) fn download_model_from_hf(
    owner: &str,
    repo: &str,
    filename: &str,
    progress: &DownloadProgressCallback,
    headers: &[(String, String)],
) -> Result<PathBuf, LoadModelError> {
    let cache_dir = get_cache_dir()?;
    let target_path = cache_dir.join(owner).join(repo).join(filename);
    let url = hf_resolve_url(owner, repo, "main", filename);
    download_file(&url, &target_path, progress, headers)?;
    Ok(target_path)
}

// =========================================================================
// HuggingFace Hub: whole-repo "model source" workflow
// =========================================================================

const DEFAULT_REVISION: &str = "main";

#[derive(Clone, Debug)]
pub(crate) enum Source {
    Local(PathBuf),
    HuggingFace {
        owner: String,
        repo: String,
        revision: String,
    },
}

/// Parse a source string. Existing local directories win; otherwise we expect
/// `owner/repo` and treat it as a HuggingFace Hub repo ID at `main`.
pub(crate) fn parse(s: &str) -> Result<Source, HuggingFaceError> {
    let path = Path::new(s);
    if path.is_dir() {
        return Ok(Source::Local(path.to_path_buf()));
    }
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 && parts.iter().all(|p| is_valid_repo_part(p)) {
        return Ok(Source::HuggingFace {
            owner: parts[0].to_string(),
            repo: parts[1].to_string(),
            revision: DEFAULT_REVISION.into(),
        });
    }
    Err(HuggingFaceError::InvalidSource(s.to_string()))
}

fn is_valid_repo_part(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn is_dotfile(path: &str) -> bool {
    path.rsplit('/').next().unwrap_or(path).starts_with('.')
}

/// Resolve a parsed source to a local directory. For HF sources, lists the
/// repo via the HF API and downloads every file into the local cache.
pub(crate) fn resolve(source: Source) -> Result<PathBuf, HuggingFaceError> {
    match source {
        Source::Local(p) => Ok(p),
        Source::HuggingFace {
            owner,
            repo,
            revision,
        } => download_repo(&owner, &repo, &revision, &default_progress_callback()),
    }
}

#[derive(serde::Deserialize)]
struct HfTreeEntry {
    #[serde(rename = "type")]
    kind: String,
    path: String,
}

fn download_repo(
    owner: &str,
    repo: &str,
    revision: &str,
    progress: &DownloadProgressCallback,
) -> Result<PathBuf, HuggingFaceError> {
    let cache_dir = get_cache_dir()
        .map_err(|e| HuggingFaceError::Download(format!("locate cache dir: {e}")))?
        .join(owner)
        .join(repo);

    let tree_url =
        format!("https://huggingface.co/api/models/{owner}/{repo}/tree/{revision}?recursive=true");
    let body = ureq::get(&tree_url)
        .call()
        .map_err(|e| HuggingFaceError::Download(format!("HF tree list ({owner}/{repo}): {e}")))?
        .body_mut()
        .read_to_string()
        .map_err(|e| HuggingFaceError::Download(format!("read HF tree response: {e}")))?;
    let entries: Vec<HfTreeEntry> = serde_json::from_str(&body)
        .map_err(|e| HuggingFaceError::Download(format!("parse HF tree response: {e}")))?;

    // Skip dotfiles (e.g. `.gitattributes`)
    let files: Vec<String> = entries
        .into_iter()
        .filter(|e| e.kind == "file")
        .map(|e| e.path)
        .filter(|p| !is_dotfile(p))
        .collect();
    if files.is_empty() {
        return Err(HuggingFaceError::Download(format!(
            "HF repo {owner}/{repo}@{revision} has no files"
        )));
    }

    for path in &files {
        let url = hf_resolve_url(owner, repo, revision, path);
        let target = cache_dir.join(path);
        download_file(&url, &target, progress, &[])
            .map_err(|e| HuggingFaceError::Download(format!("{path}: {e}")))?;
    }

    Ok(cache_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hf_repo_id() {
        let Source::HuggingFace {
            owner,
            repo,
            revision,
        } = parse("NobodyWho/Kokoro-82M").unwrap()
        else {
            panic!("expected HuggingFace");
        };
        assert_eq!(owner, "NobodyWho");
        assert_eq!(repo, "Kokoro-82M");
        assert_eq!(revision, "main");
    }

    #[test]
    fn accepts_dashes_underscores_dots_in_repo_parts() {
        assert!(matches!(
            parse("a-b/c_d.e").unwrap(),
            Source::HuggingFace { .. }
        ));
    }

    #[test]
    fn existing_local_dir_resolves_to_local() {
        let dir = std::env::temp_dir();
        let s = dir.to_str().expect("temp_dir is valid utf-8");
        assert!(matches!(parse(s).unwrap(), Source::Local(_)));
    }

    #[test]
    fn rejects_no_slash() {
        assert!(parse("nobodywho").is_err());
    }

    #[test]
    fn rejects_too_many_slashes() {
        assert!(parse("a/b/c").is_err());
    }

    #[test]
    fn rejects_empty_owner() {
        assert!(parse("/repo").is_err());
    }

    #[test]
    fn rejects_empty_repo() {
        assert!(parse("owner/").is_err());
    }

    #[test]
    fn rejects_invalid_chars() {
        assert!(parse("foo/bar baz").is_err());
        assert!(parse("foo bar/baz").is_err());
        assert!(parse("foo!/baz").is_err());
    }
}
