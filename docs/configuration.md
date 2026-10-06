# 保存先と設定

Windowsの通常保存先は `%LOCALAPPDATA%/tui5ch/data` です。`--data-dir ./data` で指定できます。画面設定は `config.toml`、ログ・既読・NG・下書きなどは `tui5ch.sqlite3` に保存します。

```toml
menu_url = "https://menu.5ch.io/bbsmenu.html"
refresh_seconds = 60
theme = "dark"
wrap = true
ng_chain = true
layout = "auto" # auto / three / two / single
board_width = 27
thread_width = 36
thread_height = 10
header_height = 3

[keys]
# 文字キーを別の標準文字キーへ割り当てます（入力欄では適用しません）。
# J = "j"
# K = "k"
```

自動更新は最短10秒です。通信は別スレッドで最大3件並行して行い、接続8秒・全体20秒でタイムアウトします。応答の上限は16MBです。HTTPエラーやレス解析失敗時は既存ログを保持します。5chの現役サーバで404/410になったスレは `oyster` の取得も試します。エッヂはDAT、したらばはrawmodeで取得し、したらばの `ERROR` ヘッダーも取得失敗として扱います。別の過去ログサーバへの自動探索は未対応です。取得は全体取得で、Rangeによる差分取得は未対応です。

端末内の画像表示は未対応です。画像リンクは `o` から既定ブラウザで表示できます。

