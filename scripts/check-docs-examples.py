"""Execute marked CLI examples and compare documented flags/defaults with --help."""

import json
import re
import shlex
import subprocess
from decimal import Decimal, InvalidOperation
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "website/content/docs"


def run(command):
    try:
        return subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True, timeout=300).stdout
    except subprocess.CalledProcessError as error:
        raise SystemExit(f"Command failed: {shlex.join(command)}\n{error.stderr}") from error


def assert_subset(expected, actual, context):
    if isinstance(expected, dict):
        for key, value in expected.items():
            assert key in actual, f"{context}: missing {key}"
            assert_subset(value, actual[key], f"{context}.{key}")
    else:
        assert expected == actual, f"{context}: expected {expected!r}, got {actual!r}"


def equal_default(documented, actual):
    try:
        return Decimal(documented) == Decimal(actual)
    except InvalidOperation:
        return documented == actual


def check_reference(source, help_text):
    options = {}
    current = None
    for line in help_text.split("\nExamples:", 1)[0].splitlines():
        match = re.search(r"^\s+(?:-([a-zA-Z]), )?--([\w-]+)", line)
        if match:
            current = match[2]
            options[current] = {"alias": match[1], "default": None}
        default = re.search(r"\[default: ([^]]+)\]", line)
        if default and current:
            options[current]["default"] = default[1]
    documented = set()
    for line in source.splitlines():
        if not line.startswith("| `--"):
            continue
        flag, alias, default, _ = [part.strip() for part in line.strip("|").split("|")]
        name = flag.strip("`").removeprefix("--")
        assert name in options, f"Unknown documented flag: {name}"
        documented.add(name)
        expected_alias = alias.strip("`").removeprefix("-") if alias.startswith("`") else None
        assert expected_alias == options[name]["alias"], f"Alias changed for --{name}"
        actual = options[name]["default"]
        if actual is not None:
            assert default.startswith("`"), f"Missing documented default for --{name}"
            assert equal_default(default.strip("`"), actual), f"Default changed for --{name}: {actual}"
        else:
            assert not default.startswith("`"), f"Unexpected documented default for --{name}"
    assert documented == set(options), f"Undocumented flags: {set(options) - documented}"
    return len(documented)


def main():
    count = 0
    for path in sorted(DOCS.glob("*.md")):
        source = path.read_text(encoding="utf-8")
        examples = re.findall(r"<!-- docs-check: ([\w-]+) -->\s*```bash\n(.*?)\n```", source, re.S)
        for name, command in examples:
            args = shlex.split(command.replace("\\\n", " "))
            # No shell execution: these small examples must invoke the CLI and emit JSON.
            assert args[:4] == ["cargo", "run", "--release", "--locked"], name
            assert args[4] == "--" and "--json" in args, name
            result = json.loads(run(args))
            expected = re.search(rf"<!-- docs-result: {re.escape(name)} -->\s*```json\n(.*?)\n```", source, re.S)
            assert expected, f"{name}: missing expected JSON fields"
            assert_subset(json.loads(expected[1]), result, name)
            count += 1
    assert count >= 2, "Expected marked quickstart and unsolvable examples"
    help_text = run(["cargo", "run", "--release", "--locked", "--", "--help"])
    flags = check_reference((DOCS / "cli.md").read_text(encoding="utf-8"), help_text)
    print(f"Docs examples: {count} CLI examples and {flags} flags/defaults passed.")


if __name__ == "__main__":
    main()
