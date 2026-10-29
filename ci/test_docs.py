"""Documentation checks for Issue #128: the README and the settings
reference, in English and Japanese, and every setting and key name in them.

Run from the repo root: python -m unittest ci/test_docs.py
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
READMES = {"en": ROOT / "README.md", "ja": ROOT / "README.ja.md"}
REFERENCES = {"en": ROOT / "docs" / "reference.md", "ja": ROOT / "docs" / "reference.ja.md"}
CONFIG = ROOT / "crates" / "core" / "src" / "config.rs"
KEYS = ROOT / "crates" / "core" / "src" / "keys.rs"


def text(path):
    return path.read_text(encoding="utf-8")


def function(source, name):
    """The body of `fn name(` in `source`, up to the next fn at its indent."""
    at = source.index(f"fn {name}(")
    end = source.find("\n    fn ", at + 1)
    return source[at:end]


def setting_names():
    """The names [general] and [edge.*] take, as config.rs matches them,
    and those of key sets and profiles."""
    config = text(CONFIG)
    names = set()
    for fn in ("general", "edge"):
        for arm in re.findall(r'^\s*((?:"[a-z_]+"\s*\|\s*)*"[a-z_]+")\s*(?:if [^=]*)?=>', function(config, fn), re.M):
            names.update(re.findall(r'"([a-z_]+)"', arm))
    return names | {"key", "layout", "process", "left", "right", "top", "bottom"}


def kinds():
    """The kinds of key, as Action::kind names them."""
    return set(re.findall(r'Action::\w+(?:\(_\)|\s*\{\s*\.\.\s*\})?\s*=>\s*"([a-z]+)"', text(CONFIG)))


def key_names():
    """The key names keys.rs has by name, and the symbols it takes."""
    keys = text(KEYS)
    named = re.findall(r'\("([a-z]+)", Key::', keys)
    symbols = list(re.search(r'const SYMBOLS: &\[u8\] = b"(.*)";', keys).group(1).replace("\\\\", "\\"))
    return named, symbols


def code_spans(doc):
    """What `code` spans of `doc` hold, lower-cased."""
    spans = re.findall(r"``\s(.+?)\s``|`([^`]+)`", doc)
    return {(a or b).strip().lower() for a, b in spans}


class DocsTest(unittest.TestCase):
    # AC1
    def test_the_readme_in_english_and_japanese_link_each_other_and_say_the_same(self):
        en, ja = text(READMES["en"]), text(READMES["ja"])
        self.assertIn("(README.ja.md)", "\n".join(en.splitlines()[:5]), "English links to Japanese at the top")
        self.assertIn("(README.md)", "\n".join(ja.splitlines()[:5]), "Japanese links to English at the top")
        heads = lambda doc: re.findall(r"^## ", doc, re.M)
        self.assertEqual(len(heads(en)), len(heads(ja)), "the same sections")
        for doc, lang in ((en, "en"), (ja, "ja")):
            image = re.search(r"!\[[^\]]*\]\(([^)]+)\)", doc)
            self.assertTrue(image and (ROOT / image.group(1)).is_file(), f"{lang}: a picture that is there")
            for must in ("21H2", "Windows 11", "x64", "%APPDATA%\\EdgePad", "edgepad.ini", "F13", "F24",
                         "(docs/code-signing-policy.md)", "(LICENSE)", "releases"):
                self.assertIn(must, doc, f"{lang}: {must}")
        self.assertIn("(docs/reference.md)", en)
        self.assertIn("(docs/reference.ja.md)", ja)
        self.assertRegex(en, r"(?i)administrator")
        self.assertIn("管理者", ja)
        self.assertRegex(en, r"(?i)Windows 11 only")
        self.assertIn("Windows 11 だけ", ja)

    # Issue #145, AC1: the download's front page says who signs, in
    # SignPath's own words, beside the policy.
    def test_both_readmes_carry_signpaths_attribution_beside_the_policy(self):
        for lang, path in READMES.items():
            line = next((l for l in text(path).splitlines() if "Free code signing provided by SignPath.io, certificate by SignPath Foundation" in l), None)
            self.assertIsNotNone(line, f"{lang}: the attribution")
            self.assertIn("(docs/code-signing-policy.md)", line, f"{lang}: beside the policy's link")

    # AC2
    def test_the_reference_in_english_and_japanese_covers_every_section(self):
        en, ja = text(REFERENCES["en"]), text(REFERENCES["ja"])
        self.assertIn("(reference.ja.md)", "\n".join(en.splitlines()[:5]))
        self.assertIn("(reference.md)", "\n".join(ja.splitlines()[:5]))
        for doc, lang in ((en, "en"), (ja, "ja")):
            for section in ("[general]", "[edge]", "[edge.left]", "[set.", "[profile."):
                self.assertIn(section, doc, f"{lang}: {section}")
            # Defaults and ranges, by example.
            for must in ("`56`", "24", "200", "`oneshot`", "0〜64" if lang == "ja" else "0 to 64"):
                self.assertIn(must, doc, f"{lang}: {must}")

    # AC3
    def test_every_setting_kind_and_key_name_is_in_both_references(self):
        names = setting_names()
        self.assertTrue({"key_size", "hotkey", "gap", "start", "columns", "rows"} <= names, names)
        named, symbols = key_names()
        self.assertIn("hankaku", named)
        for lang, path in REFERENCES.items():
            spans = code_spans(text(path))
            for name in sorted(names | kinds() | {"width", "interval", "mode", "x", "y", "w", "h"}):
                self.assertIn(name, spans, f"{lang}: setting, kind or option {name}")
            for name in named:
                self.assertIn(name, spans, f"{lang}: key {name}")
            for s in symbols:
                self.assertIn(s, spans, f"{lang}: key {s}")
            for name in ("f1", "f24", "numpad0", "numpad9", "a", "z", "0", "9"):
                self.assertIn(name, spans, f"{lang}: key {name}")


if __name__ == "__main__":
    unittest.main()
