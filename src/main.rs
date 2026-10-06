mod app;
mod model;
mod net;
mod parse;
mod posting;
mod store;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "5ch・エッヂ・したらばを読むTUI専用ブラウザ")]
struct Args {
    /// 起動時に開く板・スレURL
    url: Option<String>,
    /// 通信を使わず操作デモを表示（データは専用ディレクトリに分離）
    #[arg(long)]
    demo: bool,
    /// ローカル保存済みログのみ使用
    #[arg(long)]
    offline: bool,
    /// データ保存先を指定
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// TUIを起動せず通信とパーサーを確認
    #[arg(long)]
    check: bool,
    /// デモ画面をテキストファイルに書き出す
    #[arg(long)]
    snapshot: Option<PathBuf>,
}
fn main() -> Result<()> {
    let args = Args::parse();
    if args.check {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        let input = args
            .url
            .as_deref()
            .unwrap_or("https://agree.5ch.io/operate/");
        match model::parse_url(input)? {
            model::Target::Board(mut b) => {
                let threads = net::fetch_subjects(&client, &mut b)?;
                println!("OK: {} / {}スレ", b.url, threads.len());
                for t in threads.iter().take(5) {
                    println!("{} ({})", t.title, t.count);
                }
            }
            model::Target::Thread(t) => {
                let data = net::fetch_thread(&client, &t)?;
                println!("OK: {} / {}レス", data.thread.title, data.posts.len());
            }
        }
        return Ok(());
    }
    let dir = args.data_dir.unwrap_or_else(|| {
        let base = directories::ProjectDirs::from("", "", "tui5ch")
            .expect("データディレクトリを取得できません")
            .data_local_dir()
            .to_path_buf();
        if args.demo { base.join("demo") } else { base }
    });
    let store = store::Store::open(&dir).context("保存先を開けませんでした")?;
    let mut app = app::App::new(store, args.demo, args.offline)?;
    if let Some(url) = args.url {
        app.open_input(&url)?;
    }
    if let Some(path) = args.snapshot {
        return ui::snapshot(&mut app, &path);
    }
    app::run(&mut app)
}
