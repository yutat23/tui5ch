use crate::{
    model::{Board, Site, Thread, ThreadData},
    parse,
};
use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use std::{
    io::Read,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

pub enum Request {
    Menu(String),
    Subjects(Board),
    Thread(Thread),
}
pub enum Response {
    Menu(Result<Vec<Board>, String>),
    Subjects(Board, Result<Vec<Thread>, String>),
    Thread(Thread, Result<ThreadData, String>),
}
pub struct Network {
    pub tx: Sender<Request>,
    pub rx: Receiver<Response>,
}
impl Network {
    pub fn start() -> Self {
        let (tx, work) = mpsc::channel();
        let (result, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let client = Client::builder()
                .user_agent("tui5ch/0.1 (+terminal reader)")
                .connect_timeout(Duration::from_secs(8))
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build();
            for request in work {
                let client = client.as_ref().map_err(|e| e.to_string());
                let response = match request {
                    Request::Menu(url) => Response::Menu(client.and_then(|c| {
                        fetch(c, &url)
                            .and_then(|s| parse::menu(&s))
                            .map_err(|e| e.to_string())
                    })),
                    Request::Subjects(mut board) => {
                        let data = client
                            .and_then(|c| fetch_subjects(c, &mut board).map_err(|e| e.to_string()));
                        Response::Subjects(board, data)
                    }
                    Request::Thread(thread) => {
                        let data = client
                            .and_then(|c| fetch_thread(c, &thread).map_err(|e| e.to_string()));
                        Response::Thread(thread, data)
                    }
                };
                if result.send(response).is_err() {
                    break;
                }
            }
        });
        Self { tx, rx }
    }
}
pub fn fetch_subjects(client: &Client, board: &mut Board) -> Result<Vec<Thread>> {
    if board.site() == Site::Shitaraba
        && board.name == format!("{}/{}", board.directory().unwrap_or(""), board.slug())
    {
        let url = format!(
            "https://jbbs.shitaraba.net/bbs/api/setting.cgi/{}/{}/",
            board.directory().unwrap_or(""),
            board.slug()
        );
        if let Ok(settings) = fetch(client, &url)
            && let Some(title) = settings
                .lines()
                .find_map(|line| line.strip_prefix("BBS_TITLE="))
        {
            let title = parse::html_text(title);
            if !title.trim().is_empty() {
                board.name = title.chars().take(120).collect();
            }
        }
    }
    parse::subjects(&fetch(client, &board.subjects_url())?, board)
}
pub fn fetch(client: &Client, url: &str) -> Result<String> {
    let euc = url::Url::parse(url)
        .ok()
        .is_some_and(|u| u.host_str() == Some("jbbs.shitaraba.net"));
    fetch_encoded(
        client,
        url,
        if euc {
            encoding_rs::EUC_JP
        } else {
            encoding_rs::SHIFT_JIS
        },
    )
}
fn fetch_encoded(
    client: &Client,
    url: &str,
    fallback: &'static encoding_rs::Encoding,
) -> Result<String> {
    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("接続できません: {url}"))?
        .error_for_status()?;
    if let Some(error) = response.headers().get("ERROR") {
        bail!("掲示板の取得エラー: {}", error.to_str().unwrap_or("不明"));
    }
    let encoding = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| {
            h.split(';').find_map(|part| {
                part.trim()
                    .split_once('=')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("charset"))
                    .and_then(|(_, v)| {
                        encoding_rs::Encoding::for_label(
                            v.trim().trim_matches(['\"', '\'']).as_bytes(),
                        )
                    })
            })
        })
        .unwrap_or(fallback);
    let max = 16 * 1024 * 1024;
    let mut bytes = Vec::new();
    Read::by_ref(&mut response)
        .take(max + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("応答が16MBを超えたため停止しました");
    }
    Ok(parse::decode_with(&bytes, encoding))
}
pub fn fetch_thread(client: &Client, thread: &Thread) -> Result<ThreadData> {
    let url = thread.data_url();
    if thread.board.site() == Site::Shitaraba {
        return parse::shitaraba_raw(&fetch(client, &url)?, thread.clone());
    }
    match fetch(client, &url) {
        Ok(s) => parse::dat(&s, thread.clone()),
        Err(e) => {
            if e.downcast_ref::<reqwest::Error>()
                .and_then(|e| e.status())
                .is_some_and(|s| s == 404 || s == 410)
                && thread.board.site() == Site::FiveCh
            {
                let url = format!(
                    "{}oyster/{}/{}.dat",
                    thread.board.url,
                    &thread.key[..4],
                    thread.key
                );
                parse::dat(&fetch(client, &url)?, thread.clone())
            } else {
                Err(e)
            }
        }
    }
}

pub fn open_url(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url)?;
    if !matches!(parsed.scheme(), "https" | "http") {
        bail!("http/httpsリンクのみ開けます");
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_board_and_thread_requests() -> Result<()> {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let server = std::thread::spawn(move || {
            for (path, payload, encoding) in [
                (
                    "/liveedge/subject.txt",
                    "1791209783.dat<>エッヂのスレ (1)\n",
                    encoding_rs::UTF_8,
                ),
                (
                    "/liveedge/dat/1791209783.dat",
                    "名無し<>sage<>date ID:edge<>本文<>エッヂのスレ\n",
                    encoding_rs::UTF_8,
                ),
                (
                    "/game/12345/subject.txt",
                    "1791209783.cgi,したらばのスレ (1)\n",
                    encoding_rs::EUC_JP,
                ),
                (
                    "/bbs/rawmode.cgi/game/12345/1791209783/",
                    "1<>名無し<>sage<>date<>本文<>したらばのスレ<>shita\n",
                    encoding_rs::EUC_JP,
                ),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0; 4096];
                let count = stream.read(&mut buf).unwrap();
                assert!(
                    String::from_utf8_lossy(&buf[..count])
                        .starts_with(&format!("GET {path} HTTP/1.1"))
                );
                let payload = encoding.encode(payload).0.into_owned();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                )
                .unwrap();
                stream.write_all(&payload).unwrap();
            }
        });
        let client = Client::builder()
            .no_proxy()
            .resolve("bbs.eddibb.cc", addr)
            .resolve("jbbs.shitaraba.net", addr)
            .timeout(Duration::from_secs(3))
            .build()?;
        for (host, path, title, id) in [
            ("bbs.eddibb.cc", "liveedge", "エッヂのスレ", "edge"),
            (
                "jbbs.shitaraba.net",
                "game/12345",
                "したらばのスレ",
                "shita",
            ),
        ] {
            let mut board = Board {
                name: "登録済みの板".into(),
                category: "外部板".into(),
                url: format!("http://{host}:{}/{path}/", addr.port()),
            };
            let threads = fetch_subjects(&client, &mut board)?;
            assert_eq!(threads.len(), 1);
            assert_eq!(threads[0].title, title);
            let data = fetch_thread(&client, &threads[0])?;
            assert_eq!(data.posts.len(), 1);
            assert_eq!(data.posts[0].body, "本文");
            assert_eq!(data.posts[0].id, id);
        }
        server.join().unwrap();
        Ok(())
    }
    #[test]
    fn euc_jp_http_and_error_header_preserve_data() -> Result<()> {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let root = format!("http://{}", listener.local_addr()?);
        let server = std::thread::spawn(move || {
            for error in [false, true] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0; 4096];
                assert!(stream.read(&mut buf).unwrap() > 0);
                let payload = encoding_rs::EUC_JP
                    .encode("1<>名無し<>sage<>date<>本文<>題名<>abc\n")
                    .0
                    .into_owned();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
                    payload.len(),
                    if error {
                        "ERROR: STORAGE IN\r\n"
                    } else {
                        "Content-Type: text/plain; charset=EUC-JP\r\n"
                    }
                )
                .unwrap();
                stream.write_all(&payload).unwrap();
            }
        });
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()?;
        let raw = fetch_encoded(&client, &root, encoding_rs::SHIFT_JIS)?;
        assert!(raw.contains("本文"));
        assert!(
            fetch_encoded(&client, &root, encoding_rs::EUC_JP)
                .unwrap_err()
                .to_string()
                .contains("STORAGE IN")
        );
        server.join().unwrap();
        Ok(())
    }
    #[test]
    fn http_fetch_and_archive_fallback() -> Result<()> {
        use std::{io::Write, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let server = std::thread::spawn(move || {
            for archived in [false, true] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut req = [0u8; 4096];
                let n = stream.read(&mut req).unwrap();
                let request = String::from_utf8_lossy(&req[..n]);
                assert!(request.contains(if archived {
                    "/operate/oyster/1234/1234567890.dat"
                } else {
                    "/operate/dat/1234567890.dat"
                }));
                let payload = if archived {
                    encoding_rs::SHIFT_JIS
                        .encode("名無しさん<>sage<>2026 ID:abc<>過去ログの本文<>過去ログの題名\n")
                        .0
                        .into_owned()
                } else {
                    b"not found".to_vec()
                };
                let status = if archived { "200 OK" } else { "404 Not Found" };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                )
                .unwrap();
                stream.write_all(&payload).unwrap();
            }
        });
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()?;
        let thread = Thread {
            key: "1234567890".into(),
            title: String::new(),
            count: 0,
            board: Board {
                name: "test".into(),
                category: String::new(),
                url: format!("http://{addr}/operate/"),
            },
        };
        let data = fetch_thread(&client, &thread)?;
        assert_eq!(data.thread.title, "過去ログの題名");
        assert_eq!(data.posts[0].body, "過去ログの本文");
        server.join().unwrap();
        Ok(())
    }
}
