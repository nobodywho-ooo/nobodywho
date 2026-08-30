#!/usr/bin/env python3

import re
from pathlib import Path

SITE_URL = "https://docs.nobodywho.ooo"
DESCRIPTION = (
    "Local LLM inference for Kotlin, Python, Swift, React Native, Flutter, "
    "and Godot, built on llama.cpp."
)
ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "_site"
SECTIONS = {
    "Basics": ROOT / "docs",
    "Kotlin": ROOT / "kotlin",
    "Python": ROOT / "python",
    "Swift": ROOT / "swift",
    "React Native": ROOT / "react-native",
    "Flutter": ROOT / "flutter",
    "Godot": ROOT / "godot",
}


def split_frontmatter(*, text: str) -> tuple[dict[str, str], str]:
    if not text.startswith("---\n"):
        return {}, text

    _, frontmatter, content = text.split("---\n", 2)
    metadata = {}
    for line in frontmatter.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        metadata[key.strip()] = value.strip().strip('"\'')
    return metadata, content


def title_for(*, path: Path, metadata: dict[str, str], content: str) -> str:
    if metadata.get("title"):
        return metadata["title"]
    heading = re.search(pattern=r"^#\s+(.+)$", string=content, flags=re.MULTILINE)
    return heading.group(1) if heading else path.stem.replace("-", " ").title()


def url_for(*, section_path: Path, path: Path) -> str:
    relative = path.relative_to(section_path).with_suffix("")
    route = "/".join(relative.parts)
    return "" if route == "index" else route


def write_page_markdown(*, path: Path) -> None:
    metadata, content = split_frontmatter(text=path.read_text(encoding="utf-8"))
    title = title_for(path=path, metadata=metadata, content=content)
    target = OUTPUT / path.relative_to(ROOT).with_suffix(".html.md")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(f"# {title}\n\n{content.strip()}\n", encoding="utf-8")


def main() -> None:
    index_lines = [
        "# NobodyWho",
        "",
        f"> {DESCRIPTION}",
        "",
        (
            "This file lists the current documentation. Read "
            f"[the full current documentation]({SITE_URL}/llms-full.txt) in one file."
        ),
    ]
    full_lines = ["# NobodyWho documentation", ""]

    for section, section_path in SECTIONS.items():
        index_lines.extend(["", f"## {section}", ""])
        full_lines.extend([f"## {section}", ""])

        paths = list(section_path.glob("*.md"))
        paths.sort(key=lambda path: path.name)
        for path in paths:
            metadata, content = split_frontmatter(text=path.read_text(encoding="utf-8"))
            title = title_for(path=path, metadata=metadata, content=content)
            route = url_for(section_path=section_path, path=path)
            suffix = f"/{route}" if route else "/"
            index_lines.append(f"- [{title}]({SITE_URL}/{section_path.name}{suffix})")
            full_lines.extend([f"### {title}", "", content.strip(), "", "---", ""])

    OUTPUT.mkdir(parents=True, exist_ok=True)
    (OUTPUT / "llms.txt").write_text("\n".join(index_lines) + "\n", encoding="utf-8")
    (OUTPUT / "llms-full.txt").write_text("\n".join(full_lines), encoding="utf-8")

    page_paths = [ROOT / "index.qmd"]
    for section_path in SECTIONS.values():
        page_paths.extend(section_path.rglob("*.md"))
    for path in page_paths:
        write_page_markdown(path=path)

    for path in OUTPUT.rglob("*.html"):
        html = path.read_text(encoding="utf-8")
        path.write_text(html.replace(" – NobodyWho</title>", " | NobodyWho</title>"), encoding="utf-8")


if __name__ == "__main__":
    main()
