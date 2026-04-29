# Installing nobodywho-rust from GitHub

## Step 1 — Install Rust

All platforms need the Rust stable toolchain. If you don't have it:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then restart your terminal and verify:

```sh
rustc --version
```

---

## Step 2 — Install system dependencies

### Linux (Debian/Ubuntu)

```sh
sudo apt update
sudo apt install -y cmake clang libclang-dev build-essential
```

**GPU acceleration (Vulkan) — required on x86_64/aarch64:**

```sh
sudo apt install -y libvulkan-dev vulkan-tools
```

If you want to use a specific GPU driver's Vulkan support:

```sh
sudo apt install -y nvidia-vulkan-icd   # NVIDIA
# or
sudo apt install -y mesa-vulkan-drivers  # AMD / Intel
```

### Linux (Fedora/RHEL)

```sh
sudo dnf install -y cmake clang clang-devel gcc gcc-c++
sudo dnf install -y vulkan-devel vulkan-tools  # for GPU
```

### macOS

Install Xcode Command Line Tools:

```sh
xcode-select --install
```

This provides clang and Metal (Metal is used automatically for GPU — no extra setup needed).

OpenMP is not included in Apple's clang, so install it via Homebrew:

```sh
brew install libomp
```

Then set these environment variables before building (add them to your shell profile):

```sh
export CC=/usr/bin/clang
export CXX=/usr/bin/clang++
export LDFLAGS="-L$(brew --prefix libomp)/lib"
export CPPFLAGS="-I$(brew --prefix libomp)/include"
```


### Windows

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload selected.

2. Install [CMake](https://cmake.org/download/) — make sure to add it to PATH during install.

3. Install [LLVM](https://github.com/llvm/llvm-project/releases) — needed for bindgen. Add the `bin` directory to your PATH.

4. Install the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home#windows) — required for GPU acceleration. The installer sets `VULKAN_SDK` automatically, but verify it is set:

```powershell
echo $env:VULKAN_SDK
```

---

## Step 3 — Add the dependency

In your project's `Cargo.toml`:

```toml
[dependencies]
nobodywho-rust = { git = "https://github.com/nobodywho-ooo/nobodywho", subdirectory = "nobodywho/rust" }
```

You can pin to a specific commit for reproducible builds:

```toml
nobodywho-rust = { git = "https://github.com/nobodywho-ooo/nobodywho", subdirectory = "nobodywho/rust", rev = "abc1234" }
```

---

## Step 4 — Build

```sh
cargo build
```

The first build will take several minutes — llama.cpp is compiled from source as part of the process.

---

## Verifying GPU works

```rust
use nobodywho_rust::Model;

let model = Model::builder("your-model.gguf")
    .use_gpu(true)  // default, shown for clarity
    .build()
    .unwrap();
```

If GPU initialisation fails, the library logs a warning and falls back to CPU automatically.

---

## Troubleshooting

| Error | Fix |
|---|---|
| `could not find libclang` | Install `libclang-dev` (Linux) or LLVM (Windows) |
| `cmake not found` | Install cmake and ensure it's on PATH |
| `Vulkan SDK not found` | Install Vulkan SDK and verify `VULKAN_SDK` env var is set |
| `ld: library not found for -lomp` (macOS) | Run `brew install libomp` and set the env vars above |
| Very slow first build | Normal — llama.cpp compiles from source |
| `gmake: Makefile: No such file or directory ... gmake: *** No rule to make target 'Makefile'.` | `cargo clean` and then  try `cargo build` again. |
