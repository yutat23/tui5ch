use crate::model::{Board, Config, NgRule, Thread, ThreadData, ThreadState};
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Connection, Row, SqliteConnection, sqlite::SqliteConnectOptions};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use tokio::runtime::Runtime;

pub struct Store {
    pub dir: PathBuf,
    rt: Runtime,
    conn: RefCell<Option<SqliteConnection>>,
}
impl Drop for Store {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.get_mut().take() {
            let _ = self.rt.block_on(conn.close());
        }
    }
}
impl Store {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let rt = Runtime::new()?;
        let opts = SqliteConnectOptions::new()
            .filename(dir.join("tui5ch.sqlite3"))
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let mut conn = rt.block_on(SqliteConnection::connect_with(&opts))?;
        rt.block_on(
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS cache (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            )
            .execute(&mut conn),
        )?;
        rt.block_on(sqlx::query("CREATE TABLE IF NOT EXISTS threads (url TEXT PRIMARY KEY, data TEXT NOT NULL, state TEXT NOT NULL)").execute(&mut conn))?;
        Ok(Self {
            dir: dir.into(),
            rt,
            conn: RefCell::new(Some(conn)),
        })
    }
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let row = self.rt.block_on(
            sqlx::query("SELECT value FROM cache WHERE key=?")
                .bind(key)
                .fetch_optional(self.conn.borrow_mut().as_mut().unwrap()),
        )?;
        row.map(|r| serde_json::from_str(&r.get::<String, _>(0)).map_err(Into::into))
            .transpose()
    }
    pub fn set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> Result<()> {
        self.rt.block_on(sqlx::query("INSERT INTO cache(key,value) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .bind(key).bind(serde_json::to_string(value)?).execute(self.conn.borrow_mut().as_mut().unwrap()))?;
        Ok(())
    }
    pub fn save_thread(&self, data: &ThreadData) -> Result<()> {
        self.rt.block_on(sqlx::query("INSERT INTO threads(url,data,state) VALUES (?,?,?) ON CONFLICT(url) DO UPDATE SET data=excluded.data")
            .bind(data.thread.url()).bind(serde_json::to_string(data)?).bind(serde_json::to_string(&ThreadState::default())?).execute(self.conn.borrow_mut().as_mut().unwrap()))?;
        Ok(())
    }
    pub fn load_thread(&self, url: &str) -> Result<Option<ThreadData>> {
        let row = self.rt.block_on(
            sqlx::query("SELECT data FROM threads WHERE url=?")
                .bind(url)
                .fetch_optional(self.conn.borrow_mut().as_mut().unwrap()),
        )?;
        row.map(|r| serde_json::from_str(&r.get::<String, _>(0)).map_err(Into::into))
            .transpose()
    }
    pub fn states(&self) -> Result<std::collections::HashMap<String, ThreadState>> {
        let rows = self.rt.block_on(
            sqlx::query("SELECT url,state FROM threads")
                .fetch_all(self.conn.borrow_mut().as_mut().unwrap()),
        )?;
        rows.into_iter()
            .map(|r| {
                Ok((
                    r.get::<String, _>(0),
                    serde_json::from_str(&r.get::<String, _>(1))?,
                ))
            })
            .collect()
    }
    pub fn state(&self, url: &str, state: &ThreadState) -> Result<()> {
        self.rt.block_on(
            sqlx::query("UPDATE threads SET state=? WHERE url=?")
                .bind(serde_json::to_string(state)?)
                .bind(url)
                .execute(self.conn.borrow_mut().as_mut().unwrap()),
        )?;
        Ok(())
    }
    pub fn library(&self) -> Result<Vec<Thread>> {
        let rows = self.rt.block_on(
            sqlx::query("SELECT data FROM threads")
                .fetch_all(self.conn.borrow_mut().as_mut().unwrap()),
        )?;
        rows.into_iter()
            .map(|r| Ok(serde_json::from_str::<ThreadData>(&r.get::<String, _>(0))?.thread))
            .collect()
    }
    pub fn search_logs(&self, query: &str) -> Result<Vec<Thread>> {
        let rows = self.rt.block_on(
            sqlx::query("SELECT data FROM threads WHERE instr(lower(data), lower(?)) > 0")
                .bind(query)
                .fetch_all(self.conn.borrow_mut().as_mut().unwrap()),
        )?;
        rows.into_iter()
            .map(|r| Ok(serde_json::from_str::<ThreadData>(&r.get::<String, _>(0))?.thread))
            .collect()
    }
    pub fn config(&self) -> Result<Config> {
        let path = self.dir.join("config.toml");
        if path.exists() {
            toml::from_str(&std::fs::read_to_string(path)?).context("config.tomlを確認してください")
        } else {
            let config = Config::default();
            self.save_config(&config)?;
            Ok(config)
        }
    }
    pub fn save_config(&self, config: &Config) -> Result<()> {
        std::fs::write(
            self.dir.join("config.toml"),
            toml::to_string_pretty(config)?,
        )?;
        Ok(())
    }
    pub fn boards(&self) -> Result<Vec<Board>> {
        Ok(self.get("boards")?.unwrap_or_default())
    }
    pub fn rules(&self) -> Result<Vec<NgRule>> {
        Ok(self.get("ng")?.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistence_preserves_read_state_on_refresh() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-test-{}-{}",
            std::process::id(),
            crate::model::now()
        ));
        let s = Store::open(&dir)?;
        let (_, demo) = crate::model::demo();
        let d = &demo[0];
        s.save_thread(d)?;
        s.state(
            &d.thread.url(),
            &ThreadState {
                read: 3,
                favorite: true,
                visited: 42,
            },
        )?;
        s.save_thread(d)?;
        assert_eq!(s.states()?[&d.thread.url()].read, 3);
        s.set("draft", "日本語の下書き")?;
        drop(s);
        let s = Store::open(&dir)?;
        assert_eq!(s.get::<String>("draft")?.unwrap(), "日本語の下書き");
        assert!(s.states()?[&d.thread.url()].favorite);
        drop(s);
        std::fs::remove_dir_all(&dir).context("テスト用DBを削除できません")?;
        Ok(())
    }
}
