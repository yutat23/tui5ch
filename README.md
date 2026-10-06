# tui5ch

キーボードとマウスで操作できる、Rust製の5ch・エッヂ・したらば向けTUI専用ブラウザです。板・スレ・レスの3ペイン、スレとレスの上下表示、狭い端末向けの1ペインを切り替えられます。

Windows Terminalを中心に動作確認しています。Linux・macOSの実機での動作確認はまだ行っていません。

## 起動

[GitHub Releases](https://github.com/yutat23/tui5ch/releases)の配布物では、OS・アーキテクチャに合うZIPまたはtar.gzを展開して `tui5ch`（Windowsは `tui5ch.exe`）を起動します。アーカイブ名に対象環境を含め、`SHA256SUMS` も添付します。

Rustのstableツールチェーンを用意して、このリポジトリのルートで実行します。WindowsではMSVC対応のRustビルド環境が必要です。

```sh
cargo run --release --locked
```

まず通信なしで試す場合:

```sh
cargo run --release --locked -- --demo
```

ビルドした実行ファイルを使う場合:

```sh
cargo build --release --locked
```

Windowsでは `target/release/tui5ch.exe`、Linux・macOSでは `target/release/tui5ch` を起動します。Windowsでは `./start.ps1` も使えます。`dist/` の配布物はソースリポジトリに含めません。配布ZIPを使う場合は展開して `tui5ch.exe` を起動します。

初回は板一覧を取得します。カテゴリを開いて板を選び、スレを開きます。クリックと `Enter` の両方に対応します。画面下の `[キー 操作]` もクリックして実行できます。`?` または `F1` でキーを強調表示したヘルプを開けます。

推奨サイズは120列×36行、最低サイズは30列×10行です。デモの保存先は通常データと分離されます。

## 機能

- カテゴリ式の板一覧、検索、お気に入りフォルダと並び順の保存
- スレ検索、レス数・未読・勢い・作成日時でのソート、次スレ候補
- クリックで切替・閉じる操作ができるスレタブ、選択タブと閲覧位置の復元、未読境界、参照・同じID・被返信レスの表示
- 自動更新、新着追従、最新への移動、ドラッグできるレスのスクロールバー
- NG、正規表現、期限、連鎖NG、保存済みログの全文検索
- SQLiteへの保存、オフライン閲覧、TXT・JSONエクスポート
- 5chへのTUI内での返信・引用・スレ立て、名前・メール・下書きの保存
- 暗色・明色テーマ、AAの折り返し、キー割り当て、ペインのサイズ変更

[操作ガイド](docs/usage.md) / [保存先と設定](docs/configuration.md) / [変更履歴](CHANGELOG.md) / [デモ画面のテキスト](docs/demo-screen.txt)

## 対応サイト

| サイト | 閲覧 | 返信・スレ立て |
|---|---|---|
| 5ch.io（旧5ch.net URLも受付） | 対応 | HTTPフォーム方式。本番成功は未検証 |
| エッヂ（bbs.eddibb.cc） | 専ブラ向けsubject.txt・DATに対応 | 未対応 |
| したらば（jbbs.shitaraba.net） | subject.txt・rawmode・EUC-JPに対応 | 未対応 |

エッヂは板一覧の「エッヂ」カテゴリに追加されます。したらばは `u` で板・スレURLを入力して追加してください。板URLは `https://jbbs.shitaraba.net/カテゴリ/板番号/` 形式です。追加した板は保存され、共通のお気に入り・検索・NG・自動更新・ログ保存を使えます。旧したらばURL（jbbs.livedoor.jp/com）は現在のドメインに変換します。

エッヂ・したらばの実サーバーでのデータ取得は、この開発環境では未検証です。通信経路・文字コード・レス解析はローカルの模擬サーバーで検証しています。

## よく使う操作

| 操作 | キー・マウス |
|---|---|
| ペイン選択 | `Tab`、`1` 板 / `2` スレ / `3` レス |
| スレタブの切り替え・閉じる | タイトルをクリック、`×`・中クリックで閉じる。`[` 前 / `]` 次 / `x` 閉じる |
| 検索 | `/`、`Esc` で解除 |
| 手動更新 | `r`、各ペイン右上の「更新」 |
| 自動更新＋新着追従 | `A`、レス欄の「A 自動」 |
| 最新レスの末尾へ | `End` / `G`、「最新へ」 |
| お気に入り | `f`、板の右クリック |
| 返信・スレ立て | `w` / `R`、`B` |
| ヘルプ・終了 | `?` / `F1`、`q` |

## コマンドライン

```sh
# 起動時に板・スレを開く
cargo run --release --locked -- 'https://egg.5ch.io/software/'

# 保存済みログのみ閲覧
cargo run --release --locked -- --offline

# 保存先を指定
cargo run --release --locked -- --data-dir ./data

# デモ画面のテキストを書き出す
cargo run --release --locked -- --demo --data-dir ./data/demo --snapshot ./data/demo-screen.txt
```

`--check URL` はTUIを開かず、板のスレ一覧またはスレのレスデータを取得・解析します。旧 `5ch.net` URLは `5ch.io` に変換します。

## 現在の制限

- 5ch本番での返信・スレ立て成功は未検証です。自動テストはローカルの模擬サーバーを使います。
- 投稿はHTTPフォームと確認フォームに対応します。JavaScript、画像認証、ブラウザによるログインは未対応です。要求された場合は完了できません。
- 投稿結果が不明な場合に自動再送しません。スレを更新して投稿済みか確認してください。
- DATは全体取得です。差分取得と別の過去ログサーバーの探索は未対応です。
- 画像は端末内に表示しません。リンクを既定ブラウザで開きます。

## 開発

```sh
cargo fmt --all -- --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

[開発・配布手順](CONTRIBUTING.md)を参照してください。GitHub ActionsにはWindows・Linuxの通常チェックと、Windows・Linux・macOSのタグリリースを設定しています。

## ライセンス

[MIT](LICENSE)
