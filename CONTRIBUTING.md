# Contributing to NobodyWho

First off, thanks for taking the time to contribute! 🎉

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/nobodywho.git`
3. Create a new branch: `git checkout -b feature/amazing-feature`
4. Make your changes
5. Push to your fork: `git push origin feature/amazing-feature`
6. Open a Pull Request

## Development Setup

### On Linux or WSL

1. Install Nix package manager (if you haven't already)
2. Enable flakes: Add `experimental-features = nix-command flakes` to your Nix config
3. Run `nix develop` from any directory in the repo. To activate a development shell with rustup and libclang.
4. Install the stable rust toolchain using rustup (if you haven't already).
5. To compile the plugin: run `cargo build` from the nobodywho dir to build the plugin.
6. Set the TEST_MODEL env var to be a path to a Qwen 2.5 1.5B Instruct model in the GGUF format.
7. To run unit tests: run `cargo test -- --nocapture --test-threads=1` from the nobodywho dir
8. When done, run `nix flake check` to run all tests.

### On Windows

1. Install rustup and the rust stable toolchain
2. Install cmake, llvm, and msvc.
3. Install the Vulkan SDK, and set the VULKAN_SDK environment variable.
4. To compile the plugin: run `cargo build` from the nobodywho dir to build the plugin.
5. Set the TEST_MODEL env var to be a path to a Qwen 2.5 1.5B Instruct model in the GGUF format.
6. To run unit tests: run `cargo test -- --nocapture --test-threads=1` from the nobodywho dir

## Pull Request Process

1. Make sure all tests pass
2. Link any relevant issues in your PR description
3. The PR will be merged once you have the sign-off of at least one maintainer

## Code Style

- Follow the existing code style
- Use meaningful variable and function names
- Write tests for new features
- Keep commits atomic and write clear commit messages

## Community

- Join our [Discord](https://discord.gg/qhaMc2qCYB) or [Matrix](https://matrix.to/#/#nobodywho:matrix.org) for discussions
- Be nice to others (see our Code of Conduct)
- Ask questions if you're stuck

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (see LICENSE file). 
