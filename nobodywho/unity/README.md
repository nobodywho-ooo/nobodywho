# Unity Minimal Project

A minimal Unity project setup with testing capabilities, using Nix for reproducible development environments.

## Features

- 🎮 Unity 6000.0.34f1
- 🧪 Unity Test Framework
- ❄️ Nix development environment
- 🔧 Just task runner for common operations

## Prerequisites

- Nix package manager
- Unity account (for license)
- Git

## Getting Started

1. Clone the repository:
   ```bash
   git clone https://github.com/emilnorsker/unity-dev-template.git
   cd unity-dev-template
   ```

2. Copy the environment template:
   ```bash
   cp .template.env .env
   ```

3. Fill in your Unity credentials in `.env`

4. Enter the development shell:
   ```bash
   nix develop
   ```

5. Start Unity:
   ```bash
   just unity
   ```

## Development

### Running Tests

Run all PlayMode tests with nice output:
```bash
just test
```

Example output:
```
🧪 Running Unity tests...

📊 Test Results Summary:
====================
✨ Total Tests: 1
✅ Passed: 1
❌ Failed: 0
⏭️  Skipped: 0
⏱️  Duration: 0.42 seconds
```

You can also run tests with custom parameters:
```bash
just unity -batchmode -runTests -testPlatform PlayMode
```

Test results will be saved to `test-results.xml` in the project root.

## Project Structure

- `UnityProject/` - Unity project files
- `flake.nix` - Nix development environment
- `justfile` - Task runner commands
- `.env` - Local environment configuration 