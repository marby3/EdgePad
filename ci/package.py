"""The zip a release publishes (Issue #79): what goes in it is listed here,
and nowhere else.

    python ci/package.py --check v1.0.0
    python ci/package.py --tag v1.0.0 --exe target/release/edgepad.exe --out dist

--check only compares the tag with the version in Cargo.toml. With --tag,
the zip EdgePad-<tag>-windows-x64.zip is written to --out, the exe given
(signed or not) and the files below in it, side by side. A tag that is not
the version in Cargo.toml writes nothing and fails.
"""

import argparse
import pathlib
import sys
import tomllib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Beside the exe in the zip: (path in the repo, name in the zip). The
# settings shipped beside the exe are the ones it reads (Issue #127).
FILES = [
    ("LICENSE", "LICENSE"),
    ("README.md", "README.md"),
    ("README.ja.md", "README.ja.md"),
    ("presets/edgepad.ini", "edgepad.ini"),
    ("presets/kb-us.json", "kb-us.json"),
    ("presets/kb-split42.json", "kb-split42.json"),
    ("presets/kb-corne-v4.json", "kb-corne-v4.json"),
    ("presets/kb-alice64.json", "kb-alice64.json"),
]


def version():
    with open(ROOT / "Cargo.toml", "rb") as f:
        return tomllib.load(f)["workspace"]["package"]["version"]


def check(tag):
    """Fail unless `tag` is v and the version in Cargo.toml."""
    if tag != f"v{version()}":
        sys.exit(f"tag {tag} is not the version in Cargo.toml ({version()}): nothing published")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", metavar="TAG", help="only compare TAG with Cargo.toml's version")
    parser.add_argument("--tag", help="the release's tag, vX.Y.Z")
    parser.add_argument("--exe", type=pathlib.Path, help="the exe to put in")
    parser.add_argument("--out", type=pathlib.Path, help="folder to write the zip to")
    args = parser.parse_args()
    if args.check:
        return check(args.check)
    if not (args.tag and args.exe and args.out):
        parser.error("--tag, --exe and --out go together")
    check(args.tag)
    args.out.mkdir(parents=True, exist_ok=True)
    zipped = args.out / f"EdgePad-{args.tag}-windows-x64.zip"
    with zipfile.ZipFile(zipped, "w", zipfile.ZIP_DEFLATED) as z:
        z.write(args.exe, "edgepad.exe")
        for source, name in FILES:
            z.write(ROOT / source, name)
    print(zipped)


if __name__ == "__main__":
    main()
