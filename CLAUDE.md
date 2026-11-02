# EdgePad

## テストの回し方

`crates/edgepad/tests/` のテストは、ほとんどが `edgepad.exe` を起動します。そのうえで本物のマウスとキーを動かし、窓を前面に出します。回っている間は、PC の前の人がマウスもキーボードも使えません。そこでこれらのテストは feature `desktop-tests` で囲ってあり、CI でだけ回します（Issue #153）。

### 手元で回すもの

```
cargo test --workspace
python -m unittest ci/test_build.py ci/test_measure.py ci/test_retry.py ci/test_release.py ci/test_docs.py ci/test_desktop.py
```

- `cargo test --workspace` で回るのは、`crates/core` のテスト、`edgepad` の単体テスト、`tests/version.rs` です。
- `--all-features` や `--features desktop-tests` を付けて手元で回してはいけません。画面を操作するテストが始まり、PC が使えなくなります。
- 画面を操作するテストがコンパイルできるかだけを確かめたいときは、`cargo test --workspace --all-features --no-run` を使います。ビルドだけで、実行はしません。

### 画面を操作するテストを CI で確かめる

作業ブランチを push してから、CI を手で起動します。

```
git push -u origin <branch>
gh workflow run ci.yml --ref <branch>
gh run list --workflow ci.yml --branch <branch> --limit 1      # run の ID を見る
gh run watch <id> --exit-status
gh run view <id> --log-failed                                  # 失敗したときの出力
```

RED の確認も同じ手順で行います。

1. テストだけのコミットを push して CI を起動し、新しいテストが期待どおりの理由で失敗するのを `--log-failed` で確かめる。`ci/retry.py` が失敗したテストをもう一度回すので、RED のテストは 2 回失敗する。
2. 実装を push して CI をもう一度起動し、GREEN を確かめる。

PR を作った後は、push するたびに `pull_request` で CI が動くので、手で起動する必要はありません。

### 画面を操作するテストを新しく足すとき

`crates/edgepad/Cargo.toml` の末尾に、そのファイルの `[[test]]` を `required-features = ["desktop-tests"]` 付きで足します。足し忘れると、`ci/test_desktop.py` が失敗して知らせます。
