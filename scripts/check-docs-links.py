"""Check a static Astro build without a server, browser, or network access."""

import argparse
import re
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urljoin, urlsplit


class Document(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.ids = set()
        self.links = []
        self.canonical = None
        self.headings = 0
        self.missing_alt = 0
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        if tag == "h1":
            self.headings += 1
        if tag == "img" and "alt" not in attrs:
            self.missing_alt += 1
        if tag == "link" and attrs.get("rel") == "canonical":
            self.canonical = attrs.get("href")
            return
        for key in ("href", "src"):
            if attrs.get(key):
                self.links.append(attrs[key])


def check_site(build_dir, legacy=None):
    documents = {
        path.relative_to(build_dir).as_posix(): Document(path.read_text(encoding="utf-8"))
        for path in build_dir.rglob("*.html")
    }
    homepage = documents.get("index.html")
    if not homepage or not homepage.canonical:
        return ["Missing built homepage or canonical URL; run npm run build first."], 0
    site = urlsplit(homepage.canonical)
    base = site.path.rstrip("/") + "/"
    errors = []
    checked = 0

    def check_link(href, current):
        nonlocal checked
        target = urlsplit(urljoin(homepage.canonical, current))
        target = urlsplit(urljoin(target.geturl(), href))
        if target.scheme not in ("http", "https") or target.netloc != site.netloc:
            return
        if not target.path.startswith(base):
            errors.append(f"{current}: link escapes site base {base}: {href}")
            return
        name = unquote(target.path[len(base):])
        if not name or name.endswith("/"):
            name += "index.html"
        path = build_dir / name
        if not path.resolve().is_relative_to(build_dir.resolve()) or not path.is_file():
            errors.append(f"{current}: missing target {href}")
            return
        if target.fragment and name in documents:
            if unquote(target.fragment) not in documents[name].ids:
                errors.append(f"{current}: missing anchor {href}")
        checked += 1

    for name, doc in documents.items():
        if doc.headings != 1:
            errors.append(f"{name}: expected one h1, got {doc.headings}")
        if doc.missing_alt:
            errors.append(f"{name}: {doc.missing_alt} images missing alt attributes")
        for href in doc.links:
            check_link(href, name)
        # Astro generates a /404/ canonical URL for its special 404.html output.
        if name != "404.html" and doc.canonical:
            check_link(doc.canonical, name)
    if legacy:
        for destination in re.findall(r"'#[^']+': '([^']+)'", legacy.read_text(encoding="utf-8")):
            check_link(destination, "index.html")
    return errors, checked


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build_dir", nargs="?", type=Path, default=Path("dist"))
    args = parser.parse_args()
    legacy = Path("website/components/LegacyDocsLinks.astro")
    errors, checked = check_site(args.build_dir, legacy if legacy.exists() else None)
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Docs links: {checked} internal links, assets, and bookmarks passed.")


if __name__ == "__main__":
    main()
