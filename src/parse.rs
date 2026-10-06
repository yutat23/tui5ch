use crate::model::{Board, Post, Target, Thread, ThreadData, parse_url};
use anyhow::{Result, bail};
use encoding_rs::{Encoding, SHIFT_JIS};
use regex::Regex;
use std::sync::OnceLock;

pub fn decode(bytes: &[u8]) -> String {
    decode_with(bytes, SHIFT_JIS)
}
pub fn decode_with(bytes: &[u8], encoding: &'static Encoding) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim_start_matches('\u{feff}').into(),
        Err(_) => encoding.decode(bytes).0.into_owned(),
    }
}
pub fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect::<String>()
        .replace('\t', "    ")
}
pub fn html_text(s: &str) -> String {
    static BR: OnceLock<Regex> = OnceLock::new();
    static TAGS: OnceLock<Regex> = OnceLock::new();
    static ENTITIES: OnceLock<Regex> = OnceLock::new();
    let br = BR.get_or_init(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
    let tags = TAGS.get_or_init(|| Regex::new(r"(?s)<[^>]*>").unwrap());
    let entities =
        ENTITIES.get_or_init(|| Regex::new(r"&(#x[0-9a-fA-F]+|#\d+|[A-Za-z]+);").unwrap());
    let s = br.replace_all(s, "\n");
    let s = tags.replace_all(&s, "");
    clean(&entities.replace_all(&s, |c: &regex::Captures| {
        match &c[1] {
            "amp" => "&".into(),
            "lt" => "<".into(),
            "gt" => ">".into(),
            "quot" => "\"".into(),
            "apos" => "'".into(),
            "nbsp" => " ".into(),
            n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default(),
            n if n.starts_with('#') => n[1..]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_default(),
            _ => c[0].into(),
        }
    }))
}
pub fn menu(s: &str) -> Result<Vec<Board>> {
    let re = Regex::new(
        r#"(?is)<b\b[^>]*>(.*?)</b>|<a\b[^>]*href\s*=\s*["']?(https?://[^\s"'>]+)["']?[^>]*>(.*?)</a>"#,
    )?;
    let mut category = "掲示板".to_string();
    let mut boards = Vec::new();
    for c in re.captures_iter(s) {
        if let Some(label) = c.get(1) {
            category = html_text(label.as_str());
        } else if let Ok(Target::Board(mut b)) = parse_url(&c[2]) {
            if let Some(old) = boards.iter_mut().find(|old: &&mut Board| old.url == b.url) {
                // The official menu repeats promoted boards before their normal category.
                if matches!(old.category.as_str(), "おすすめ" | "特別企画")
                    && !matches!(category.as_str(), "おすすめ" | "特別企画")
                {
                    old.category = category.clone();
                    old.name = html_text(&c[3]);
                }
                continue;
            }
            b.name = html_text(&c[3]);
            b.category = category.clone();
            boards.push(b);
        }
    }
    if boards.is_empty() {
        bail!("板一覧を解析できませんでした（HTML応答や規制の可能性）");
    }
    Ok(boards)
}
pub fn subjects(s: &str, board: &Board) -> Result<Vec<Thread>> {
    let re = Regex::new(r"^(\d+)(?:\.dat<>|\.cgi,\s*)(.*)\s+\((\d+)\)$")?;
    let result: Vec<_> = s
        .lines()
        .filter_map(|line| re.captures(line.trim()))
        .filter(|c| c[1].len() >= 4)
        .map(|c| Thread {
            key: c[1].into(),
            title: html_text(&c[2]),
            count: c[3].parse().unwrap_or(0),
            board: board.clone(),
        })
        .collect();
    if result.is_empty() {
        bail!("スレ一覧を解析できませんでした（規制やサーバ障害の可能性）");
    }
    Ok(result)
}
pub fn references(body: &str) -> Vec<usize> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r">>(\d+)(?:[-－](\d+))?").unwrap());
    let mut refs = Vec::new();
    for c in re.captures_iter(body) {
        let start = c[1].parse::<usize>().unwrap_or(0);
        let end = c
            .get(2)
            .and_then(|s| s.as_str().parse().ok())
            .unwrap_or(start)
            .min(start.saturating_add(100));
        refs.extend(start..=end);
    }
    refs.sort_unstable();
    refs.dedup();
    refs
}
pub fn dat(s: &str, mut thread: Thread) -> Result<ThreadData> {
    let id = Regex::new(r"(?:^|\s)ID:([^\s]+)")?;
    let mut posts = Vec::new();
    for (i, line) in s.lines().enumerate() {
        let fields: Vec<_> = line.splitn(5, "<>").collect();
        if fields.len() != 5 {
            bail!("DATの{}行目が不正です。ログは更新していません", i + 1);
        }
        if i == 0 && !fields[4].is_empty() {
            thread.title = html_text(fields[4]);
        }
        let date = html_text(fields[2]);
        let body = html_text(fields[3]).trim_matches(' ').to_string();
        posts.push(Post {
            number: i + 1,
            name: html_text(fields[0]),
            mail: html_text(fields[1]),
            id: id.captures(&date).map(|c| c[1].into()).unwrap_or_default(),
            date,
            refs: references(&body),
            body,
        });
    }
    if posts.is_empty() {
        bail!("レスが取得できませんでした");
    }
    thread.count = posts.len();
    Ok(ThreadData { thread, posts })
}
pub fn shitaraba_raw(s: &str, mut thread: Thread) -> Result<ThreadData> {
    let mut posts = Vec::new();
    for line in s.lines() {
        let f: Vec<_> = line.split("<>").collect();
        if f.len() != 7 {
            bail!("したらばのレス形式が不正です。ログは更新していません");
        }
        let number = f[0].parse::<usize>()?;
        if number <= posts.len() || number > 100_000 {
            bail!("したらばのレス番号が不正です");
        }
        while posts.len() + 1 < number {
            posts.push(Post {
                number: posts.len() + 1,
                name: "あぼーん".into(),
                mail: String::new(),
                date: String::new(),
                id: String::new(),
                body: "削除されたレス".into(),
                refs: vec![],
            });
        }
        if !f[5].is_empty() {
            thread.title = html_text(f[5]);
        }
        let id = html_text(f[6]).trim_start_matches("ID:").to_owned();
        let body = html_text(f[4]).trim_matches(' ').to_owned();
        posts.push(Post {
            number,
            name: html_text(f[1]),
            mail: html_text(f[2]),
            date: format!(
                "{}{}",
                html_text(f[3]),
                if id.is_empty() {
                    String::new()
                } else {
                    format!(" ID:{id}")
                }
            ),
            id,
            refs: references(&body),
            body,
        });
    }
    if posts.is_empty() {
        bail!("レスが取得できませんでした");
    }
    thread.count = posts.len();
    Ok(ThreadData { thread, posts })
}
pub fn links(s: &str) -> Vec<String> {
    Regex::new(r#"https?://[^\s<>"']+"#)
        .unwrap()
        .find_iter(s)
        .map(|m| {
            m.as_str()
                .trim_end_matches([')', '）', '。', ','])
                .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_urls_encodings_subjects_and_raw_posts() -> Result<()> {
        use crate::model::{Site, Target};
        let Target::Board(edge) = parse_url("https://bbs.eddibb.cc/liveedge/")? else {
            panic!()
        };
        assert_eq!(edge.site(), Site::Edge);
        assert_eq!(edge.name, "エッヂ");
        for url in [
            "https://bbs.eddibb.cc/liveedge/1791209783",
            "https://bbs.eddibb.cc/test/read.cgi/liveedge/1791209783/l50",
            "https://bbs.eddibb.cc/liveedge/dat/1791209783.dat",
        ] {
            let Target::Thread(t) = parse_url(url)? else {
                panic!()
            };
            assert_eq!(t.url(), "https://bbs.eddibb.cc/liveedge/1791209783/");
            assert_eq!(
                t.data_url(),
                "https://bbs.eddibb.cc/liveedge/dat/1791209783.dat"
            );
        }
        let Target::Thread(thread) =
            parse_url("http://jbbs.livedoor.jp/bbs/read.cgi/computer/12345/1234567890/l50")?
        else {
            panic!()
        };
        assert_eq!(thread.board.site(), Site::Shitaraba);
        assert_eq!(
            thread.board.url,
            "https://jbbs.shitaraba.net/computer/12345/"
        );
        assert_eq!(
            thread.data_url(),
            "https://jbbs.shitaraba.net/bbs/rawmode.cgi/computer/12345/1234567890/"
        );
        let subjects_text = "1234567890.cgi,スレッド &amp; 日本語 (3)\n0.cgi,掲示板の一覧 (1)\n";
        let bytes = encoding_rs::EUC_JP.encode(subjects_text).0.into_owned();
        let decoded = decode_with(&bytes, encoding_rs::EUC_JP);
        let threads = subjects(&decoded, &thread.board)?;
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].title, "スレッド & 日本語");
        let raw = "1<>名無しさん<>sage<>2026/10/06<>日本語 &amp; 本文<>題名<>abc\n3<>別の人<><>2026/10/06<>&gt;&gt;1<br>返信<><>def\n";
        let bytes = encoding_rs::EUC_JP.encode(raw).0.into_owned();
        let data = shitaraba_raw(&decode_with(&bytes, encoding_rs::EUC_JP), thread)?;
        assert_eq!(data.posts.len(), 3);
        assert_eq!(data.posts[1].number, 2);
        assert_eq!(data.posts[1].name, "あぼーん");
        assert_eq!(data.posts[2].id, "def");
        assert_eq!(data.posts[2].refs, vec![1]);
        assert_eq!(data.thread.title, "題名");
        assert!(shitaraba_raw("1<>broken", data.thread.clone()).is_err());
        assert!(shitaraba_raw("100001<>a<><>date<>body<>title<>id", data.thread).is_err());
        for url in [
            "https://bbs.eddibb.cc.evil.example/liveedge/",
            "https://evil.example/computer/123/",
            "https://jbbs.shitaraba.net/computer/abc/",
            "https://jbbs.shitaraba.net:8443/computer/123/",
            "https://user@bbs.eddibb.cc/liveedge/",
        ] {
            assert!(parse_url(url).is_err(), "{url}");
        }
        Ok(())
    }
    #[test]
    fn promoted_boards_use_their_normal_category() {
        let boards=menu("<B>おすすめ</B><A HREF=https://greta.5ch.io/poverty/>ニュー速(嫌儲)</A><A HREF=https://mi.5ch.io/news4vip/>ニュー速VIP</A><A HREF=https://krsw.5ch.io/yanineko/>ヤニねこ</A><B>特別企画</B><A HREF=https://greta.5ch.io/poverty/>ニュー速(嫌儲)</A><B>雑談系２</B><A HREF=https://mi.5ch.io/news4vip/>ニュー速VIP</A><A HREF=https://greta.5ch.io/poverty/>ニュー速(嫌儲)</A>").unwrap();
        assert_eq!(boards.len(), 3);
        assert_eq!(
            boards
                .iter()
                .find(|b| b.slug() == "poverty")
                .unwrap()
                .category,
            "雑談系２"
        );
        assert_eq!(
            boards
                .iter()
                .find(|b| b.slug() == "news4vip")
                .unwrap()
                .category,
            "雑談系２"
        );
        assert_eq!(
            boards
                .iter()
                .find(|b| b.slug() == "yanineko")
                .unwrap()
                .category,
            "おすすめ"
        );
    }
    #[test]
    fn actual_formats_and_encoding() {
        let (b, _) = crate::model::demo();
        let text = "12345.dat<>日本語 &amp; スレ (12)\n54321.dat<>別のスレ (1)\n";
        let encoded = SHIFT_JIS.encode(text).0;
        assert_eq!(
            subjects(&decode(&encoded), &b[0]).unwrap()[0].title,
            "日本語 & スレ"
        );
        let thread = subjects(text, &b[0]).unwrap().remove(0);
        let d = dat(
            "<b>名無し</b><>sage<>2026 ID:abc<>本文<br>&gt;&gt;1-3 &#x1f600;<>題名\n",
            thread,
        )
        .unwrap();
        assert_eq!(d.posts[0].refs, vec![1, 2, 3]);
        assert_eq!(d.posts[0].id, "abc");
        assert!(d.posts[0].body.contains('😀'));
    }
    #[test]
    fn menu_and_url_validation() {
        let b = menu("<B>ニュース</B><A HREF=https://news.5ch.net/news/>ニュース速報</A>").unwrap();
        assert_eq!(b[0].url, "https://news.5ch.io/news/");
        assert!(parse_url("https://5ch.io.evil.example/news/").is_err());
        assert!(parse_url("https://egg.5ch.io:1234/software/").is_err());
        assert!(parse_url("https://egg.5ch.io/test/read.cgi/software/123456/").is_ok());
    }
    #[test]
    fn controls_and_malformed_dat() {
        assert!(!html_text("hi&#27;[31m\u{7}").contains('\u{1b}'));
        let (_, d) = crate::model::demo();
        assert!(dat("<html>denied</html>", d[0].thread.clone()).is_err());
    }
}
