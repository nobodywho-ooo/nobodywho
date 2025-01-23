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

1. Install Nix package manager (if you haven't already)
2. Enable flakes: Add `experimental-features = nix-command flakes` to your Nix config
3. Run `nix develop` to enter the development shell
4. when done, run `nix flake check` to run tests.

## Pull Request Process

1. Update the example game if your changes affect the API
2. Make sure all tests pass
3. Link any relevant issues in your PR description
4. The PR will be merged once you have the sign-off of at least one maintainer

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