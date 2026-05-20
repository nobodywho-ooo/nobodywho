//! Resolve a TTS model source string to a local directory.
//!
//! Accepts either an existing local directory path or a HuggingFace Hub repo
//! ID in `owner/repo` form (matching the convention used by `transformers`,
//! `huggingface_hub`, etc.). The whole repo is mirrored into the user's
//! download cache on first use, then reused on subsequent runs.

use crate::errors::TtsError;
use crate::llm::{
    default_progress_callback, download_file, get_cache_dir, hf_resolve_url,
    DownloadProgressCallback,
};
use std::path::{Path, PathBuf};

const DEFAULT_REVISION: &str = "main";

#[derive(Clone, Debug)]
pub(super) enum Source {
    Local(PathBuf),
    HuggingFace {
        owner: String,
        repo: String,
        revision: String,
    },
}

/// Parse a source string. Existing local directories win; otherwise we expect
/// `owner/repo` and treat it as a HuggingFace Hub repo ID at `main`.
pub(super) fn parse(s: &str) -> Result<Source, TtsError> {
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
    Err(TtsError::Init(format!(
        "tts: source {s:?} is neither an existing directory nor a valid `owner/repo` HuggingFace ID"
    )))
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
pub(super) fn resolve(source: Source) -> Result<PathBuf, TtsError> {
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
) -> Result<PathBuf, TtsError> {
    let cache_dir = get_cache_dir()
        .map_err(|e| TtsError::Init(format!("tts: locate cache dir: {e}")))?
        .join(owner)
        .join(repo);

    let tree_url =
        format!("https://huggingface.co/api/models/{owner}/{repo}/tree/{revision}?recursive=true");
    let body = ureq::get(&tree_url)
        .call()
        .map_err(|e| TtsError::Init(format!("tts: HF tree list failed ({owner}/{repo}): {e}")))?
        .body_mut()
        .read_to_string()
        .map_err(|e| TtsError::Init(format!("tts: read HF tree response: {e}")))?;
    let entries: Vec<HfTreeEntry> = serde_json::from_str(&body)
        .map_err(|e| TtsError::Init(format!("tts: parse HF tree response: {e}")))?;

    // Skip dotfiles (e.g. `.gitattributes`) — git metadata, no runtime value.
    let files: Vec<String> = entries
        .into_iter()
        .filter(|e| e.kind == "file")
        .map(|e| e.path)
        .filter(|p| !is_dotfile(p))
        .collect();
    if files.is_empty() {
        return Err(TtsError::Init(format!(
            "tts: HF repo {owner}/{repo}@{revision} has no files"
        )));
    }

    for path in &files {
        let url = hf_resolve_url(owner, repo, revision, path);
        let target = cache_dir.join(path);
        download_file(&url, &target, progress, &[])
            .map_err(|e| TtsError::Init(format!("tts: download {path}: {e}")))?;
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
