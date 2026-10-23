"""Regression tests for the documentation checks, using small synthetic fixtures."""

import importlib.util
import tempfile
import unittest
from pathlib import Path


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


links = load("check-docs-links")
examples = load("check-docs-examples")


class LinkChecks(unittest.TestCase):
    def test_root_and_subpath_links_and_failures(self):
        for base in ("/", "/project/"):
            with self.subTest(base=base), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                (root / "guide").mkdir()
                (root / "guide/index.html").write_text('<h1 id="intro">Guide</h1><a href="../">Home</a>')
                (root / "icon.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg"/>')
                prefix = f'<link rel="canonical" href="https://example.test{base}"><h1>Home</h1>'
                valid = '<a href="./guide/#intro">Guide</a><img src="./icon.svg" alt="">'
                (root / "index.html").write_text(prefix + valid)
                self.assertEqual(links.check_site(root)[0], [])
                for bad, message in (
                    ('<a href="./missing/">Missing</a>', 'missing target'),
                    ('<a href="./guide/#absent">Anchor</a>', 'missing anchor'),
                    ('<script src="./missing.js"></script>', 'missing target'),
                    ('<img src="./icon.svg">', 'missing alt'),
                ):
                    (root / "index.html").write_text(prefix + valid + bad)
                    self.assertTrue(any(message in error for error in links.check_site(root)[0]))
                if base != "/":
                    (root / "index.html").write_text(prefix + '<a href="/guide/">Wrong base</a>')
                    self.assertTrue(any('escapes site base' in error for error in links.check_site(root)[0]))

    def test_unbuilt_site_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertTrue(links.check_site(Path(tmp))[0])

    def test_legacy_bookmark_destination_is_checked(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'index.html').write_text('<link rel="canonical" href="https://example.test/"><h1>Home</h1>')
            legacy = root / 'legacy.astro'
            legacy.write_text("'#old': 'missing/#section',")
            self.assertTrue(any('missing target' in error for error in links.check_site(root, legacy)[0]))


class ReferenceChecks(unittest.TestCase):
    def test_defaults_aliases_and_new_flags(self):
        source = '| `--rate` | `-r` | `0.10` | Mutation rate |'
        help_text = '  -r, --rate <VALUE>\n          Rate [default: 0.1]\n'
        self.assertEqual(examples.check_reference(source, help_text), 1)
        for changed in (
            help_text.replace('0.1', '0.2'),
            help_text.replace('-r,', '-m,'),
            help_text + '      --new-flag\n          New flag\n',
        ):
            with self.assertRaises(AssertionError):
                examples.check_reference(source, changed)

    def test_expected_output_checks_nested_fields(self):
        examples.assert_subset({'best': {'conflicts': 0}}, {'best': {'conflicts': 0, 'other': 1}}, 'run')
        with self.assertRaises(AssertionError):
            examples.assert_subset({'best': {'conflicts': 0}}, {'best': {'conflicts': 2}}, 'run')


if __name__ == '__main__':
    unittest.main()
