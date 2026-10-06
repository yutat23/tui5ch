use anyhow::{Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    FiveCh,
    Edge,
    Shitaraba,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Board {
    pub name: String,
    pub category: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
    pub key: String,
    pub title: String,
    pub count: usize,
    pub board: Board,
}
impl Thread {
    pub fn url(&self) -> String {
        let base = Url::parse(&self.board.url).unwrap();
        let origin = base.origin().ascii_serialization();
        match self.board.site() {
            Site::Shitaraba => format!(
                "{origin}/bbs/read.cgi/{}/{}/{}/",
                self.board.directory().unwrap_or(""),
                self.board.slug(),
                self.key
            ),
            Site::Edge => format!("{origin}/{}/{}/", self.board.slug(), self.key),
            Site::FiveCh => format!("{origin}/test/read.cgi/{}/{}/", self.board.slug(), self.key),
        }
    }
    pub fn data_url(&self) -> String {
        if self.board.site() == Site::Shitaraba {
            self.url().replace("/bbs/read.cgi/", "/bbs/rawmode.cgi/")
        } else {
            format!("{}dat/{}.dat", self.board.url, self.key)
        }
    }
    pub fn created_timestamp(&self) -> Option<i64> {
        self.key.parse::<i64>().ok().filter(|&value| value >= 0)
    }
    pub fn created_label(&self) -> String {
        self.created_timestamp()
            .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
            .map(|date| {
                date.with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap())
                    .format("%m/%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|| "—".into())
    }
    pub fn momentum(&self) -> f64 {
        let created = self.key.parse::<u64>().unwrap_or(0);
        let age = now().saturating_sub(created).max(3600) as f64 / 86400.0;
        self.count as f64 / age
    }
}
impl Board {
    pub fn site(&self) -> Site {
        let host = Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        if host == "jbbs.shitaraba.net" {
            Site::Shitaraba
        } else if host == "eddibb.cc" || host.ends_with(".eddibb.cc") {
            Site::Edge
        } else {
            Site::FiveCh
        }
    }
    pub fn directory(&self) -> Option<&str> {
        if self.site() == Site::Shitaraba {
            self.url.split('/').nth(3)
        } else {
            None
        }
    }
    pub fn subjects_url(&self) -> String {
        format!("{}subject.txt", self.url)
    }
    pub fn slug(&self) -> &str {
        self.url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub number: usize,
    pub name: String,
    pub mail: String,
    pub date: String,
    pub id: String,
    pub body: String,
    pub refs: Vec<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadData {
    pub thread: Thread,
    pub posts: Vec<Post>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ThreadState {
    pub read: usize,
    pub favorite: bool,
    pub visited: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub menu_url: String,
    pub refresh_seconds: u64,
    pub theme: String,
    pub wrap: bool,
    pub ng_chain: bool,
    pub keys: HashMap<String, String>,
    pub layout: String,
    pub board_width: u16,
    pub thread_width: u16,
    pub thread_height: u16,
    pub header_height: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            menu_url: "https://menu.5ch.io/bbsmenu.html".into(),
            refresh_seconds: 60,
            theme: "dark".into(),
            wrap: true,
            ng_chain: true,
            keys: HashMap::new(),
            layout: "auto".into(),
            board_width: 27,
            thread_width: 36,
            thread_height: 10,
            header_height: 3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NgRule {
    pub field: String,
    pub pattern: String,
    pub regex: bool,
    pub expires: Option<u64>,
}
impl NgRule {
    pub fn matches(&self, field: &str, value: &str) -> bool {
        if self.field != field || self.expires.is_some_and(|t| t <= now()) {
            return false;
        }
        if self.regex {
            Regex::new(&self.pattern).is_ok_and(|r| r.is_match(value))
        } else {
            value.to_lowercase().contains(&self.pattern.to_lowercase())
        }
    }
}
pub fn hidden_posts(posts: &[Post], rules: &[NgRule], chain: bool) -> Vec<bool> {
    let active: Vec<_> = rules
        .iter()
        .filter(|r| r.expires.is_none_or(|t| t > now()))
        .map(|r| {
            (
                r,
                if r.regex {
                    Regex::new(&r.pattern).ok()
                } else {
                    None
                },
            )
        })
        .collect();
    let mut hidden: Vec<bool> = posts
        .iter()
        .map(|p| {
            active.iter().any(|(r, re)| {
                let value = match r.field.as_str() {
                    "id" => &p.id,
                    "name" => &p.name,
                    "body" => &p.body,
                    _ => return false,
                };
                if r.regex {
                    re.as_ref().is_some_and(|re| re.is_match(value))
                } else {
                    value.to_lowercase().contains(&r.pattern.to_lowercase())
                }
            })
        })
        .collect();
    if chain {
        loop {
            let mut changed = false;
            for (i, p) in posts.iter().enumerate() {
                if !hidden[i]
                    && p.refs
                        .iter()
                        .any(|&n| n > 0 && hidden.get(n - 1).copied().unwrap_or(false))
                {
                    hidden[i] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
    hidden
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub enum Target {
    Board(Board),
    Thread(Thread),
}
pub fn parse_url(input: &str) -> Result<Target> {
    let mut url = Url::parse(input.trim())?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("http/https URLを指定してください");
    }
    let host = url.host_str().unwrap_or("").to_owned();
    let host = if host == "5ch.net" {
        "5ch.io".into()
    } else if let Some(s) = host.strip_suffix(".5ch.net") {
        format!("{s}.5ch.io")
    } else {
        host
    };
    let host = if matches!(host.as_str(), "jbbs.livedoor.jp" | "jbbs.livedoor.com") {
        "jbbs.shitaraba.net".to_owned()
    } else {
        host
    };
    let site = if host == "jbbs.shitaraba.net" {
        Site::Shitaraba
    } else if host == "eddibb.cc" || host.ends_with(".eddibb.cc") {
        Site::Edge
    } else if host == "5ch.io" || host.ends_with(".5ch.io") {
        Site::FiveCh
    } else {
        bail!("対応URLは5ch・エッヂ・したらばです");
    };
    if url.port().is_some() || !url.username().is_empty() || url.password().is_some() {
        bail!("ポート番号や認証情報を含むURLには対応しません");
    }
    url.set_host(Some(&host))?;
    url.set_scheme("https").ok();
    let p: Vec<_> = url.path().split('/').filter(|s| !s.is_empty()).collect();
    let (slug, key, directory) = if site == Site::Shitaraba {
        if p.len() >= 5
            && p[0] == "bbs"
            && matches!(p[1], "read.cgi" | "rawmode.cgi" | "read_archive.cgi")
        {
            (p[3], Some(p[4]), Some(p[2]))
        } else if p.len() == 2 || p.len() == 3 && p[2] == "subject.txt" {
            (p[1], None, Some(p[0]))
        } else {
            bail!("したらばの板・スレURLを指定してください");
        }
    } else if p.len() >= 4 && p[0] == "test" && p[1] == "read.cgi" {
        (p[2], Some(p[3]), None)
    } else if p.len() == 3 && p[1] == "dat" {
        (p[0], Some(p[2].trim_end_matches(".dat")), None)
    } else if site == Site::Edge && p.len() >= 2 && p[1].chars().all(|c| c.is_ascii_digit()) {
        (p[0], Some(p[1]), None)
    } else if p.len() == 1 || p.len() == 2 && p[1] == "subject.txt" {
        (p[0], None, None)
    } else {
        bail!("板またはスレのURLを指定してください");
    };
    let valid_slug =
        |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid_slug(slug)
        || directory.is_some_and(|d| !valid_slug(d))
        || site == Site::Shitaraba && !slug.chars().all(|c| c.is_ascii_digit())
    {
        bail!("不正な板名です");
    }
    let board = Board {
        name: if site == Site::Edge && slug == "liveedge" {
            "エッヂ".into()
        } else if let Some(dir) = directory {
            format!("{dir}/{slug}")
        } else {
            slug.into()
        },
        category: match site {
            Site::FiveCh => "URLから追加",
            Site::Edge => "エッヂ",
            Site::Shitaraba => "したらば",
        }
        .into(),
        url: if let Some(dir) = directory {
            format!("https://{host}/{dir}/{slug}/")
        } else {
            format!("https://{host}/{slug}/")
        },
    };
    if let Some(key) = key {
        if key.len() < 4 || !key.chars().all(|c| c.is_ascii_digit()) {
            bail!("不正なスレッドキーです");
        }
        Ok(Target::Thread(Thread {
            key: key.into(),
            title: "読み込み中".into(),
            count: 0,
            board,
        }))
    } else {
        Ok(Target::Board(board))
    }
}

pub fn demo() -> (Vec<Board>, Vec<ThreadData>) {
    let board = Board {
        name: "デモ掲示板".into(),
        category: "オフラインデモ".into(),
        url: "https://agree.5ch.io/operate/".into(),
    };
    let bodies = [
        "tui5chへようこそ。\nTabでペインを切替、j/kで移動、Enterで開きます。\n実際の投稿ではなく、操作確認用のデータです。",
        "お気に入りは f、更新は r。\n/ で検索、n で未読、g でレス番号へジャンプ。",
        ">>1\n参照は a、同じIDの抽出は i、返信一覧は t。\nhttps://www.rust-lang.org/",
        ">>3\nAAを読むときは W で折り返しを切り替えられます。\n  /\\_/\\\n ( o.o )\n  > ^ <",
        "NGの追加は N。\nid、name、body、titleから対象を選べます。",
        "長い日本語の行も端末幅に合わせて表示します。閲覧位置・お気に入り・下書きはローカルに保存されます。",
        ">>1 >>3\nwで返信の下書きを作成できます。内容確認と送信もTUI内で行います。",
        "Spaceでページ送り、Aで自動更新と新着追従。\n? でキー一覧を開けます。",
    ];
    let posts = bodies
        .iter()
        .enumerate()
        .map(|(i, b)| Post {
            number: i + 1,
            name: "名無しさん".into(),
            mail: "sage".into(),
            date: "2026/10/04 12:00:00".into(),
            id: format!("demo{}", i % 3),
            body: (*b).into(),
            refs: crate::parse::references(b),
        })
        .collect();
    let thread = Thread {
        key: "1791068400".into(),
        title: "tui5chの使い方を試すスレ".into(),
        count: 8,
        board: board.clone(),
    };
    let second = ThreadData {
        thread: Thread {
            key: "1791068401".into(),
            title: "タブ・お気に入り・検索を試すスレ".into(),
            count: 1,
            board: board.clone(),
        },
        posts: vec![Post {
            number: 1,
            name: "名無しさん".into(),
            mail: String::new(),
            date: "2026/10/04".into(),
            id: "demoX".into(),
            body: "別のスレです。 [ / ] で開いているタブを切り替えられます。".into(),
            refs: vec![],
        }],
    };
    (vec![board], vec![ThreadData { thread, posts }, second])
}
