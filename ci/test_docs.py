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
SPEC = ROOT / "docs" / "spec.md"


def text(path):
    return path.read_text(encoding="utf-8")


def section(doc, head):
    """`doc` from the heading `head` up to the next heading of its level or
    above."""
    at = doc.index(head)
    level = len(head) - len(head.lstrip("#"))
    found = re.compile(rf"^#{{1,{level}}} ", re.M).search(doc, at + len(head))
    return doc[at:found.start() if found else len(doc)]


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
                         "(LICENSE)", "releases"):
                self.assertIn(must, doc, f"{lang}: {must}")
        self.assertIn("(docs/reference.md)", en)
        self.assertIn("(docs/reference.ja.md)", ja)
        self.assertRegex(en, r"(?i)administrator")
        self.assertIn("管理者", ja)
        self.assertRegex(en, r"(?i)Windows 11 only")
        self.assertIn("Windows 11 だけ", ja)

    # Issue #147, AC3: the exe is unsigned, and how to get past SmartScreen.
    def test_both_readmes_say_how_to_open_the_unsigned_exe(self):
        musts = {"en": ("unsigned", "SmartScreen", "Windows protected your PC", "More info", "Run anyway"),
                 "ja": ("署名", "SmartScreen", "Windows によって PC が保護されました", "詳細情報", "実行")}
        heads = {"en": "## Installing", "ja": "## 入れ方"}
        for lang, path in READMES.items():
            doc = text(path)
            at = doc.index(heads[lang])
            section = doc[at:doc.find("\n## ", at + 1)]
            for must in musts[lang]:
                self.assertIn(must, section, f"{lang}: {must}")

    # Issue #149, AC5: monitor numbers are Windows's, and v1.0's may point
    # elsewhere now.
    def test_both_references_number_monitors_as_windows_does(self):
        musts = {"en": ("Windows", "Settings", "Display", "v1.1", "left to right"),
                 "ja": ("Windows", "設定", "ディスプレイ", "v1.1", "左から")}
        for lang, path in REFERENCES.items():
            doc = text(path)
            row = next(l for l in doc.splitlines() if l.startswith("| `monitor`"))
            self.assertIn("Windows", row, f"{lang}: the row itself says whose numbers")
            at = doc.index("| `monitor`")
            near = doc[at:at + 1500]
            for must in musts[lang]:
                self.assertIn(must, near, f"{lang}: {must}")

    # Issue #136, AC1: the words the screens use, first in the Japanese
    # reference, and the spec pointing there.
    def test_the_japanese_reference_opens_with_the_glossary(self):
        doc = text(REFERENCES["ja"])
        heads = re.findall(r"^## (.+)$", doc, re.M)
        self.assertEqual(heads[0], "用語", heads[:3])
        glossary = section(doc, "## 用語")
        for word in ("エッジ", "左エッジ", "パネル", "ハンドル", "キーセット", "エッジに沿って並べる", "自由配列",
                     "キー配置", "キーマッピング", "プロファイル", "修飾キー", "ラッチ", "レイヤー", "ページ",
                     "パイメニュー", "スワイプ", "フリック"):
            self.assertRegex(glossary, rf"(?m)^\| [^|]*{word}", word)
        for name in ("`[edge]`", "`[set.", "`[profile.", "`handle`", "`layout`"):
            self.assertIn(name, glossary, name)
        self.assertIn("辺 → エッジ", glossary, "old to new")
        self.assertIn("フリック → スワイプ", glossary, "old to new")
        self.assertIn("(reference.ja.md#用語)", text(SPEC))

    # Issue #136, AC2: no 辺 in what the exe puts on the screen, the tray,
    # notifications or errors: its string literals.
    def test_no_string_the_exe_shows_says_hen(self):
        literal = re.compile(r'"(?:[^"\\]|\\.)*"')
        for path in sorted((ROOT / "crates").glob("*/src/**/*.rs")):
            for n, line in enumerate(text(path).splitlines(), 1):
                if line.lstrip().startswith("//"):
                    continue
                for s in literal.findall(line):
                    self.assertNotIn("辺", s, f"{path.relative_to(ROOT)}:{n}: {s}")

    # Issue #136, AC3: hiding a panel is a swipe; a flick is a key's.
    def test_the_spec_calls_hiding_a_panel_a_swipe(self):
        spec = text(SPEC)
        for head in ("### 4.5", "### 4.6", "### 5.1"):
            self.assertNotIn("フリック", section(spec, head), head)
        self.assertIn("スワイプ", section(spec, "### 4.5"))

    # Issue #136, AC5: the documents and the shipped settings say エッジ.
    def test_the_documents_and_shipped_settings_say_edge_not_hen(self):
        for path in (READMES["ja"], READMES["en"], SPEC, ROOT / "presets" / "edgepad.ini"):
            for n, line in enumerate(text(path).splitlines(), 1):
                self.assertNotIn("辺", line, f"{path.relative_to(ROOT)}:{n}")
        ja = text(REFERENCES["ja"])
        rest = ja.replace(section(ja, "## 用語"), "")
        self.assertNotIn("辺", rest, "the reference past its glossary")
        self.assertIn("エッジ（共通）", text(READMES["en"]), "the English README's Japanese page names")

    # Issue #139, AC6: the shipped settings leave room with space, and the
    # spec and both references say how.
    def test_space_is_shipped_and_written_up(self):
        preset = text(ROOT / "presets" / "edgepad.ini")
        csp = section(preset.replace("[set.", "## [set."), "## [set.csp]")
        self.assertRegex(csp, r"(?m)^space\s*=\s*1\b", "the room after Esc")
        self.assertNotIn("| none", csp, "no unseen none key for room")
        self.assertNotIn("​", csp, "no unseen label")
        for lang, path in REFERENCES.items():
            self.assertIn("`space`", text(path), lang)
            self.assertIn("0.25", text(path), lang)
        spec = text(SPEC)
        self.assertIn("space =", section(spec, "## 7."), "chapter 7")
        self.assertIn("空き", section(spec, "### 7.1"), "7.1")

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
