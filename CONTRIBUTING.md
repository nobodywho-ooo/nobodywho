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

### Dev tools (all platforms)

Run the setup script once after cloning:

```sh
./setup.sh
```

This installs `just` (if not already present) and wires up the pre-push hook. After that, `just check` is available as a manual command and tolerates uncommitted changes. On `git push`, the hook runs `just check --require-clean`, which also fails if formatting or generated files differ from what is committed.

### On Linux or WSL

1. Install Nix package manager (if you haven't already)
2. Enable flakes: Add `experimental-features = nix-command flakes` to your Nix config
3. Run `nix develop` from any directory in the repo. This activates a development shell with all required tools and sets up the pre-push hook automatically — no need to run `setup.sh`.
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
3. If the change affects users, add a change file with `just change` (see [Changelog entries](#changelog-entries)). Otherwise ask a maintainer to add the `no-changelog` label; CI fails without one or the other. Don't edit `CHANGELOG.md` directly: CI fails that too, unless a maintainer adds the `edit-changelog` label for a deliberate edit such as fixing a past entry.
4. The PR will be merged once you have the sign-off of at least one maintainer

## Changelog entries

Pending changes are described as files in [`.changeset/`](.changeset/) rather than edited into [`CHANGELOG.md`](CHANGELOG.md) directly. Each binding is versioned and released on its own, and at release time these files are turned into a dated `CHANGELOG.md` entry and each released binding's next version. `just change` asks which bindings the change affects and how, which changelog section it belongs in, a description and a file name, and writes a file like this:

```markdown
---
section: changed
bindings:
  python: minor
  flutter: major
---

One or two sentences describing the change for users.
```

- `section` is the Keep a Changelog section: `added`, `changed`, `deprecated`, `removed`, `fixed` or `security`.
- Under `bindings`, list every binding whose users will notice the change, and only those. A change in `core/` usually affects all six: `python`, `godot`, `flutter`, `kotlin`, `react-native` and `swift`.
- Pick the bump per binding: `major` if existing code can break (removed or renamed API, changed signature or behaviour, newly rejected input), `minor` for new functionality or other changes, `patch` for bug fixes. `just change` gives every binding the same bump; edit the file if they should differ.
- Don't name the affected bindings or mark the change as breaking in the text; `CHANGELOG.md` adds both from the frontmatter.
- Write two files if the wording should differ between bindings.

`just check-changesets` validates the files, and `just next-versions` previews the `CHANGELOG.md` entry they would produce. Don't add entries for internal maintenance, CI, or documentation-only changes.

At release time, `just prepare-release` bumps the version files, writes the `CHANGELOG.md` entry (and Flutter's pub.dev changelog) and deletes the consumed change files. The [release skill](.agents/skills/release/SKILL.md) describes the full process, including tagging.

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
