use crate::{
    model::{Board, Thread, now},
    parse,
};
use anyhow::{Context, Result, bail};
use encoding_rs::SHIFT_JIS;
use regex::Regex;
use reqwest::{
    blocking::Client,
    header::{CONTENT_TYPE, COOKIE, LOCATION, REFERER, SET_COOKIE},
};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};
use url::Url;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub mail: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Draft {
    pub board: Board,
    pub thread: Option<Thread>,
    pub subject: String,
    pub name: String,
    pub mail: String,
    pub body: String,
}
impl Draft {
    pub fn url(&self) -> String {
        self.thread
            .as_ref()
            .map(Thread::url)
            .unwrap_or_else(|| self.board.url.clone())
    }
    pub fn key(&self) -> String {
        format!("post-draft:{}", self.url())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    host_only: bool,
    expires: Option<u64>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Jar(pub Vec<Cookie>);
impl Jar {
    fn header(&self, url: &Url) -> String {
        let host = url.host_str().unwrap_or("");
        let path = url.path();
        self.0
            .iter()
            .filter(|c| {
                c.expires.is_none_or(|t| t > now())
                    && (!c.secure || url.scheme() == "https")
                    && (host == c.domain
                        || !c.host_only && host.ends_with(&format!(".{}", c.domain)))
                    && (path == c.path
                        || path.starts_with(&c.path)
                            && (c.path.ends_with('/')
                                || path.as_bytes().get(c.path.len()) == Some(&b'/')))
            })
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }
    fn set(&mut self, url: &Url, text: &str) {
        if text.len() > 4096 {
            return;
        }
        let host = url.host_str().unwrap_or("");
        let mut parts = text.split(';');
        let Some((name, value)) = parts.next().and_then(|s| s.split_once('=')) else {
            return;
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty()
            || name
                .bytes()
                .any(|c| c <= 32 || c >= 127 || b"()<>@,;:\\\"/[]?={}".contains(&c))
            || value.chars().any(|c| c.is_control())
        {
            return;
        }
        let mut cookie = Cookie {
            name: name.into(),
            value: value.into(),
            domain: host.into(),
            path: url
                .path()
                .rsplit_once('/')
                .map(|(p, _)| if p.is_empty() { "/" } else { p })
                .unwrap_or("/")
                .into(),
            secure: false,
            host_only: true,
            expires: None,
        };
        let mut max_age = None;
        for part in parts {
            let (key, val) = part.trim().split_once('=').unwrap_or((part.trim(), ""));
            match key.to_ascii_lowercase().as_str() {
                "domain" => {
                    let domain = val.trim_start_matches('.').to_ascii_lowercase();
                    if !(domain == "5ch.io" || domain.ends_with(".5ch.io") || domain == host)
                        || !(host == domain || host.ends_with(&format!(".{domain}")))
                    {
                        return;
                    }
                    cookie.domain = domain;
                    cookie.host_only = false;
                }
                "path" if val.starts_with('/') => cookie.path = val.into(),
                "secure" => cookie.secure = true,
                "max-age" => max_age = val.parse::<i64>().ok(),
                "expires" => {
                    cookie.expires = chrono::DateTime::parse_from_rfc2822(val)
                        .ok()
                        .map(|t| t.timestamp().max(0) as u64)
                }
                _ => {}
            }
        }
        if let Some(age) = max_age {
            cookie.expires = Some(if age <= 0 {
                0
            } else {
                now().saturating_add(age as u64)
            });
        }
        if cookie.name.starts_with("__Secure-") && (!cookie.secure || url.scheme() != "https") {
            return;
        }
        if cookie.name.starts_with("__Host-")
            && (!cookie.secure
                || !cookie.host_only
                || cookie.path != "/"
                || url.scheme() != "https")
        {
            return;
        }
        self.0.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
                && c.expires.is_none_or(|t| t > now())
        });
        if cookie.expires.is_none_or(|t| t > now()) {
            self.0.push(cookie);
        }
        if self.0.len() > 128 {
            self.0.remove(0);
        }
    }
}
#[derive(Clone, Debug)]
pub struct Choice {
    pub label: String,
    pub name: String,
    pub value: String,
}
#[derive(Clone, Debug)]
pub struct Checkbox {
    pub label: String,
    pub name: String,
    pub value: String,
    pub checked: bool,
}
#[derive(Clone, Debug)]
pub struct Form {
    pub action: String,
    pub referer: String,
    pub fields: Vec<(String, String)>,
    pub choices: Vec<Choice>,
    pub checks: Vec<Checkbox>,
    pub utf8: bool,
}
#[derive(Clone, Debug)]
pub struct Page {
    pub html: String,
    pub text: String,
    pub url: String,
    pub forms: Vec<Form>,
    pub success: bool,
    pub status: u16,
}
pub enum Request {
    Start(Draft, Jar),
    Confirm(Draft, Form, usize, Jar),
}
pub struct Response {
    pub draft: Draft,
    pub jar: Jar,
    pub result: Result<Page, String>,
}
pub struct Posting {
    pub tx: Sender<Request>,
    pub rx: Receiver<Response>,
}
impl Posting {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let (out, results) = mpsc::channel();
        std::thread::spawn(move || {
            let client = Client::builder()
                .user_agent("Monazilla/1.00 tui5ch/0.2")
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(20))
                .connect_timeout(Duration::from_secs(8))
                .build();
            for request in rx {
                let (draft, mut jar, form, choice) = match request {
                    Request::Start(d, j) => (d, j, None, 0),
                    Request::Confirm(d, f, c, j) => (d, j, Some(f), c),
                };
                let result=client.as_ref().map_err(|e|e.to_string()).and_then(|client|{
                (||->Result<Page>{
                    let form=if let Some(f)=form{f}else{
                        let(url,html,status)=get(client,&draft.url(),&mut jar)?;
                        if status>=400{bail!("投稿先の取得に失敗: HTTP {status}\n{}",parse::html_text(&html));}
                        let forms=forms(&html,&url)?;
                        let mut form=forms.into_iter().find(|f|f.fields.iter().any(|(n,v)|n=="bbs"&&v==draft.board.slug())&&f.fields.iter().any(|(n,_)|n=="MESSAGE")&&if let Some(t)=&draft.thread{f.fields.iter().any(|(n,v)|n=="key"&&v==&t.key)}else{f.fields.iter().any(|(n,_)|n=="subject")}).ok_or_else(||anyhow::anyhow!("投稿フォームが取得できません。JavaScript専用フォーム・認証要求・規制には対応できない場合があります"))?;
                        replace(&mut form.fields,"FROM",&draft.name);replace(&mut form.fields,"mail",&draft.mail);replace(&mut form.fields,"MESSAGE",&draft.body);if draft.thread.is_none(){replace(&mut form.fields,"subject",&draft.subject);}form
                    };
                    send(client,&draft,&form,choice,&mut jar)
                })().map_err(|e|e.to_string())
            });
                if out.send(Response { draft, jar, result }).is_err() {
                    break;
                }
            }
        });
        Self { tx, rx: results }
    }
}
fn replace(fields: &mut Vec<(String, String)>, name: &str, value: &str) {
    fields.retain(|(n, _)| n != name);
    fields.push((name.into(), value.into()));
}
fn allowed(url: &Url) -> bool {
    #[cfg(test)]
    if url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return true;
    }

    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|h| h == "5ch.io" || h.ends_with(".5ch.io"))
}
fn checked(url: &str) -> Result<Url> {
    let u = Url::parse(url)?;
    if !allowed(&u) {
        bail!("5ch.io以外・HTTPの投稿先には送信しません");
    }
    Ok(u)
}
fn body(response: &mut reqwest::blocking::Response) -> Result<String> {
    let mut bytes = vec![];
    Read::by_ref(response)
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 {
        bail!("投稿応答が2MBを超えています。結果不明のため再送していません");
    }
    Ok(parse::decode(&bytes))
}
fn cookies(jar: &mut Jar, url: &Url, response: &reqwest::blocking::Response) {
    for h in response.headers().get_all(SET_COOKIE) {
        if let Ok(s) = h.to_str() {
            jar.set(url, s);
        }
    }
}
fn get(client: &Client, url: &str, jar: &mut Jar) -> Result<(String, String, u16)> {
    let mut url = checked(url)?;
    for _ in 0..5 {
        let mut response = client
            .get(url.clone())
            .header(COOKIE, jar.header(&url))
            .send()?;
        cookies(jar, &url, &response);
        if response.status().is_redirection() {
            let loc = response
                .headers()
                .get(LOCATION)
                .and_then(|h| h.to_str().ok())
                .context("リダイレクト先がありません")?;
            url = checked(url.join(loc)?.as_str())?;
            continue;
        }
        let status = response.status().as_u16();
        return Ok((url.into(), body(&mut response)?, status));
    }
    bail!("リダイレクトが多すぎます")
}
fn send(client: &Client, draft: &Draft, form: &Form, choice: usize, jar: &mut Jar) -> Result<Page> {
    let url = checked(&form.action)?;
    let referer = checked(&form.referer)?;
    if url.host_str() != Url::parse(&draft.board.url)?.host_str() {
        bail!("投稿先サーバーが板のサーバーと異なるため停止しました");
    }
    if !matches!(url.path(), "/test/bbs.cgi" | "/test/subbbs.cgi") {
        bail!("未対応の投稿エンドポイントです");
    }
    let mut fields = form.fields.clone();
    for c in &form.checks {
        if c.checked {
            fields.push((c.name.clone(), c.value.clone()));
        }
    }
    let submit = form
        .choices
        .get(choice)
        .context("送信ボタンが選択されていません")?;
    if !submit.name.is_empty() {
        fields.push((submit.name.clone(), submit.value.clone()));
    }
    let encoded = encode(&fields, form.utf8);
    let origin = format!("{}://{}", url.scheme(), url.host_str().unwrap());
    let mut response=client.post(url.clone()).header(COOKIE,jar.header(&url)).header(REFERER,referer.as_str()).header("Origin",origin).header(CONTENT_TYPE,"application/x-www-form-urlencoded").body(encoded).send().context("送信結果を確認できません。重複投稿を避けるため自動再送していません。スレを更新して確認してください")?;
    cookies(jar, &url, &response);
    let status = response.status().as_u16();
    let html = body(&mut response)?;
    if response.status().is_redirection() {
        bail!(
            "投稿後にHTTP {status}で移動が指示されました。投稿結果は不明です。スレを更新して確認してください（自動再送なし）"
        );
    }
    let text = parse::html_text(&html);
    let title = Regex::new(r"(?is)<title[^>]*>(.*?)</title>")
        .unwrap()
        .captures(&html)
        .map(|c| parse::html_text(&c[1]))
        .unwrap_or_default();
    let successful = |s: &str| {
        let s = s.trim();
        s.starts_with("書きこみました") || s.starts_with("書き込みました")
    };
    let success = status < 400 && (successful(&title) || successful(&text));
    let forms = if success {
        vec![]
    } else {
        forms(&html, url.as_str())
            .unwrap_or_default()
            .into_iter()
            .filter(|f| {
                f.fields
                    .iter()
                    .any(|(n, v)| n == "bbs" && v == draft.board.slug())
            })
            .collect()
    };
    Ok(Page {
        html,
        text,
        url: url.into(),
        forms,
        success,
        status,
    })
}
fn encode(fields: &[(String, String)], utf8: bool) -> String {
    let field = |s: &str| {
        let bytes = if utf8 {
            s.as_bytes().to_vec()
        } else {
            SHIFT_JIS.encode(s).0.into_owned()
        };
        let mut out = String::new();
        for b in bytes {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                    out.push(b as char)
                }
                b' ' => out.push('+'),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    };
    fields
        .iter()
        .map(|(n, v)| {
            format!(
                "{}={}",
                field(n),
                field(&v.replace("\r\n", "\n").replace('\n', "\r\n"))
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}
fn attrs(tag: &str) -> std::collections::HashMap<String, String> {
    let re = Regex::new(
        r#"(?is)(?:^|\s)([a-zA-Z_:][\w:.-]*)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+)))?"#,
    )
    .unwrap();
    re.captures_iter(tag)
        .map(|c| {
            (
                c[1].to_ascii_lowercase(),
                c.get(2)
                    .or(c.get(3))
                    .or(c.get(4))
                    .map(|s| parse::html_text(s.as_str()))
                    .unwrap_or_default(),
            )
        })
        .collect()
}
fn forms(html: &str, page: &str) -> Result<Vec<Form>> {
    let base = checked(page)?;
    let re = Regex::new(r"(?is)<form\b([^>]*)>(.*?)</form>")?;
    let controls = Regex::new(
        r"(?is)<input\b[^>]*>|<textarea\b([^>]*)>(.*?)</textarea>|<button\b([^>]*)>(.*?)</button>",
    )?;
    let mut result = vec![];
    let utf8 = Regex::new(r#"(?i)charset\s*=\s*["']?utf-8"#)
        .unwrap()
        .is_match(html);
    for form in re.captures_iter(html) {
        let a = attrs(&form[1]);
        if !a
            .get("method")
            .is_some_and(|m| m.eq_ignore_ascii_case("post"))
        {
            continue;
        }
        let action = base.join(a.get("action").map(String::as_str).unwrap_or(page))?;
        if !allowed(&action) || !matches!(action.path(), "/test/bbs.cgi" | "/test/subbbs.cgi") {
            continue;
        }
        let mut f = Form {
            action: action.into(),
            referer: page.into(),
            fields: vec![],
            choices: vec![],
            checks: vec![],
            utf8: a
                .get("accept-charset")
                .is_some_and(|s| s.eq_ignore_ascii_case("utf-8"))
                || utf8,
        };
        for c in controls.captures_iter(&form[2]) {
            let tag = c[0].split('>').next().unwrap();
            let a = attrs(tag);
            if a.contains_key("disabled") {
                continue;
            }
            let name = a.get("name").cloned().unwrap_or_default();
            let typ = a.get("type").map(|s| s.to_lowercase()).unwrap_or_else(|| {
                if tag.to_lowercase().starts_with("<button") {
                    "submit".into()
                } else {
                    "text".into()
                }
            });
            let value = if let Some(body) = c.get(2) {
                parse::html_text(body.as_str())
            } else {
                a.get("value").cloned().unwrap_or_default()
            };
            if typ == "submit" {
                f.choices.push(Choice {
                    label: if let Some(label) = c.get(4) {
                        parse::html_text(label.as_str())
                    } else {
                        value.clone()
                    },
                    name,
                    value,
                });
            } else if typ == "checkbox" {
                f.checks.push(Checkbox {
                    label: name.clone(),
                    name,
                    value: if value.is_empty() { "on".into() } else { value },
                    checked: a.contains_key("checked"),
                });
            } else if !name.is_empty()
                && !matches!(typ.as_str(), "button" | "reset" | "file" | "radio")
            {
                f.fields.push((name, value));
            }
        }
        if !f.choices.is_empty() {
            result.push(f);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cookies_scope_expiry_and_encoding() {
        let mut j = Jar::default();
        let url = Url::parse("https://egg.5ch.io/test/bbs.cgi").unwrap();
        j.set(
            &url,
            "MonaTicket=abc; Domain=.5ch.io; Path=/; Secure; Max-Age=60",
        );
        assert!(
            j.header(&Url::parse("https://agree.5ch.io/test/bbs.cgi").unwrap())
                .contains("MonaTicket=abc")
        );
        assert!(
            j.header(&Url::parse("https://evil.example/").unwrap())
                .is_empty()
        );
        j.set(&url, "MonaTicket=gone; Domain=.5ch.io; Path=/; Max-Age=0");
        assert!(j.header(&url).is_empty());
        assert_eq!(
            encode(&[("MESSAGE".into(), "日本\n&".into())], false),
            "MESSAGE=%93%FA%96%7B%0D%0A%26"
        );
    }
    #[test]
    fn confirmation_preserves_hidden_fields_and_buttons() {
        let html = "<form method='POST' action='/test/bbs.cgi'><input type='hidden' name='bbs' value='operate'><input name='nonce' value='a&amp;b'><textarea name='MESSAGE'>日本語</textarea><input type='submit' name='submit' value='承諾して書き込む'><input type='submit' name='submit' value='キャンセル'></form>";
        let f = forms(html, "https://agree.5ch.io/test/bbs.cgi")
            .unwrap()
            .remove(0);
        assert!(f.fields.contains(&("nonce".into(), "a&b".into())));
        assert_eq!(f.choices.len(), 2);
        assert_eq!(
            f.fields.iter().find(|(n, _)| n == "MESSAGE").unwrap().1,
            "日本語"
        );
        assert!(
            forms(
                &html.replace("/test/bbs.cgi", "https://evil.example/test/bbs.cgi"),
                "https://agree.5ch.io/test/bbs.cgi"
            )
            .unwrap()
            .is_empty()
        );
    }
    #[test]
    fn local_http_reply_and_thread_confirmation() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        for new_thread in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let root = format!("http://{}", listener.local_addr().unwrap());
            let (tx, rx) = mpsc::channel();
            let server = std::thread::spawn(move || {
                for stage in 0..3 {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buf = [0; 4096];
                    loop {
                        let n = stream.read(&mut buf).unwrap();
                        request.extend_from_slice(&buf[..n]);
                        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&request[..end]);
                            let length = headers
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .and_then(|v| v.parse::<usize>().ok())
                                })
                                .unwrap_or(0);
                            if request.len() >= end + 4 + length {
                                break;
                            }
                        }
                    }
                    tx.send(String::from_utf8(request).unwrap()).unwrap();
                    let html = if stage == 2 {
                        "<title>書きこみました。</title>完了".to_string()
                    } else {
                        format!(
                            "<title>確認</title><form method='post' action='/test/bbs.cgi'><input name='bbs' value='operate'><input name='{}' value='{}'><input name='nonce' value='n{}'><input name='FROM'><input name='mail'><textarea name='MESSAGE'>保留本文</textarea><input type='submit' name='submit' value='承諾して書き込む'></form>",
                            if new_thread { "subject" } else { "key" },
                            if new_thread { "新スレ" } else { "123" },
                            stage
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nSet-Cookie: MonaTicket=local; Path=/\r\nConnection: close\r\n\r\n{}",
                        html.len(),
                        html
                    );
                    stream.write_all(response.as_bytes()).unwrap();
                }
            });
            let client = Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            let board = Board {
                name: "テスト".into(),
                category: "test".into(),
                url: format!("{root}/operate/"),
            };
            let draft = Draft {
                thread: None,
                board,
                subject: "新スレ".into(),
                name: "名前".into(),
                mail: "sage".into(),
                body: "日本語\n本文".into(),
            };
            let mut jar = Jar::default();
            let (url, html, _) = get(&client, &draft.board.url, &mut jar).unwrap();
            let mut form = forms(&html, &url).unwrap().remove(0);
            replace(&mut form.fields, "MESSAGE", &draft.body);
            replace(&mut form.fields, "FROM", &draft.name);
            replace(&mut form.fields, "mail", &draft.mail);
            let first = send(&client, &draft, &form, 0, &mut jar).unwrap();
            assert!(!first.success);
            assert_eq!(first.forms.len(), 1);
            let done = send(&client, &draft, &first.forms[0], 0, &mut jar).unwrap();
            assert!(done.success);
            server.join().unwrap();
            let requests: Vec<_> = rx.try_iter().collect();
            assert_eq!(requests.len(), 3);
            assert!(requests[0].starts_with("GET "));
            assert!(requests[1].starts_with("POST /test/bbs.cgi"));
            assert!(requests[1].contains("MESSAGE=%93%FA%96%7B%8C%EA%0D%0A%96%7B%95%B6"));
            assert!(
                requests[1]
                    .to_ascii_lowercase()
                    .contains("cookie: monaticket=local")
            );
            assert!(requests[2].contains("nonce=n1"));
            assert!(requests[1].contains(if new_thread { "subject=" } else { "key=123" }));
        }
    }
}
