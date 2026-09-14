"""Generate a plain Markdown Wiki landing page linking to the canonical Pages guides."""

import json
import os
import re
import sys
from pathlib import Path

repository = os.environ.get("GITHUB_REPOSITORY", "andreashdez/n_queens_problem_rust")
owner, name = repository.split("/", 1)
site = f"https://{owner}.github.io/{name}/"
root = Path(__file__).resolve().parents[1]
pages = [
    "getting-started",
    "gui",
    "cli",
    "algorithm",
    "tuning",
    "benchmarks",
    "library",
    "development",
]
lines = [
    "# N-Queens in Rust",
    "",
    "A Rust genetic solver with a native desktop GUI, CLI, and library API.",
    "",
    f"The full documentation lives on **[GitHub Pages]({site})**.",
    "This index links to the current guides so the Wiki and website stay in sync.",
    "",
]
for slug in pages:
    source = (root / "website/content/docs" / f"{slug}.md").read_text(encoding="utf-8")
    # Page metadata uses JSON-quoted YAML scalars to keep this generator dependency-free.
    title = json.loads(re.search(r"^title: (.+)$", source, re.MULTILINE)[1])
    description = json.loads(re.search(r"^description: (.+)$", source, re.MULTILINE)[1])
    lines.append(f"- **[{title}]({site}{slug}/)** — {description}")
lines.extend(["", f"[Source repository](https://github.com/{repository})", ""])
Path(sys.argv[1]).write_text("\n".join(lines), encoding="utf-8")
