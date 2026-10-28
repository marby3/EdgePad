"""Checks for Issue #123: the failed tests of a cargo test run, read from
its output, become the runs that try them once more (ci/retry.py).

Run from the repo root: python -m unittest ci/test_retry.py
"""

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import retry  # noqa: E402  (ci/retry.py)

# cargo test --workspace output, cut down: a unit test binary of each kind
# and integration tests of the same name in two packages, a test of each
# failing, with the panic of another passing test's output in between.
OUTPUT = r"""
     Running unittests src\lib.rs (target\debug\deps\edgepad_core-1111.exe)

running 2 tests
test keys::tests::parses ... ok
test layout::tests::fits ... FAILED

failures:

---- layout::tests::fits stdout ----
thread 'layout::tests::fits' panicked at crates\core\src\layout.rs:9:5:
not fitted

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests\config.rs (target\debug\deps\config-2222.exe)

running 1 test
test a_file_reads ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\main.rs (target\debug\deps\edgepad-3333.exe)

running 1 test
test tray::tests::menu ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

     Running tests\config.rs (target\debug\deps\config-4444.exe)

running 2 tests
test the_window_opens ... ok
test saving_checks_with_the_parser ... FAILED

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.10s

     Running tests\keymap.rs (target\debug\deps\keymap-5555.exe)

running 3 tests
thread 'main' panicked at crates\edgepad\src\tray.rs:425:61:
fault injection
test the_sample_is_drawn ... ok
test each_kind_has_its_own_fields ... FAILED
test the_hotkey_is_captured_too ... FAILED

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 99.00s
"""


class RetryTest(unittest.TestCase):
    # AC3
    def test_the_failed_tests_become_one_run_per_test_target(self):
        runs = retry.reruns(OUTPUT)
        self.assertEqual(
            runs,
            [
                (["--lib"], ["layout::tests::fits"]),
                (["--bins"], ["tray::tests::menu"]),
                (["--test", "config"], ["saving_checks_with_the_parser"]),
                (["--test", "keymap"], ["each_kind_has_its_own_fields", "the_hotkey_is_captured_too"]),
            ],
        )
        self.assertEqual(retry.reruns(OUTPUT.replace("FAILED", "ok")), [], "nothing failed: nothing to run again")
        # A test binary that did not build is no test that failed by chance.
        self.assertIsNone(retry.reruns(OUTPUT + "\nerror[E0063]: missing field `gap`\n"))


if __name__ == "__main__":
    unittest.main()
