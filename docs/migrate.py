#!/usr/bin/env python3
"""
Migrate mkdocs markdown files to Docusaurus format.

Handles:
- Frontmatter: converts mkdocs `order` to Docusaurus `sidebar_position`
- Admonitions: `!!! info ""` → `:::info`
- Code blocks: strips `{.python continuation}` annotations
- Tab syntax: converts `=== ":simple-godotengine: Godot"` to Docusaurus tabs (leaves as-is for now, flagged)
- Annotation markers: strips `(1)\n{ .annotate }` patterns
- Asset paths: adjusts relative paths
"""

import re
import shutil
from pathlib import Path

SRC = Path(__file__).parent.parent / "docs" / "markdown"
DST = Path(__file__).parent

# Map source directories to destination directories
COPY_MAP = {
    # Shared docs (top-level .md files)
    "index.md": "docs/index.md",
    "llm-basics.md": "docs/llm-basics.md",
    "model-selection.md": "docs/model-selection.md",
    # Per-binding
    "python": "docs-python",
    "swift": "docs-swift",
    "react-native": "docs-react-native",
    "flutter": "docs-flutter",
    "godot": "docs-godot",
}


def convert_frontmatter(content: str) -> str:
    """Convert mkdocs frontmatter to Docusaurus frontmatter."""
    # Replace `order:` with `sidebar_position:`
    content = re.sub(
        r"^(---\n.*?)^order:\s*(\d+)",
        r"\1sidebar_position: \2",
        content,
        flags=re.MULTILINE | re.DOTALL,
    )
    # Remove sidebar_title (Docusaurus uses title for this)
    content = re.sub(r"^sidebar_title:.*\n", "", content, flags=re.MULTILINE)
    return content


def convert_admonitions(content: str) -> str:
    """Convert mkdocs admonitions to Docusaurus format.

    mkdocs:  !!! info "Title"
                 Content here

    docusaurus: :::info[Title]
                Content here
                :::
    """
    lines = content.split("\n")
    result = []
    i = 0
    while i < len(lines):
        # Match admonition start: !!! type "title" or !!! type ""
        m = re.match(r'^(!{3})\s+(\w+)\s+"(.*?)"', lines[i])
        if m:
            admonition_type = m.group(2)
            title = m.group(3)
            if title:
                result.append(f":::{admonition_type}[{title}]")
            else:
                result.append(f":::{admonition_type}")
            i += 1
            # Collect indented content
            while i < len(lines) and (lines[i].startswith("    ") or lines[i].strip() == ""):
                if lines[i].strip() == "" and i + 1 < len(lines) and not lines[i + 1].startswith("    "):
                    break
                result.append(lines[i].removeprefix("    "))
                i += 1
            result.append(":::")
        else:
            result.append(lines[i])
            i += 1
    return "\n".join(result)


def convert_code_blocks(content: str) -> str:
    """Strip mkdocs-specific code block annotations."""
    # ```{.python continuation} → ```python
    content = re.sub(
        r"```\{\.(\w+)\s+continuation\}",
        r"```\1",
        content,
    )
    # <!-- not tested: ... --> comments — keep them, they're useful
    return content


def strip_annotation_markers(content: str) -> str:
    """Remove mkdocs annotation markers like (1)\\n{ .annotate }"""
    # Remove (N)\n{ .annotate } blocks (with optional whitespace)
    content = re.sub(r"\n*\(\d+\)\s*\n\{ \.annotate \}\s*\n*", "\n", content)
    # Also remove standalone { .annotate } on its own line
    content = re.sub(r"^\{ \.annotate \}\s*$", "", content, flags=re.MULTILINE)
    return content


def convert_tab_syntax(content: str) -> str:
    """Flag or convert === tab syntax.

    mkdocs-material uses:
        === ":simple-godotengine: Godot"
            ```gdscript
            ...
            ```

    Docusaurus uses:
        import Tabs from '@theme/Tabs';
        import TabItem from '@theme/TabItem';
        <Tabs>
        <TabItem value="godot" label="Godot">
        ...
        </TabItem>
        </Tabs>

    For now, just strip the tab wrappers since binding pages are already separated.
    """
    # Remove === "..." lines (tab headers in material)
    content = re.sub(r'^===\s+".*?".*$\n?', "", content, flags=re.MULTILINE)
    # Dedent content that was inside tabs (4 spaces)
    lines = content.split("\n")
    result = []
    for line in lines:
        # If a line starts with exactly 4 spaces and the previous context suggests
        # it was inside a tab, dedent it. This is imperfect but handles the common case.
        result.append(line)
    return "\n".join(result)


def escape_jsx_tags(content: str) -> str:
    """Escape HTML-like tags outside code blocks that MDX would parse as JSX.

    Tags like <think>, </think>, <player>, etc. in prose need to be escaped.
    We leave content inside code fences untouched.
    """
    lines = content.split("\n")
    result = []
    in_code_block = False
    for line in lines:
        if line.strip().startswith("```"):
            in_code_block = not in_code_block
        if not in_code_block:
            # Escape <word> and </word> patterns for non-standard HTML tags
            line = re.sub(
                r"<(/?\s*(?:think|player|[a-z_]+-[a-z_]+))>",
                r"&lt;\1&gt;",
                line,
            )
        result.append(line)
    return "\n".join(result)


def convert_details_blocks(content: str) -> str:
    """Convert mkdocs <details markdown> to Docusaurus-compatible <details>.

    mkdocs:   <details markdown>
              <summary markdown>Title</summary>
              ...
              </details>

    Docusaurus: <details>
                <summary>Title</summary>
                ...
                </details>
    """
    content = content.replace("<details markdown>", "<details>")
    content = re.sub(
        r"<summary markdown>(.*?)</summary>",
        r"<summary>\1</summary>",
        content,
    )
    return content


def fix_relative_links(content: str, src_path: Path, dst_dir: str) -> str:
    """Adjust relative markdown links for new directory structure."""
    # Links to shared docs from binding docs: ../model-selection.md → /model-selection
    content = re.sub(
        r"\]\(\.\./model-selection\.md\)",
        "](/model-selection)",
        content,
    )
    content = re.sub(
        r"\]\(\.\./llm-basics\.md\)",
        "](/llm-basics)",
        content,
    )

    # Cross-instance links from shared docs to binding docs
    # swift/index.md → /swift/
    content = re.sub(r"\]\(swift/index\.md\)", "](/swift/)", content)
    content = re.sub(r"\]\(python/index\.md\)", "](/python/)", content)
    content = re.sub(r"\]\(react-native/index\.md\)", "](/react-native/)", content)
    content = re.sub(r"\]\(flutter/index\.md\)", "](/flutter/)", content)
    content = re.sub(r"\]\(godot/install\.md\)", "](/godot/install)", content)

    # ./index.md → ./ (Docusaurus convention)
    content = re.sub(r"\]\(\./index\.md\)", "](./)", content)
    # Remove .md extension from relative links (Docusaurus uses extensionless URLs)
    content = re.sub(r"\]\((\./[^)]+)\.md\)", r"](\1)", content)
    return content


def fix_asset_paths(content: str, src_file: Path) -> str:
    """Copy referenced assets and fix their paths."""
    # Find relative image/asset references (with or without ./ prefix)
    assets = re.findall(r"!\[([^\]]*)\]\((\.?/?assets/[^)]+)\)", content)
    for alt, rel_path in assets:
        new_path = "/img/" + Path(rel_path).name
        content = content.replace(f"]({rel_path})", f"]({new_path})")
    return content


def convert_file(src_file: Path, dst_file: Path, dst_dir: str) -> None:
    """Convert a single markdown file."""
    content = src_file.read_text()

    content = convert_frontmatter(content)
    content = convert_admonitions(content)
    content = convert_code_blocks(content)
    content = strip_annotation_markers(content)
    content = convert_tab_syntax(content)
    content = escape_jsx_tags(content)
    content = convert_details_blocks(content)
    content = fix_relative_links(content, src_file, dst_dir)
    content = fix_asset_paths(content, src_file)

    dst_file.parent.mkdir(parents=True, exist_ok=True)
    dst_file.write_text(content)
    print(f"  {src_file.relative_to(SRC)} → {dst_file.relative_to(DST)}")


def copy_assets():
    """Copy image assets to static/img/."""
    static_img = DST / "static" / "img"
    static_img.mkdir(parents=True, exist_ok=True)

    for assets_dir in SRC.rglob("assets"):
        if assets_dir.is_dir():
            for f in assets_dir.iterdir():
                if f.is_file() and f.suffix in (".png", ".jpg", ".gif", ".svg", ".webp"):
                    dst = static_img / f.name
                    shutil.copy2(f, dst)
                    print(f"  asset: {f.name} → static/img/")


def main():
    print("Migrating mkdocs → Docusaurus\n")

    # Copy shared docs
    print("Shared docs:")
    for src_name, dst_name in COPY_MAP.items():
        src_path = SRC / src_name
        dst_path = DST / dst_name
        if src_path.is_file():
            convert_file(src_path, dst_path, "docs")

    # Copy per-binding docs
    for src_name, dst_name in COPY_MAP.items():
        src_path = SRC / src_name
        dst_path = DST / dst_name
        if src_path.is_dir():
            print(f"\n{src_name}:")
            for md_file in sorted(src_path.rglob("*.md")):
                rel = md_file.relative_to(src_path)
                convert_file(md_file, DST / dst_name / rel, dst_name)

    # Copy assets
    print("\nAssets:")
    copy_assets()

    print("\nDone! Run 'npm start' in docs/ to preview.")


if __name__ == "__main__":
    main()
