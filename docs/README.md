# NobodyWho documentation

The documentation site uses [Quarto](https://quarto.org/) and deploys to Cloudflare Pages.

## Local development

Install Quarto 1.10 or newer, then run:

```bash
cd docs
quarto preview --host 127.0.0.1 --port 8765 --no-browser
```

Build the production site with:

```bash
quarto render
```

Quarto writes the site to `docs/_site/`. The build also creates `llms.txt`, `llms-full.txt`, and the Markdown files used by each page's copy button.

## Theme

The site uses Quarto's Cosmo theme with the styling from the [Inspect AI documentation](https://inspect.aisi.org.uk/). The copied theme code is MIT licensed. See `THIRD_PARTY_NOTICES.md`.

## Structure

The shared guides live in `docs/docs/`. Each language has its own directory, such as `docs/python/`.

- Files in `docs/<binding>/` document the latest release.
- Files in `docs/<binding>/main/` document the unreleased main branch.
- Files in `docs/<binding>/<version>/` document older releases.

The current release numbers live in `docs/versions.json`. Navigation and page order live in `docs/_quarto.yml`.

## Publishing a new version

First, move the current release files into a directory named after that release. Next, copy the files from `main/` into the binding directory. Then, update `versions.json` and the matching sidebar entries in `_quarto.yml`.

Keep the existing public routes when moving files. The latest release stays at `/<binding>/`, the main branch stays at `/<binding>/main/`, and older releases use `/<binding>/<version>/`.

## Tests

Python documentation tests scan the Markdown files under `docs/`. Flutter documentation tests read `docs/flutter/main/`. Keep executable examples in ordinary fenced code blocks.
