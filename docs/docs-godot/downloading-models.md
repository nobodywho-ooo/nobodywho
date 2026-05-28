# Downloading models
_How NobodyWho downloads, caches, and inspects GGUF models in Godot._

---

NobodyWho can either load a model from a path on disk or download it for you on first use, caching it for subsequent runs. This page covers the available model path formats, how to observe a download in progress, how to access gated/private models, and how to inspect what's already in the local cache.

## Supported model path formats

The `model_path` field on `NobodyWhoModel` (and `projection_model_path` for vision models) accepts several forms:

| Form | Example | Notes |
| ---- | ------- | ----- |
| Godot resource path | `res://models/my-model.gguf` | Bundled with your game export |
| User data path | `user://downloaded.gguf` | Written by your game at runtime |
| Absolute filesystem path | `/opt/models/foo.gguf` | Local file |
| HuggingFace reference | `huggingface:owner/repo/file.gguf` or `hf://owner/repo/file.gguf` | Downloaded and cached on first use |
| HTTPS URL | `https://example.com/model.gguf` | Downloaded and cached on first use |

Remote models are downloaded to the platform cache directory on first load and re-used on subsequent runs. Downloads happen on a background thread — the Godot main loop stays responsive while a multi-GB model is fetched.

## Showing download progress

`NobodyWhoModel` emits a `download_progress(downloaded, total)` signal while a remote model is downloading, throttled to roughly 10 Hz with a guaranteed final emit on completion. Connect it if you'd like to drive a progress bar:

```gdscript
model.download_progress.connect(func(downloaded: int, total: int):
    print("%d / %d bytes" % [downloaded, total])
)
```

The signal is not emitted for local files or already-cached downloads.

## Downloading a gated model

Some HuggingFace models are private or gated by a license you need to accept. In both cases you need to be authorized to download the model weights.

You can manually download the GGUF file via your web browser and then point your `NobodyWhoModel` at the local path.

Alternatively, use the `NobodyWhoDownloader` node, which lets you pass an authorization header:

```gdscript
var dl = NobodyWhoDownloader.new()
dl.model_path = "huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
dl.headers = {"Authorization": "Bearer your_hf_token"}
dl.download_complete.connect(func(local_path: String):
    get_node("../ChatModel").model_path = local_path
)
dl.download_failed.connect(func(error: String):
    push_error("Download failed: " + error)
)
dl.start_download()
add_child(dl)
```

You can generate a HuggingFace token in [your account settings](https://huggingface.co/settings/tokens).

## Inspecting the model cache

`NobodyWhoModel.get_cached_models()` is a static function that returns every `.gguf` model in NobodyWho's cache directory, paired with its size in bytes. This is the same cache used by `NobodyWhoDownloader` and by `NobodyWhoModel`'s `huggingface:` paths.

```gdscript
for entry in NobodyWhoModel.get_cached_models():
    print("%s: %d bytes" % [entry["path"], entry["size"]])
```

Each entry is a `Dictionary` with two keys:

- `"path"` — absolute path to the cached `.gguf` file
- `"size"` — size in bytes

The array is empty if nothing has been downloaded yet. On error the function returns `null` and logs a Godot error to the console.
