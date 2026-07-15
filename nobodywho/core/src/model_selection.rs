use crate::errors::MemoryDetectionError;
use crate::memory;
use tracing::info;

const GIB: u64 = 1024 * 1024 * 1024;

// Sorted by minimum usable memory. The first entry is the fallback.
const RECOMMENDATIONS: &[(u64, &str)] = &[
    (
        0,
        "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    ),
    (
        GIB,
        "hf://NobodyWho/Qwen_Qwen3.5-0.8B-GGUF/Qwen_Qwen3.5-0.8B-Q4_K_M-vendor-sampling.gguf",
    ),
    (
        2 * GIB,
        "hf://NobodyWho/Qwen_Qwen3.5-2B-GGUF/Qwen_Qwen3.5-2B-Q4_K_M-vendor-sampling.gguf",
    ),
    (
        4 * GIB,
        "hf://NobodyWho/Qwen_Qwen3.5-4B-GGUF/Qwen_Qwen3.5-4B-Q4_K_M-vendor-sampling.gguf",
    ),
    (
        5 * GIB,
        "hf://NobodyWho/Google_Gemma4-E2B-GGUF/gemma-4-E2B-it-Q4_K_M.gguf",
    ),
    (
        7 * GIB,
        "hf://NobodyWho/Google_Gemma4-E4B-GGUF/gemma-4-E4B-it-Q4_K_M.gguf",
    ),
    (
        10 * GIB,
        "hf://NobodyWho/Google_Gemma4-12B-GGUF/gemma-4-12b-it-Q4_K_M.gguf",
    ),
    (
        24 * GIB,
        "hf://NobodyWho/Qwen_Qwen3.6-27B-GGUF/Qwen_Qwen3.6-27B-Q4_K_M-vendor-sampling.gguf",
    ),
];

fn recommend_llm_for(usable_memory: u64) -> &'static str {
    RECOMMENDATIONS
        .iter()
        .rev()
        .find(|(minimum_memory, _)| usable_memory >= *minimum_memory)
        .map(|(_, model_path)| *model_path)
        .expect("recommendations must include a zero-memory fallback")
}

fn resolve_model_path_for(model_path: &str, usable_memory: u64) -> &str {
    if model_path == "auto" {
        recommend_llm_for(usable_memory)
    } else {
        model_path
    }
}

pub(crate) fn resolve_model_path(
    model_path: &str,
    use_gpu: bool,
) -> Result<&str, MemoryDetectionError> {
    if model_path != "auto" {
        return Ok(model_path);
    }

    let available_memory = memory::available_model_memory(use_gpu)?;
    let recommendation = resolve_model_path_for(model_path, available_memory.usable_bytes);

    info!(
        model = recommendation,
        available_memory_gib = available_memory.free_bytes as f64 / GIB as f64,
        usable_memory_gib = available_memory.usable_bytes as f64 / GIB as f64,
        "Automatically selected model"
    );

    Ok(recommendation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommendations_are_sorted_with_a_fallback() {
        assert_eq!(RECOMMENDATIONS.first().map(|entry| entry.0), Some(0));
        assert!(RECOMMENDATIONS
            .windows(2)
            .all(|entries| entries[0].0 < entries[1].0));
    }

    #[test]
    fn recommends_each_model_at_its_threshold() {
        for &(threshold, model_path) in RECOMMENDATIONS {
            assert_eq!(recommend_llm_for(threshold), model_path);
        }
    }

    #[test]
    fn keeps_previous_model_below_the_next_threshold() {
        for entries in RECOMMENDATIONS.windows(2) {
            assert_eq!(recommend_llm_for(entries[1].0 - 1), entries[0].1);
        }
    }

    #[test]
    fn resolves_auto_to_a_model_path() {
        assert_eq!(resolve_model_path_for("auto", 0), RECOMMENDATIONS[0].1);
    }

    #[test]
    fn leaves_explicit_model_paths_unchanged() {
        let path = "./model.gguf";
        assert_eq!(resolve_model_path_for(path, 0), path);
    }
}
