# NobodyWho Godot tests

Headless GDScript test suites for the NobodyWho Godot bindings rewrite. Run
from the CLI — no editor, no window.

## Prerequisites

1. **Build the Rust extension first.** From the repo root:

   ```sh
   cargo build -p nobodywho-godot
   ```

   The test project's `nobodywho.gdextension` points at
   `res://../../target/debug/libnobodywho_godot.so` (release build at
   `../../target/release/...`). Recompile whenever you change Rust code.

2. **Godot 4.6+.** The bindings target gdext API 4.6. On Nix:

   ```sh
   nix shell nixpkgs#godot_4
   ```

   (resolves to a 4.7.x at time of writing). Or use any local
   `godot`/`godot4` binary >= 4.6.

## Running the tests

From this directory:

```sh
# 1. One-time import (builds the .godot/ cache so extension classes resolve).
#    This step aborts at editor shutdown with exit code 134 — that's a known
#    gdext + headless-editor-exit hiccup and is harmless; the cache is written
#    before the abort. You only need to repeat it after deleting .godot/.
nix shell nixpkgs#godot_4 --command godot --headless --import --path .

# 2. Run the suite.
nix shell nixpkgs#godot_4 --command godot --headless --path .
```

Or as a single line, skipping the noisy import output:

```sh
nix shell nixpkgs#godot_4 --command godot --headless --import --path . >/dev/null 2>&1 || true
nix shell nixpkgs#godot_4 --command godot --headless --path .
```

### Exit codes

- `0` — all tests passed.
- `1` — at least one failure (see `FAIL:` lines / `push_error` output).
- `124` — timeout (a test hung; one of the latch/async primitives regressed).

### Expected noise

- The `task: blocking-panic` test deliberately panics inside
  `on_blocking_thread`; gdext logs it as `ERROR: [panic ...] deliberate panic
  in on_blocking_thread` followed by `NobodyWhoTask._test_blocking_panic:
  closure panicked`. The test then asserts the result is `null` and passes —
  those error lines are the *expected* behavior, not a failure.
- A few `ObjectDB instances were leaked at exit` / `resources still in use`
  warnings may appear at shutdown. They're cosmetic (spawned task objects
  outliving the immediate quit) and don't affect the exit code.
- The `prompt_test: create aborts on a bad element` case deliberately
  feeds `NobodyWhoPrompt.create` four malformed parts, each logging a
  `godot_error!` (`part is not a Dictionary`, `unknown part type`, missing
  field) and returning `null`. Those error lines are the *expected*
  behavior, not a failure — the test asserts each call returns null.

## Model-backed tests

Some suites need a live model or a download, which needs a source path.
They self-skip when their environment variable is unset, so the model-free
suites still run green on their own:

```sh
# Chat + tool-calling tests (GGUF LLM):
TEST_MODEL=/path/to/model.gguf \
  nix shell nixpkgs#godot_4 --command godot --headless --path .

# Text-to-speech (downloads from HF or a local dir):
TEST_TTS_SOURCE=hf://NobodyWho/Kokoro-82M \
  nix shell nixpkgs#godot_4 --command godot --headless --path .

# Speech-to-text (Whisper ONNX + an audio file with known speech):
TEST_STT_SOURCE=hf://onnx-community/whisper-base \
TEST_AUDIO_FILE=/path/to/hello.wav \
  nix shell nixpkgs#godot_4 --command godot --headless --path .

# Multimodal / vision (a GGUF with an MTMD projector + a known image).
# Set TEST_VISION_MMPROJ too if the projector is a separate file (it usually
# is for Gemma 3 / Qwen2-VL / Llama 3.2 Vision GGUFs):
TEST_VISION_MODEL=/path/to/multimodal.gguf \
TEST_VISION_MMPROJ=/path/to/mmproj.gguf \
TEST_IMAGE_FILE=/path/to/known_image.png \
  nix shell nixpkgs#godot_4 --command godot --headless --path .

# All at once:
TEST_MODEL=/path/to/model.gguf \
TEST_TTS_SOURCE=hf://NobodyWho/Kokoro-82M \
TEST_STT_SOURCE=hf://onnx-community/whisper-base \
TEST_AUDIO_FILE=/path/to/hello.wav \
  nix shell nixpkgs#godot_4 --command godot --headless --path .
```

A small CPU-friendly model works fine for `TEST_MODEL` (e.g.
`Qwen_Qwen3-0.6B-Q4_K_M.gguf`). The first run will download/load it and print
a lot of `llama_model_loader` / ONNX log noise — that's expected. Tests that
need a model read their env var in `run()` and print `SKIP: ...` if it's
empty, so a missing var never fails the suite or hangs.

## Layout

```
project.godot             # minimal headless project, main scene = tests.tscn
nobodywho.gdextension     # loads the built .so from ../../target/
tests.tscn                # main scene: a Node running test_runner.gd
test_runner.gd            # loads each *_test.gd suite, runs them, quits
chat_test.gd              # NobodyWhoChat query/mutation tests (needs TEST_MODEL)
tools_test.gd             # NobodyWhoTool tests (needs TEST_MODEL)
tts_test.gd               # NobodyWhoTextToSpeech tests (needs TEST_TTS_SOURCE)
stt_test.gd               # NobodyWhoSpeechToText tests (needs TEST_STT_SOURCE + TEST_AUDIO_FILE)
prompt_test.gd             # NobodyWhoPrompt tests (tier 1 model-less; tier 2 needs TEST_VISION_MODEL + TEST_IMAGE_FILE, optional TEST_VISION_MMPROJ)
```

## Adding a test suite

1. Create a new `*_test.gd` file, e.g. `chat_test.gd`:

   ```gdscript
   extends Node

   func run(runner: Node) -> void:
       # ...your async test body...
       # Call `await` freely. Report results through the runner:
       runner.ok("chat: some behavior")
       runner.fail("chat: expected X, got %s" % str(actual))
   ```

2. Register it in `test_runner.gd`:

   ```gdscript
   var suites: Array = [
       preload("res://chat_test.gd").new(),
       preload("res://tools_test.gd").new(),
       preload("res://tts_test.gd").new(),
       preload("res://stt_test.gd").new(),
       preload("res://your_test.gd").new(),   # <-- add here
   ]
   ```

3. Rebuild the Rust extension if your test touches newly added `#[func]`s,
   then rerun step 2 above. (`preload` by path avoids needing `class_name`
   global-class registration, so no re-import is needed for new scripts.)
4. If your suite needs a model, read `OS.get_environment("TEST_MODEL")` at
   the top of `run()` and `return` early with a `SKIP:` print when it's empty
   (see `chat_test.gd` for the pattern).

## GDScript pitfalls in this codebase

A few things that bit the test author and are worth knowing:

- **No `_ = expr` discard.** GDScript reserves `_` as a wildcard; you can't
  assign to it as a statement. To discard a return value, just call the
  expression as a statement: `await chat.set_system_prompt("x")`.
- **`:=` can't infer a method call on a `Variant`.** The query/mutation
  `#[func]`s return `Variant` (so they can be awaited whether pending or
  latched). `var x := chat.get_sampler_config().to_json()` fails with "cannot
  infer type" because the method call on a `Variant` has no static return
  type. Use an explicit type: `var x: String = ...`.
- **Don't `free()` a `RefCounted`.** `NobodyWhoChat`/`NobodyWhoModel`/
  `NobodyWhoTool`/`NobodyWhoTask`/`NobodyWhoTokenStream` are all `RefCounted`
  (refcount-managed). Calling `.free()` on them errors. Just let them drop
  out of scope.
- **Awaiting the async methods directly.** Every async `#[func]` —
  `create()` included — returns the internal task's `wait()` result (a
  value-or-Signal `Variant`), so `await chat.foo()` works directly; you
  never call `.wait()`. Await the return value **immediately**: storing it
  and awaiting after another await/frame may await an already-fired one-shot
  Signal and hang.

## Notes

- Model-backed tests live in `chat_test.gd` and are guarded behind the
  `TEST_MODEL` env var (see "Model-backed tests" above). Full end-to-end
  `ask()` streaming tests (which generate tokens and need a longer timeout)
  are a natural next addition — follow the same `TEST_MODEL` + skip pattern.
- The `_test_*` `#[func]` scaffolding from the rewrite's early phases has
  been removed; the latch/stream primitives are now exercised end-to-end by
  the model-backed `chat_test.gd` and `tools_test.gd` suites.
