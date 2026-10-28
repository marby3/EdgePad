"""Run the workspace's tests, and the ones that fail once more (Issue #123).

The settings window's tests drive a real desktop. On a GitHub runner, with
no one at it, a window now and then takes longer to come to the front or to
draw than any wait in a test allows, and a test fails by chance. This runs
`cargo test --workspace --all-features --no-fail-fast`, and then each test
that failed once more, by itself. A test that passes the second time is
named in a warning on the run; a test that fails twice fails it, as before.
A test binary that does not build is no chance: nothing runs again.

Writes the output of the run that decides to test-output.txt, for the step
that reports the failures.

    python ci/retry.py
"""

import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CARGO = ["cargo", "test", "--workspace", "--all-features", "--no-fail-fast"]
OUTPUT = ROOT / "test-output.txt"


def reruns(output):
    """The runs that try the failed tests of `output`, a cargo test run's,
    once more: for each test target with a failure, in the order run, its
    cargo arguments and the failed tests' names. None if a test did not
    build."""
    if "could not compile" in output or re.search(r"^error\[E\d+\]", output, re.M):
        return None
    runs = []
    target = None
    for line in output.splitlines():
        running = re.match(r"\s*Running (?:unittests )?(\S+)", line)
        if running:
            path = running.group(1).replace("\\", "/")
            if path.endswith("src/lib.rs"):
                target = ["--lib"]
            elif path.startswith("src/"):
                target = ["--bins"]
            else:
                target = ["--test", pathlib.PurePosixPath(path).stem]
            continue
        failed = re.match(r"test (\S+) \.\.\. FAILED$", line.strip())
        if failed and target:
            for args, names in runs:
                if args == target:
                    if failed.group(1) not in names:
                        names.append(failed.group(1))
                    break
            else:
                runs.append((target, [failed.group(1)]))
    return runs


def run(args):
    """Run cargo test with `args`, echoing its output as it comes; its
    output and whether it passed. cargo says which binary runs on stderr
    and the tests write theirs to stdout: one stream keeps them in order."""
    lines = []
    with subprocess.Popen(
        CARGO + args, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace"
    ) as cargo:
        for line in cargo.stdout:
            sys.stdout.write(line)
            sys.stdout.flush()
            lines.append(line)
    return "".join(lines), cargo.returncode == 0


def main():
    # The tests write Japanese; the runner's console is not UTF-8.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    output, passed = run([])
    OUTPUT.write_text(output, encoding="utf-8")
    if passed:
        return 0
    runs = reruns(output)
    if not runs:
        return 1
    again = []
    failed_twice = False
    for args, names in runs:
        print(f"\n==> once more: {' '.join(args)} -- {' '.join(names)}", flush=True)
        out, ok = run(args + ["--", "--exact", *names])
        again.append(out)
        for name in names:
            if re.search(rf"^test {re.escape(name)} \.\.\. ok$", out, re.M):
                print(f"::warning::{name} は 1 回目に落ち、走らせ直して通った（{' '.join(args)}）")
            else:
                failed_twice = True
        failed_twice |= not ok
    # The runs that decided, for the report of what failed.
    OUTPUT.write_text("\n".join(again), encoding="utf-8")
    return 1 if failed_twice else 0


if __name__ == "__main__":
    sys.exit(main())
