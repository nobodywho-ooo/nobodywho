//! Download infrastructure and HuggingFace Hub helpers.
//!
//! Two independent use cases share the low-level cache/download machinery
//! ([`ModelCache`]) below:
//! - [`GgufSource`] — a single GGUF file (LLM / embedding / reranker models).
//! - [`OnnxSource`] — a whole multi-file ONNX model repo (STT / TTS models).

use crate::errors::{GetCacheDirError, GetCachedModelsError, HuggingFaceError, LoadModelError};
use indicatif::{ProgressBar, ProgressStyle};
use nom::branch::alt;
use nom::bytes::complete::{tag, tag_no_case, take_until};
use nom::combinator::{cut, map, rest, verify};
use nom::sequence::{preceded, terminated};
use nom::Parser;
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

/// Default terminal progress bar shown when the user doesn't pass their own callback,
/// labeled with the last path segment of `path` — e.g. `"hf://owner/repo/model.gguf"`
/// shows as `"model.gguf"`.
///
/// indicatif auto-disables on non-TTY stderr, so this is safe to use unconditionally —
/// GUI bindings (Godot, Flutter mobile) won't see output in production. Callers
/// downloading more than one file (a repo download, or a GGUF model plus its
/// mmproj) should create one of these per file rather than sharing a single
/// instance, so each file gets its own visibly-labeled bar instead of an
/// unlabeled one that appears to just restart.
pub fn default_progress_callback(path: &str) -> DownloadProgressCallback {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {msg} {wide_bar:.cyan/blue} \
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
            bar.set_message(name.clone());
            bar.enable_steady_tick(Duration::from_millis(100));
            s.0 = Some(bar);
            s.1 = total;
        }
        let bar = s.0.as_ref().unwrap();
        bar.set_position(downloaded);

        if total > 0 && downloaded >= total {
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
// ModelCache — shared cache root + low-level file download
// =========================================================================

/// Root directory all downloaded models are cached under, shared by both
/// [`GgufSource`] and [`OnnxSource`]. Cheap to construct — open a fresh one
/// wherever a source needs resolving, no need to cache the instance across calls.
struct ModelCache {
    root: PathBuf,
}

impl ModelCache {
    fn open() -> Result<Self, GetCacheDirError> {
        let base = Self::platform_cache_dir()?;
        Ok(Self {
            root: base.join("nobodywho").join("models"),
        })
    }

    /// On Android, the package name is read from `/proc/self/cmdline` and the user ID
    /// is derived from the UID (`uid / 100000`). This avoids needing JNI or an Android
    /// Context object, which isn't reliably available — Flutter loads native libraries
    /// via `dlopen` (not `System.loadLibrary`), so `JNI_OnLoad` is never called.
    ///
    /// On other platforms, uses the `dirs` crate to find the standard cache directory.
    #[cfg(target_os = "android")]
    fn platform_cache_dir() -> Result<PathBuf, GetCacheDirError> {
        // Multi-process apps get a colon suffix here (e.g. "com.example.app:remote").
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
    fn platform_cache_dir() -> Result<PathBuf, GetCacheDirError> {
        dirs::cache_dir().ok_or(GetCacheDirError::NoCacheDir)
    }

    /// Streams into a `tempfile::NamedTempFile` next to the target and atomically
    /// persists it on success. Unlike a hand-rolled temp path, this cleans itself
    /// up on any early return — including a panic mid-download — rather than only
    /// on an explicit `Err` we remembered to handle.
    fn fetch_to_path(
        &self,
        url: &str,
        target_path: &Path,
        progress: &DownloadProgressCallback,
        headers: &[(String, String)],
    ) -> Result<(), LoadModelError> {
        Self::validate_no_traversal(target_path)?;

        if target_path.exists() {
            info!("Using cached file: {}", target_path.display());
            return Ok(());
        }

        Self::ensure_parent_dir(target_path)?;
        info!("Downloading {} -> {}", url, target_path.display());

        let (mut reader, content_length) = Self::open_http_stream(url, headers)?;
        let mut tmp_file = Self::create_temp_file(target_path)?;
        let tmp_path = tmp_file.path().to_path_buf();

        Self::stream_to_file(
            &mut reader,
            url,
            tmp_file.as_file_mut(),
            &tmp_path,
            content_length,
            progress,
        )?;

        tmp_file
            .persist(target_path)
            .map_err(|e| LoadModelError::RenameTempFile {
                from: e.file.path().to_path_buf(),
                to: target_path.to_path_buf(),
                source: e.error,
            })?;

        info!("Download complete: {}", target_path.display());
        Ok(())
    }

    fn validate_no_traversal(path: &Path) -> Result<(), LoadModelError> {
        for component in path.components() {
            if component == std::path::Component::ParentDir {
                return Err(LoadModelError::PathTraversal {
                    path: path.to_path_buf(),
                });
            }
        }
        Ok(())
    }

    fn ensure_parent_dir(path: &Path) -> Result<(), LoadModelError> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent).map_err(|source| LoadModelError::CreateCacheDir {
            path: parent.to_path_buf(),
            source,
        })
    }

    /// Returned `u64` is the parsed `Content-Length`; 0 means unknown (see below).
    fn open_http_stream(
        url: &str,
        headers: &[(String, String)],
    ) -> Result<(Box<dyn Read>, u64), LoadModelError> {
        let mut request = ureq::get(url);
        for (name, value) in headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let response = request.call().map_err(|e| match e {
            ureq::Error::StatusCode(status) => LoadModelError::from_http_status(url, status),
            source => LoadModelError::HttpRequest {
                url: url.to_owned(),
                source,
            },
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

        Ok((Box::new(response.into_body().into_reader()), content_length))
    }

    // Same directory as `target_path` so the later `persist` rename is atomic.
    fn create_temp_file(target_path: &Path) -> Result<tempfile::NamedTempFile, LoadModelError> {
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        tempfile::Builder::new()
            .prefix(&format!(
                "{}.",
                target_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ))
            .suffix(".part")
            .tempfile_in(parent)
            .map_err(|source| LoadModelError::CreateTempFile {
                path: target_path.to_path_buf(),
                source,
            })
    }

    fn stream_to_file(
        reader: &mut dyn Read,
        url: &str,
        file: &mut std::fs::File,
        tmp_path: &Path,
        content_length: u64,
        progress: &DownloadProgressCallback,
    ) -> Result<(), LoadModelError> {
        let mut downloaded: u64 = 0;
        let mut last_logged_pct: u64 = 0;
        let mut buf = vec![0u8; 256 * 1024];

        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|source| LoadModelError::ReadDownload {
                    url: url.to_owned(),
                    source,
                })?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|source| LoadModelError::WriteDownload {
                    path: tmp_path.to_path_buf(),
                    source,
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
            return Err(LoadModelError::IncompleteDownload {
                url: url.to_owned(),
                got: downloaded,
                expected: content_length,
            });
        }

        // Content-Length was unknown throughout (chunked transfer), so every
        // call above reported `total = 0`. Report the now-known final size so
        // the bar can close out against a real total instead of never
        // learning how big the file actually was.
        if content_length == 0 {
            progress(downloaded, downloaded);
        }
        Ok(())
    }
}

const DEFAULT_REVISION: &str = "main";

/// A HuggingFace Hub repo at a given revision. Used by both [`GgufSource`]
/// (one file) and [`OnnxSource`] (a whole repo).
#[derive(Clone, Debug)]
struct HfRepo {
    owner: String,
    repo: String,
    revision: String,
}

impl HfRepo {
    fn main(owner: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            revision: DEFAULT_REVISION.into(),
        }
    }

    /// `owner/repo`, used in error messages and as the cache subdirectory key.
    fn id(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    fn cache_dir(&self, cache: &ModelCache) -> PathBuf {
        cache.root.join(&self.owner).join(&self.repo)
    }

    fn resolve_url(&self, path: &str) -> String {
        format!(
            "https://huggingface.co/{}/{}/resolve/{}/{}",
            self.owner, self.repo, self.revision, path
        )
    }

    fn tree_url(&self) -> String {
        format!(
            "https://huggingface.co/api/models/{}/{}/tree/{}?recursive=true",
            self.owner, self.repo, self.revision
        )
    }
}

// =========================================================================
// GgufSource — a single GGUF file (LLM / embedding / reranker models)
// =========================================================================

/// Where a single GGUF model file comes from: a HuggingFace Hub repo, or an
/// arbitrary HTTP(S) URL. (A local filesystem path needs no resolving at all,
/// so it's handled entirely by the caller — see [`download_gguf`].)
enum GgufSource {
    HuggingFace { repo: HfRepo, filename: String },
    Url(String),
}

impl ModelCache {
    /// Resolve a [`GgufSource`] to a local file, downloading it first unless
    /// it's already cached.
    fn download_file(
        &self,
        source: &GgufSource,
        progress: &DownloadProgressCallback,
        headers: &[(String, String)],
    ) -> Result<PathBuf, LoadModelError> {
        let (url, target) = match source {
            GgufSource::HuggingFace { repo, filename } => (
                repo.resolve_url(filename),
                repo.cache_dir(self).join(filename),
            ),
            GgufSource::Url(url) => {
                let path_part = url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://");
                (url.clone(), self.root.join("http").join(path_part))
            }
        };

        self.fetch_to_path(&url, &target, progress, headers)?;
        Ok(target)
    }

    /// Every `.gguf` file already in the cache, paired with its byte size.
    fn list_cached(&self) -> Result<Vec<(PathBuf, usize)>, GetCachedModelsError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        walkdir::WalkDir::new(&self.root)
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
}

/// Every `.gguf` model in the nobodywho cache, paired with its byte size.
pub fn get_cached_models() -> Result<Vec<(PathBuf, usize)>, GetCachedModelsError> {
    ModelCache::open()?.list_cached()
}

/// A parsed `model_path` string, before it's resolved to somewhere on disk.
#[derive(Clone)]
pub(crate) enum ParsedModelPath {
    HuggingFaceUrl(String, String, String), // e.g. hf://owner/repo/model.gguf -> (owner, repo, filename)
    HttpUrl(String),                        // e.g. https://example.com/lol/qwen3.gguf
    FilesystemPath(PathBuf),                // e.g. ./qwen3.gguf
}

pub(crate) fn parse_model_path(
    model_path: &str,
) -> Result<ParsedModelPath, nom::Err<nom::error::Error<String>>> {
    let mut parser = alt((
        // hf://owner/repo/filename.gguf (also hf:, huggingface:, huggingface://)
        map(
            preceded(
                alt((
                    tag_no_case("huggingface://"),
                    tag_no_case("huggingface:"),
                    tag_no_case("hf://"),
                    tag_no_case("hf:"),
                )),
                cut((
                    terminated(take_until("/"), tag("/")),
                    terminated(take_until("/"), tag("/")),
                    verify(rest, |s: &str| !s.is_empty()),
                )),
            ),
            |(owner, repo, filename): (&str, &str, &str)| {
                ParsedModelPath::HuggingFaceUrl(owner.into(), repo.into(), filename.into())
            },
        ),
        // https://... or http://...
        map(
            (alt((tag_no_case("https://"), tag_no_case("http://"))), rest),
            |(scheme, path): (&str, &str)| ParsedModelPath::HttpUrl(format!("{}{}", scheme, path)),
        ),
        // Anything else is a filesystem path (expand leading ~ on non-Android)
        map(rest, |p: &str| {
            ParsedModelPath::FilesystemPath(PathBuf::from(p))
        }),
    ));
    let result: nom::IResult<&str, ParsedModelPath> = parser.parse(model_path);
    result
        .map(|(_, parsed)| parsed)
        .map_err(|e| e.map(|e| e.cloned()))
}

/// Takes a fancy path (possibly with `hf:` or `https://` in front) and resolves
/// it to a realized path on the filesystem, downloading it first if needed.
pub(crate) fn download_gguf(
    parsed_path: ParsedModelPath,
    progress: &DownloadProgressCallback,
    headers: &[(String, String)],
) -> Result<PathBuf, LoadModelError> {
    let cache = ModelCache::open()?;
    let fs_model_path = match parsed_path {
        ParsedModelPath::HuggingFaceUrl(owner, repo, filename) => {
            let source = GgufSource::HuggingFace {
                repo: HfRepo::main(owner, repo),
                filename,
            };
            cache.download_file(&source, progress, headers)?
        }
        ParsedModelPath::FilesystemPath(path) => path,
        ParsedModelPath::HttpUrl(url) => {
            cache.download_file(&GgufSource::Url(url), progress, headers)?
        }
    };

    if !fs_model_path.exists() {
        return Err(LoadModelError::from_missing_path(&fs_model_path));
    }

    LoadModelError::validate_model_file(&fs_model_path)?;

    Ok(fs_model_path)
}

// =========================================================================
// OnnxSource — a whole multi-file ONNX model repo (STT / TTS models)
// =========================================================================

/// Name of the local marker file recording which repo files a previous call
/// to `OnnxSource::download_repo` confirmed were fully downloaded. Used to
/// skip the network round-trip on a later call — see its use below for why
/// this can't be inferred from `required_files` alone.
const COMPLETE_MARKER: &str = ".nobodywho-complete";

#[derive(serde::Deserialize)]
struct HfTreeEntry {
    #[serde(rename = "type")]
    kind: String,
    path: String,
}

/// Where a whole multi-file ONNX model comes from: an existing local
/// directory, or a HuggingFace Hub repo.
///
/// `required_files` narrows a HuggingFace download to just the files actually
/// needed (e.g. one ONNX precision variant out of a dozen a repo may ship),
/// rather than downloading the entire repo. Pass an empty list to download
/// everything unfiltered.
#[derive(Clone, Debug)]
enum OnnxSource {
    Local(PathBuf),
    HuggingFace {
        repo: HfRepo,
        required_files: Vec<String>,
    },
}

impl OnnxSource {
    fn is_dotfile(path: &str) -> bool {
        path.rsplit('/').next().unwrap_or(path).starts_with('.')
    }

    /// Narrow `available` (every file in a repo) down to just `required` (e.g. a
    /// single ONNX precision variant out of the dozen a repo may ship), rather
    /// than downloading the entire repo. An empty `required` list keeps the
    /// download-everything behavior, for callers that genuinely need that.
    ///
    /// Also pulls in ONNX external-data sidecars: a `.onnx` file's weights may be
    /// split into a companion `<path>_data` file (or `<path>_data/` directory of
    /// chunks) when the graph would otherwise exceed protobuf's 2GB limit — e.g.
    /// `onnx-community/whisper-large-v3-turbo` ships `encoder_model.onnx` (440KB,
    /// just the graph) alongside `encoder_model.onnx_data` (2.5GB, the weights).
    /// Without this, loading the model fails after a "successful" download.
    ///
    /// Returns the names in `required` that aren't present in `available` as `Err`.
    fn select_required_files(
        available: Vec<String>,
        required: &[String],
    ) -> Result<Vec<String>, Vec<String>> {
        if required.is_empty() {
            return Ok(available);
        }
        let missing: Vec<String> = required
            .iter()
            .filter(|r| !available.contains(r))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(missing);
        }
        Ok(available
            .into_iter()
            .filter(|p| required.contains(p) || Self::is_external_data_for(p, required))
            .collect())
    }

    /// True if `path` is the ONNX external-data sidecar (or a chunk inside the
    /// sidecar directory) for one of the files in `required`.
    fn is_external_data_for(path: &str, required: &[String]) -> bool {
        required.iter().any(|r| {
            let prefix = format!("{r}_data");
            path == prefix || path.starts_with(&format!("{prefix}/"))
        })
    }

    fn read_completed_files(cache_dir: &Path) -> Option<Vec<String>> {
        let content = std::fs::read_to_string(cache_dir.join(COMPLETE_MARKER)).ok()?;
        let files: Vec<String> = content
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        (!files.is_empty()).then_some(files)
    }

    /// Record that `files` were just fully downloaded, merging with any
    /// previously recorded set (e.g. a different quantization of the same repo)
    /// so neither is forgotten. Best-effort: a write failure just means the next
    /// call re-verifies over the network instead of failing this one.
    fn write_completed_files(cache_dir: &Path, files: &[String]) {
        let mut all_files = Self::read_completed_files(cache_dir).unwrap_or_default();
        for f in files {
            if !all_files.contains(f) {
                all_files.push(f.clone());
            }
        }
        if let Err(e) = std::fs::write(cache_dir.join(COMPLETE_MARKER), all_files.join("\n")) {
            warn!(
                "Failed to write download-completion marker in {}: {e}",
                cache_dir.display()
            );
        }
    }
}

impl ModelCache {
    /// Resolve an [`OnnxSource`] to a local directory. An existing local
    /// directory is returned as-is; a HuggingFace repo is downloaded (or
    /// verified already-cached) first.
    fn download_repo(
        &self,
        source: &OnnxSource,
        progress: &DownloadProgressCallback,
    ) -> Result<PathBuf, HuggingFaceError> {
        let (repo, required_files) = match source {
            OnnxSource::Local(p) => return Ok(p.clone()),
            OnnxSource::HuggingFace {
                repo,
                required_files,
            } => (repo, required_files),
        };

        let cache_dir = repo.cache_dir(self);

        // Skip the network round-trip if a previous call already downloaded
        // everything this call needs. We can't infer that from `required_files`
        // alone: an empty list (e.g. TTS backends, which don't filter) matches
        // vacuously without proving anything is on disk, and a non-empty list
        // may omit ONNX external-data sidecars that only `select_required_files`
        // discovers from the live repo tree. So completeness is recorded in a
        // local marker after a successful download, not guessed beforehand.
        if let Some(completed) = OnnxSource::read_completed_files(&cache_dir) {
            let satisfied = required_files.iter().all(|f| completed.contains(f))
                && completed.iter().all(|f| cache_dir.join(f).exists());
            if satisfied {
                info!("Using cached repo: {}", cache_dir.display());
                return Ok(cache_dir);
            }
        }

        let body = ureq::get(&repo.tree_url())
            .call()
            .map_err(|source| HuggingFaceError::ListRepoTree {
                repo: repo.id(),
                source,
            })?
            .body_mut()
            .read_to_string()
            .map_err(|source| HuggingFaceError::ReadRepoTree {
                repo: repo.id(),
                source,
            })?;
        let entries: Vec<HfTreeEntry> =
            serde_json::from_str(&body).map_err(|source| HuggingFaceError::ParseRepoTree {
                repo: repo.id(),
                source,
            })?;

        // Skip dotfiles (e.g. `.gitattributes`)
        let all_files: Vec<String> = entries
            .into_iter()
            .filter(|e| e.kind == "file")
            .map(|e| e.path)
            .filter(|p| !OnnxSource::is_dotfile(p))
            .collect();

        let files =
            OnnxSource::select_required_files(all_files, required_files).map_err(|missing| {
                HuggingFaceError::MissingRequiredFiles {
                    repo: repo.id(),
                    files: missing,
                }
            })?;

        if files.is_empty() {
            return Err(HuggingFaceError::EmptyRepo {
                repo: repo.id(),
                revision: repo.revision.clone(),
            });
        }

        for path in &files {
            let url = repo.resolve_url(path);
            let target = cache_dir.join(path);
            // `progress` is one instance shared across every file in this repo, so on
            // its own it can't label the bar per file. Drive a freshly-labeled bar
            // here (named after the concrete file) alongside it, forwarding the same
            // byte counts to both.
            let named = default_progress_callback(path);
            let progress = progress.clone();
            let file_progress: DownloadProgressCallback = Arc::new(move |downloaded, total| {
                named(downloaded, total);
                progress(downloaded, total);
            });
            self.fetch_to_path(&url, &target, &file_progress, &[])
                .map_err(|source| HuggingFaceError::DownloadEntry {
                    path: path.clone(),
                    source: Box::new(source),
                })?;
        }
        OnnxSource::write_completed_files(&cache_dir, &files);

        Ok(cache_dir)
    }
}

/// A parsed STT/TTS `source` string, before it's resolved to a local directory.
///
/// This mirrors [`ParsedModelPath`] but only needs `owner/repo` (a whole repo
/// download), never a filename. `http(s)://` URLs and bare `owner/repo` IDs
/// are not recognized — they fall through to [`ParsedOnnxPath::Local`] and
/// fail the downstream directory check.
pub(crate) enum ParsedOnnxPath {
    HuggingFace(String, String), // owner, repo
    Local(PathBuf),
}

/// Parse an STT/TTS source string. Recognized forms:
/// - `hf://owner/repo`, `hf:owner/repo`, `huggingface://owner/repo`,
///   `huggingface:owner/repo` → [`ParsedOnnxPath::HuggingFace`]. A trailing
///   path (e.g. `hf://owner/repo/extra`) is rejected — ONNX download is
///   repo-scoped.
/// - Anything else (local paths, `http(s)://` URLs, bare `owner/repo`) →
///   [`ParsedOnnxPath::Local`]. Callers reject non-directories downstream.
pub(crate) fn parse_onnx_path(s: &str) -> Result<ParsedOnnxPath, HuggingFaceError> {
    let mut parser = alt((
        // hf://owner/repo (also hf:, huggingface:, huggingface://) — exactly
        // owner/repo, no trailing path. The repo part must be non-empty and
        // contain no further '/', so hf://owner/repo/extra is rejected.
        map(
            preceded(
                alt((
                    tag_no_case("huggingface://"),
                    tag_no_case("huggingface:"),
                    tag_no_case("hf://"),
                    tag_no_case("hf:"),
                )),
                cut((
                    verify(terminated(take_until("/"), tag("/")), |s: &str| {
                        !s.is_empty()
                    }),
                    verify(rest, |s: &str| !s.is_empty() && !s.contains('/')),
                )),
            ),
            |(owner, repo): (&str, &str)| ParsedOnnxPath::HuggingFace(owner.into(), repo.into()),
        ),
        // Anything else is a local filesystem path. http(s):// URLs land here
        // and are rejected by the caller's directory check.
        map(rest, |p: &str| ParsedOnnxPath::Local(PathBuf::from(p))),
    ));
    let result: nom::IResult<&str, ParsedOnnxPath> = parser.parse(s);
    result
        .map(|(_, parsed)| parsed)
        .map_err(|_| HuggingFaceError::InvalidSource(s.to_string()))
}

/// Resolve a model source string to a local directory. `hf://owner/repo`
/// (or `huggingface:` variants) downloads from the HuggingFace Hub, narrowed
/// to `required_files` (an empty slice downloads the entire repo unfiltered).
/// Anything else is treated as a local directory path and must exist on disk.
pub(crate) fn download_onnx(
    source: &str,
    required_files: &[String],
) -> Result<PathBuf, HuggingFaceError> {
    let cache = ModelCache::open()?;
    // No caller threads a custom callback through yet, so there's nothing
    // meaningful to forward here — `download_repo` labels its own per-file bars.
    let progress: DownloadProgressCallback = Arc::new(|_downloaded, _total| {});
    let source = match parse_onnx_path(source)? {
        ParsedOnnxPath::HuggingFace(owner, repo) => OnnxSource::HuggingFace {
            repo: HfRepo::main(owner, repo),
            required_files: required_files.to_vec(),
        },
        ParsedOnnxPath::Local(path) => OnnxSource::Local(path),
    };
    cache.download_repo(&source, &progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hf_scheme() {
        let ParsedOnnxPath::HuggingFace(owner, repo) =
            parse_onnx_path("hf://NobodyWho/Kokoro-82M").unwrap()
        else {
            panic!("expected HuggingFace");
        };
        assert_eq!(owner, "NobodyWho");
        assert_eq!(repo, "Kokoro-82M");
    }

    #[test]
    fn parses_hf_colon_scheme() {
        assert!(matches!(
            parse_onnx_path("hf:NobodyWho/Kokoro-82M").unwrap(),
            ParsedOnnxPath::HuggingFace(_, _)
        ));
    }

    #[test]
    fn parses_huggingface_scheme_variants() {
        assert!(matches!(
            parse_onnx_path("huggingface://a/b").unwrap(),
            ParsedOnnxPath::HuggingFace(_, _)
        ));
        assert!(matches!(
            parse_onnx_path("huggingface:a/b").unwrap(),
            ParsedOnnxPath::HuggingFace(_, _)
        ));
    }

    #[test]
    fn accepts_dashes_underscores_dots_in_repo_parts() {
        assert!(matches!(
            parse_onnx_path("hf://a-b/c_d.e").unwrap(),
            ParsedOnnxPath::HuggingFace(_, _)
        ));
    }

    #[test]
    fn existing_local_dir_resolves_to_local() {
        let dir = std::env::temp_dir();
        let s = dir.to_str().expect("temp_dir is valid utf-8");
        assert!(matches!(
            parse_onnx_path(s).unwrap(),
            ParsedOnnxPath::Local(_)
        ));
    }

    #[test]
    fn bare_repo_id_is_local_not_hf() {
        // Bare `owner/repo` is no longer auto-interpreted as HuggingFace; it's
        // a local path that the caller rejects if it isn't an existing directory.
        assert!(matches!(
            parse_onnx_path("NobodyWho/Kokoro-82M").unwrap(),
            ParsedOnnxPath::Local(_)
        ));
    }

    #[test]
    fn http_url_is_local_not_hf() {
        // http(s):// falls through to Local; the caller rejects non-directories.
        assert!(matches!(
            parse_onnx_path("https://huggingface.co/owner/repo").unwrap(),
            ParsedOnnxPath::Local(_)
        ));
    }

    #[test]
    fn rejects_trailing_path_on_hf_scheme() {
        // ONNX download is repo-scoped: a trailing path is a parse error.
        assert!(parse_onnx_path("hf://owner/repo/extra").is_err());
    }

    #[test]
    fn rejects_no_slash_after_hf_scheme() {
        assert!(parse_onnx_path("hf:nobodywho").is_err());
    }

    #[test]
    fn rejects_empty_owner_after_hf_scheme() {
        assert!(parse_onnx_path("hf:///repo").is_err());
    }
}
