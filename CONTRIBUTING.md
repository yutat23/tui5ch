# 開発・配布

Rustのstableツールチェーンを使います。`Cargo.lock` はバイナリアプリの再現可能なビルドのためGitに含めます。

## ローカル確認

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
cargo run --release --locked -- --demo --data-dir ./data/demo
```

自動テストはローカルデータと模擬HTTPサーバーを使います。本番の掲示板へ書き込むテストはありません。投稿処理を検証するときも、模擬サーバーを使ってください。

`--check` は実際に5chへ読み取りの通信を行います。投稿はしません。

## ディレクトリ

| パス | 内容 |
|---|---|
| `src/app.rs` | 状態、キー・マウス操作、更新処理 |
| `src/ui.rs` | 画面、レイアウト、ヘルプ |
| `src/model.rs` | 板・スレ・レス・設定のモデル |
| `src/net.rs` | 読み取りのHTTP処理 |
| `src/posting.rs` | 投稿フォーム、確認、Cookie処理 |
| `src/parse.rs` | 板一覧・スレ一覧・DATの解析 |
| `src/store.rs` | SQLite保存と復元 |
| `src/main.rs` | CLI引数と起動 |
| `docs/` | 操作・設定とデモ表示 |
| `scripts/` | ローカル配布用スクリプト |
| `.github/workflows/` | CI |

`data/`、`target/`、`dist/`、`.local/` はローカル専用で、Gitの対象外です。旧GUI投稿モジュールのローカル退避先も `.local/` に置きます。現在のビルドに.NETやWebView2は不要です。

## Windows向け配布

リポジトリのルートで実行します。

```powershell
./scripts/package.ps1
```

Releaseビルド後、実行ファイル・README・LICENSE・操作ガイドなどの公開ドキュメントを `dist/` のZIPへまとめます。配布物のファイル一覧に閲覧ログ・下書き・Cookieを含めません。現在はスクリプトを実行したWindows環境のホストアーキテクチャ向けです。

## GitHub Releaseを作る

GitHubにリポジトリを公開して、GitHub Actionsを有効にしてください。追加のPATは不要です。公開ジョブだけに `contents: write` を付けています。

1. `Cargo.toml` のバージョンを更新し、`cargo check --locked` で確認します。バージョンを変更した場合は `cargo check` で `Cargo.lock` も更新してから確認します。
2. CHANGELOGの内容を `## 0.1.0 — 2026-10-06` のようなリリース見出しへ移し、空の `## Unreleased` を残します。
3. 変更をコミットし、そのコミットに注釈付きタグを作成します。

```sh
git tag -a v0.1.0 -m "tui5ch v0.1.0"
git push origin main
git push origin v0.1.0
```

タグのpushで [Releaseワークフロー](.github/workflows/release.yml) が開始します。タグ・Cargoバージョン・日付付きCHANGELOGの一致を確認し、3OSでfmt・テスト・Clippy・ビルドを実行します。成功後にGitHub Releaseを公開し、Windows向けZIP、Linux・macOS向けtar.gz、`SHA256SUMS` を添付します。各ランナーのホストアーキテクチャをファイル名に付けます。全CPUアーキテクチャへのクロスビルドは行いません。`v0.2.0-beta.1` のようなタグはprereleaseになります。

タグを手元で作れない場合は、リリース準備を `chore: release v0.1.0` のような完全一致のコミットメッセージで `main` にpushしても開始できます。バージョン・CHANGELOGの検証と全OSのビルド成功後、CIがそのコミットに注釈付きタグを作成してReleaseを公開します。既存タグが別のコミットを指している場合は停止し、タグの付け替えは行いません。通常のコミットからリリースは作成しません。

リリース説明にはCHANGELOGの該当バージョンを使います。各アーカイブには実行ファイルと明示した公開ドキュメントだけを含めます。LinuxバイナリはUbuntuランナー上でビルドしたglibc依存のバイナリです。Linux・macOSでの実機TUI操作は別途確認してください。

ローカルで配布処理を確認する場合はPython 3.11以上を使用します。

```sh
python -m unittest discover -s scripts -p 'test_*.py'
cargo build --release --locked
python scripts/release.py package
python scripts/release.py checksums --output dist
python scripts/release.py validate --tag v0.1.0 --output .local/release
```

`validate` はリリース見出しを用意した後に成功します。

## 変更を送るとき

変更内容と、実行したチェックをPull Requestに記載してください。不具合報告ではOS・端末・再現手順を添えてください。保存データベースや投稿Cookieを公開Issueへ添付しないでください。

利用者に影響する機能追加・変更・修正は `CHANGELOG.md` の `Unreleased` に記録してください。リリース時は、その内容をバージョンと日付を付けた見出しへ移し、次の変更用に空の `Unreleased` を残します。

CI設定の参照: [actions/checkout](https://github.com/actions/checkout)、[dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain)。
