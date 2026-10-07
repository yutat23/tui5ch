use crate::{
    model::*,
    net::{self, Network, Request, Response},
    store::Store,
};
use anyhow::{Result, bail};
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Boards,
    Threads,
    Posts,
}
#[derive(Clone, Copy)]
pub enum Divider {
    Boards,
    Threads,
    Tabs,
}
pub struct Drag {
    pub divider: Divider,
    pub column: u16,
    pub row: u16,
    pub original: u16,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Board,
    Favorites,
    History,
    Logs,
}
#[derive(Clone, Copy, Debug)]
pub enum Prompt {
    Search,
    Url,
    Jump,
    Ng,
    Export,
    Refresh,
    Logs,
    Name,
    Mail,
    Subject,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadSort {
    Count,
    Unread,
    Momentum,
    Created,
}
impl ThreadSort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Count => "レス",
            Self::Unread => "未読",
            Self::Momentum => "勢い",
            Self::Created => "日時",
        }
    }
}
#[derive(Clone, Debug)]
pub enum BoardRow {
    Category(String, String, usize),
    Board(usize, bool),
}
pub enum Mode {
    Normal,
    Input(Prompt, Editor),
    Help(usize),
    Popup(String, Vec<PopupItem>, usize),
    Draft(Editor),
    Confirm(Editor),
    Sending,
    Server(crate::posting::Page, usize, usize, bool),
}
pub struct PopupItem {
    pub label: String,
    pub post: Option<usize>,
    pub url: Option<String>,
}
#[derive(Clone, Copy)]
pub struct PostScrollbar {
    pub rect: ratatui::layout::Rect,
    pub max_scroll: usize,
    pub thumb_top: u16,
    pub thumb_len: u16,
}
#[derive(Clone, Copy)]
pub enum TabAction {
    Select(usize),
    Close(usize),
    Previous,
    Next,
}
pub struct Tab {
    pub data: ThreadData,
    pub selected: usize,
    pub scroll: usize,
    pub boundary: usize,
    pub auto: bool,
    pub follow: bool,
    pub refreshed: Instant,
    pub rendered: Option<(usize, u16)>,
    pub jump_bottom: bool,
}
#[derive(Default)]
pub struct Editor {
    pub text: String,
    pub cursor: usize,
}
impl Editor {
    pub fn new(text: String) -> Self {
        let cursor = text.len();
        Self { text, cursor }
    }
    pub fn insert(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }
    pub fn key(&mut self, key: KeyEvent, multiline: bool) {
        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&c.to_string())
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let pos = self.text[..self.cursor].char_indices().last().unwrap().0;
                    self.text.drain(pos..self.cursor);
                    self.cursor = pos;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.text.len() {
                    let len = self.text[self.cursor..].chars().next().unwrap().len_utf8();
                    self.text.drain(self.cursor..self.cursor + len);
                }
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor = self.text[..self.cursor].char_indices().last().unwrap().0;
                }
            }
            KeyCode::Right => {
                if self.cursor < self.text.len() {
                    self.cursor += self.text[self.cursor..].chars().next().unwrap().len_utf8();
                }
            }
            KeyCode::Home => {
                self.cursor = self.text[..self.cursor]
                    .rfind('\n')
                    .map(|n| n + 1)
                    .unwrap_or(0)
            }
            KeyCode::End => {
                self.cursor += self.text[self.cursor..]
                    .find('\n')
                    .unwrap_or(self.text.len() - self.cursor)
            }
            KeyCode::Up if multiline => {
                let start = self.text[..self.cursor]
                    .rfind('\n')
                    .map(|n| n + 1)
                    .unwrap_or(0);
                let col = self.text[start..self.cursor].chars().count();
                if start > 0 {
                    let end = start - 1;
                    let prev = self.text[..end].rfind('\n').map(|n| n + 1).unwrap_or(0);
                    self.cursor = prev
                        + self.text[prev..end]
                            .char_indices()
                            .nth(col)
                            .map(|(n, _)| n)
                            .unwrap_or(end - prev);
                }
            }
            KeyCode::Down if multiline => {
                let start = self.text[..self.cursor]
                    .rfind('\n')
                    .map(|n| n + 1)
                    .unwrap_or(0);
                let col = self.text[start..self.cursor].chars().count();
                if let Some(offset) = self.text[self.cursor..].find('\n') {
                    let next = self.cursor + offset + 1;
                    let end = next
                        + self.text[next..]
                            .find('\n')
                            .unwrap_or(self.text.len() - next);
                    self.cursor = next
                        + self.text[next..end]
                            .char_indices()
                            .nth(col)
                            .map(|(n, _)| n)
                            .unwrap_or(end - next);
                }
            }
            KeyCode::Enter if multiline => self.insert("\n"),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.clear();
                self.cursor = 0;
            }
            _ => {}
        }
    }
}

pub struct App {
    pub store: Store,
    pub config: Config,
    pub boards: Vec<Board>,
    pub threads: Vec<Thread>,
    pub states: HashMap<String, ThreadState>,
    pub board_favorites: Vec<String>,
    pub rules: Vec<NgRule>,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub tab_offset: usize,
    pub tab_area: ratatui::layout::Rect,
    pub tab_hits: Vec<(ratatui::layout::Rect, TabAction)>,
    pub command_hits: Vec<(ratatui::layout::Rect, KeyCode)>,
    pub focus: Focus,
    pub view: View,
    pub board_sel: usize,
    pub board_cursor: usize,
    pub expanded: HashSet<String>,
    pub board_drag: Option<(usize, bool)>,
    pub post_actions: Vec<(ratatui::layout::Rect, char)>,
    pub refresh_hits: Vec<(ratatui::layout::Rect, Focus)>,
    pub post_scrollbar: Option<PostScrollbar>,
    pub scrollbar_drag: Option<(i32, u16)>,
    pub post_starts: Vec<(usize, usize)>,
    pub post_text_width: u16,
    pub thread_sel: usize,
    pub board_filter: String,
    pub thread_filter: String,
    pub post_filter: String,
    pub mode: Mode,
    pub status: String,
    pub offline: bool,
    pub demo: bool,
    pub drag: Option<Drag>,
    pub dividers: [ratatui::layout::Rect; 3],
    pub busy: HashSet<String>,
    pub post_height: usize,
    pub viewport: ratatui::layout::Rect,
    pub areas: [ratatui::layout::Rect; 3],
    pub board_hits: Vec<(ratatui::layout::Rect, usize)>,
    pub thread_hits: Vec<(ratatui::layout::Rect, usize)>,
    pub sort_hits: Vec<(ratatui::layout::Rect, ThreadSort)>,
    pub thread_sort: Option<ThreadSort>,
    pub sort_desc: bool,
    pub running: bool,
    network: Network,
    posting: crate::posting::Posting,
    pub compose: Option<crate::posting::Draft>,
    pending_open: HashSet<String>,
    requested_thread: Option<String>,
}
impl App {
    pub fn new(store: Store, demo: bool, offline: bool) -> Result<Self> {
        let config = store.config()?;
        let boards = store.boards()?;
        let states = store.states()?;
        let rules = store.rules()?;
        let board_favorites = store.get("board_favorites")?.unwrap_or_default();
        let expanded = store.get("board-expanded")?.unwrap_or_default();
        let mut app = Self {
            store,
            config,
            boards,
            threads: vec![],
            states,
            board_favorites,
            rules,
            tabs: vec![],
            tab: 0,
            tab_offset: 0,
            tab_area: Default::default(),
            tab_hits: vec![],
            command_hits: vec![],
            focus: Focus::Boards,
            view: View::Board,
            board_sel: 0,
            board_cursor: 0,
            expanded,
            board_drag: None,
            post_actions: vec![],
            refresh_hits: vec![],
            post_scrollbar: None,
            scrollbar_drag: None,
            post_starts: vec![],
            post_text_width: 0,
            thread_sel: 0,
            board_filter: String::new(),
            thread_filter: String::new(),
            post_filter: String::new(),
            mode: Mode::Normal,
            status: "? で操作一覧 / u でURLを開く".into(),
            offline: offline || demo,
            demo,
            drag: None,
            dividers: [Default::default(); 3],
            busy: HashSet::new(),
            post_height: 10,
            viewport: Default::default(),
            areas: [Default::default(); 3],
            board_hits: vec![],
            thread_hits: vec![],
            sort_hits: vec![],
            thread_sort: None,
            sort_desc: true,
            running: true,
            network: Network::start(),
            posting: crate::posting::Posting::start(),
            compose: None,
            pending_open: HashSet::new(),
            requested_thread: None,
        };
        if demo {
            let (boards, data) = crate::model::demo();
            app.boards = boards;
            app.threads = data.iter().map(|d| d.thread.clone()).collect();
            app.store.set("boards", &app.boards)?;
            app.store
                .set(&format!("subjects:{}", app.boards[0].url), &app.threads)?;
            for d in &data {
                app.store.save_thread(d)?;
            }
            app.open_data(data[0].clone())?;
            app.status = "DEMO: サンプルデータで操作確認 / 通信・投稿は行いません".into();
        } else {
            if !app
                .boards
                .iter()
                .any(|b| b.url == "https://bbs.eddibb.cc/liveedge/")
                && let Target::Board(board) = parse_url("https://bbs.eddibb.cc/liveedge/")?
            {
                app.boards.push(board);
                app.store.set("boards", &app.boards)?;
            }
            let session: Vec<String> = app.store.get("session")?.unwrap_or_default();
            let active = app.store.get::<Option<String>>("active-thread")?.flatten();
            for url in session {
                if let Some(data) = app.store.load_thread(&url)? {
                    app.open_data_with_activation(data, false)?;
                }
            }
            if !app.tabs.is_empty() {
                let selected = active
                    .and_then(|url| app.tabs.iter().position(|t| t.data.thread.url() == url))
                    .unwrap_or(app.tabs.len() - 1);
                app.select_tab(selected)?;
            }
            if offline {
                app.set_view(View::Logs)?;
            } else {
                app.request_menu();
                if !app.boards.is_empty() {
                    app.load_board()?;
                }
            }
        }
        Ok(app)
    }
    pub fn current(&self) -> Option<&Tab> {
        self.tabs.get(self.tab)
    }
    pub fn latest_post(&mut self) -> Result<()> {
        self.post_filter.clear();
        let last = self.post_indices().last().copied();
        if let Some(t) = self.current_mut() {
            t.selected = last.unwrap_or(0);
            t.jump_bottom = true;
            t.follow = false;
        }
        self.mark_read()?;
        self.status = "最新レスの末尾へ移動しました / Fで新着追従".into();
        Ok(())
    }
    fn scroll_posts_to(&mut self, scroll: usize) -> Result<()> {
        let max = self.post_scrollbar.map(|b| b.max_scroll).unwrap_or(0);
        let scroll = scroll.min(max);
        let selected = self
            .post_starts
            .iter()
            .rev()
            .find(|(start, _)| *start <= scroll)
            .map(|(_, i)| *i);
        let width = self.post_text_width;
        if let Some(t) = self.current_mut() {
            t.scroll = scroll;
            t.follow = false;
            t.jump_bottom = false;
            if let Some(i) = selected {
                t.selected = i;
            }
            t.rendered = Some((t.selected, width));
        }
        self.mark_read()?;
        Ok(())
    }
    fn drag_post_scrollbar(&mut self, row: u16, grab: i32) -> Result<()> {
        if let Some(bar) = self.post_scrollbar {
            let travel = bar.rect.height.saturating_sub(bar.thumb_len) as usize;
            let top = (row as i32 - bar.rect.y as i32 - grab).clamp(0, travel as i32) as usize;
            let scroll = (top * bar.max_scroll + travel / 2)
                .checked_div(travel)
                .unwrap_or(0);
            self.scroll_posts_to(scroll)?;
        }
        Ok(())
    }
    pub fn mouse(&mut self, m: MouseEvent) -> Result<()> {
        if !matches!(self.mode, Mode::Normal) {
            if let Mode::Help(scroll) = &mut self.mode {
                let limit = crate::ui::help_scroll_limit(self.viewport);
                *scroll = match m.kind {
                    MouseEventKind::ScrollDown => scroll.saturating_add(3).min(limit),
                    MouseEventKind::ScrollUp => scroll.saturating_sub(3),
                    _ => *scroll,
                };
            }
            return Ok(());
        }
        let position = ratatui::layout::Position::new(m.column, m.row);
        if m.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some((_, code)) = self
                .command_hits
                .iter()
                .find(|(rect, _)| rect.contains(position))
        {
            return self.key(KeyEvent::new(*code, KeyModifiers::NONE));
        }
        if self.tab_area.contains(position) {
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((_, action)) =
                        self.tab_hits.iter().find(|(r, _)| r.contains(position))
                    {
                        match *action {
                            TabAction::Select(index) => self.select_tab(index)?,
                            TabAction::Close(index) => self.close_tab(index)?,
                            TabAction::Previous => self.cycle_tab(-1)?,
                            TabAction::Next => self.cycle_tab(1)?,
                        }
                    }
                }
                MouseEventKind::Down(MouseButton::Middle) => {
                    if let Some((_, TabAction::Select(index) | TabAction::Close(index))) =
                        self.tab_hits.iter().find(|(r, _)| r.contains(position))
                    {
                        self.close_tab(*index)?;
                    }
                }
                MouseEventKind::ScrollDown => self.cycle_tab(1)?,
                MouseEventKind::ScrollUp => self.cycle_tab(-1)?,
                _ => {}
            }
            return Ok(());
        }
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(bar) = self.post_scrollbar
                    && bar.rect.contains(position)
                {
                    self.focus = Focus::Posts;
                    let on_thumb = m.row >= bar.thumb_top && m.row < bar.thumb_top + bar.thumb_len;
                    let grab = if on_thumb {
                        (m.row - bar.thumb_top) as i32
                    } else {
                        (bar.thumb_len / 2) as i32
                    };
                    self.scrollbar_drag = Some((grab, m.row));
                    if on_thumb {
                        let scroll = self.current().map(|t| t.scroll).unwrap_or(0);
                        self.scroll_posts_to(scroll)?;
                    } else {
                        self.drag_post_scrollbar(m.row, grab)?;
                    }
                    return Ok(());
                }

                if let Some((_, focus)) =
                    self.refresh_hits.iter().find(|(r, _)| r.contains(position))
                {
                    self.focus = *focus;
                    self.key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))?;
                    return Ok(());
                }

                if let Some(i) = self.dividers.iter().position(|r| r.contains(position)) {
                    let divider = [Divider::Boards, Divider::Threads, Divider::Tabs][i];
                    let original = match divider {
                        Divider::Boards => self.areas[0].width,
                        Divider::Threads => {
                            if self.config.layout == "two" {
                                self.areas[1].height
                            } else {
                                self.areas[1].width
                            }
                        }
                        Divider::Tabs => self.dividers[2].y - self.viewport.y + 1,
                    };
                    self.drag = Some(Drag {
                        divider,
                        column: m.column,
                        row: m.row,
                        original,
                    });
                    return Ok(());
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((grab, last_row)) = self.scrollbar_drag {
                    if m.row != last_row {
                        self.drag_post_scrollbar(m.row, grab)?;
                        self.scrollbar_drag = Some((grab, m.row));
                    }
                    return Ok(());
                }

                if let Some((_, moved)) = &mut self.board_drag {
                    *moved = true;
                    return Ok(());
                }

                if let Some(drag) = &self.drag {
                    let delta = match drag.divider {
                        Divider::Threads if self.config.layout == "two" => {
                            m.row as i32 - drag.row as i32
                        }
                        Divider::Boards | Divider::Threads => m.column as i32 - drag.column as i32,
                        Divider::Tabs => m.row as i32 - drag.row as i32,
                    };
                    let requested = (drag.original as i32 + delta).max(0) as u16;
                    match drag.divider {
                        Divider::Boards => {
                            self.config.board_width =
                                requested.clamp(14, self.viewport.width.saturating_sub(46).max(14))
                        }
                        Divider::Threads => {
                            if self.config.layout == "two" {
                                self.config.thread_height = requested.clamp(
                                    4,
                                    (self.areas[1].height + self.areas[2].height)
                                        .saturating_sub(4)
                                        .max(4),
                                );
                            } else {
                                self.config.thread_width = requested.clamp(
                                    18,
                                    self.viewport
                                        .width
                                        .saturating_sub(self.areas[0].width + 28)
                                        .max(18),
                                );
                            }
                        }
                        Divider::Tabs => {
                            self.config.header_height =
                                requested.clamp(3, self.viewport.height.saturating_sub(12).max(3))
                        }
                    }
                    self.status = "境界をドラッグしてサイズ調整 / マウスを離すと保存".into();
                    return Ok(());
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some((grab, last_row)) = self.scrollbar_drag.take() {
                    if m.row != last_row {
                        self.drag_post_scrollbar(m.row, grab)?;
                    }
                    return Ok(());
                }

                if let Some((source, moved)) = self.board_drag.take() {
                    if moved {
                        if let Some((_, row)) =
                            self.board_hits.iter().find(|(r, _)| r.contains(position))
                            && let Some(BoardRow::Board(target, true)) = self.board_tree().get(*row)
                        {
                            self.reorder_favorite(source, *target)?;
                        }
                    } else {
                        self.activate_board()?;
                    }
                    return Ok(());
                }

                if self.drag.take().is_some() {
                    self.store.save_config(&self.config)?;
                    self.status = "表示サイズを保存しました".into();
                    return Ok(());
                }
            }
            _ => {}
        }
        if m.kind == MouseEventKind::Down(MouseButton::Right)
            && let Some((_, row)) = self.board_hits.iter().find(|(r, _)| r.contains(position))
        {
            self.board_cursor = *row;
            self.focus = Focus::Boards;
            self.favorite()?;
            return Ok(());
        }
        if self.drag.is_none()
            && matches!(
                m.kind,
                MouseEventKind::Down(MouseButton::Left)
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollUp
            )
        {
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                if let Some((_, sort)) = self.sort_hits.iter().find(|(r, _)| r.contains(position)) {
                    let sort = *sort;
                    self.focus = Focus::Threads;
                    self.sort_threads(sort);
                    return Ok(());
                }
                if let Some((_, selected)) =
                    self.board_hits.iter().find(|(r, _)| r.contains(position))
                {
                    self.board_cursor = *selected;
                    self.focus = Focus::Boards;
                    match self.board_tree().get(self.board_cursor).cloned() {
                        Some(BoardRow::Category(..)) => self.activate_board()?,
                        Some(BoardRow::Board(i, _)) => {
                            self.board_sel = self
                                .board_indices()
                                .iter()
                                .position(|&n| n == i)
                                .unwrap_or(0);
                            self.board_drag = Some((i, false));
                        }
                        None => {}
                    }
                    return Ok(());
                }
                if let Some((_, action)) =
                    self.post_actions.iter().find(|(r, _)| r.contains(position))
                {
                    let action = *action;
                    self.focus = Focus::Posts;
                    self.key(KeyEvent::new(KeyCode::Char(action), KeyModifiers::NONE))?;
                    return Ok(());
                }
                if let Some((_, selected)) =
                    self.thread_hits.iter().find(|(r, _)| r.contains(position))
                {
                    self.thread_sel = *selected;
                    self.focus = Focus::Threads;
                    if let Some(&i) = self.thread_indices().get(self.thread_sel) {
                        self.open_thread(self.threads[i].clone())?;
                    }
                    return Ok(());
                }
            }
            if !self.areas.iter().any(|r| r.contains(position)) {
                return Ok(());
            }

            for (i, area) in self.areas.iter().enumerate() {
                if area.contains(position) {
                    self.focus = [Focus::Boards, Focus::Threads, Focus::Posts][i];
                    break;
                }
            }
            match m.kind {
                MouseEventKind::ScrollDown if self.focus == Focus::Posts => {
                    let scroll = self.current().map(|t| t.scroll).unwrap_or(0);
                    self.scroll_posts_to(scroll.saturating_add(3))?;
                }
                MouseEventKind::ScrollUp if self.focus == Focus::Posts => {
                    let scroll = self.current().map(|t| t.scroll).unwrap_or(0);
                    self.scroll_posts_to(scroll.saturating_sub(3))?;
                }
                MouseEventKind::ScrollDown => self.move_selection(1)?,
                MouseEventKind::ScrollUp => self.move_selection(-1)?,
                _ => {}
            }
        }
        Ok(())
    }
    pub fn current_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.tab)
    }
    pub fn board_indices(&self) -> Vec<usize> {
        let q = self.board_filter.to_lowercase();
        let mut ids: Vec<_> = self
            .boards
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                format!("{} {}", b.name, b.category)
                    .to_lowercase()
                    .contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        ids.sort_by_key(|&i| !self.board_favorites.contains(&self.boards[i].url));
        ids
    }
    pub fn board_tree(&self) -> Vec<BoardRow> {
        let ids = self.board_indices();
        let mut rows = vec![];
        let favorites: Vec<_> = self
            .board_favorites
            .iter()
            .filter_map(|url| ids.iter().copied().find(|&i| self.boards[i].url == *url))
            .collect();
        let mut groups: Vec<(String, String, Vec<usize>)> =
            vec![("favorites".into(), "お気に入り".into(), favorites)];
        for i in ids {
            let name = self.boards[i].category.clone();
            let key = format!("category:{name}");
            if let Some(g) = groups.iter_mut().find(|g| g.0 == key) {
                g.2.push(i);
            } else {
                groups.push((key, name, vec![i]));
            }
        }
        for (key, name, boards) in groups {
            rows.push(BoardRow::Category(key.clone(), name, boards.len()));
            if self.expanded.contains(&key) || !self.board_filter.is_empty() {
                for i in boards {
                    rows.push(BoardRow::Board(i, key == "favorites"));
                }
            }
        }
        rows
    }
    pub fn activate_board(&mut self) -> Result<()> {
        match self.board_tree().get(self.board_cursor).cloned() {
            Some(BoardRow::Category(key, _, _)) => {
                if !self.expanded.remove(&key) {
                    self.expanded.insert(key);
                }
                self.store.set("board-expanded", &self.expanded)?;
            }
            Some(BoardRow::Board(i, _)) => {
                self.board_sel = self
                    .board_indices()
                    .iter()
                    .position(|&n| n == i)
                    .unwrap_or(0);
                self.load_board()?;
            }
            None => {}
        }
        Ok(())
    }
    fn reorder_favorite(&mut self, source: usize, target: usize) -> Result<()> {
        let url = self.boards[source].url.clone();
        let target = self.boards[target].url.clone();
        if let (Some(from), Some(to)) = (
            self.board_favorites.iter().position(|u| u == &url),
            self.board_favorites.iter().position(|u| u == &target),
        ) {
            let item = self.board_favorites.remove(from);
            self.board_favorites.insert(to, item);
            self.store.set("board_favorites", &self.board_favorites)?;
            self.board_cursor = self
                .board_tree()
                .iter()
                .position(|r| matches!(r,BoardRow::Board(i,true) if *i==source))
                .unwrap_or(0);
            self.status = "お気に入りの順序を保存しました".into();
        }
        Ok(())
    }
    pub fn thread_indices(&self) -> Vec<usize> {
        let q = self.thread_filter.to_lowercase();
        let mut ids: Vec<usize> = self
            .threads
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.title.to_lowercase().contains(&q)
                    && !self.rules.iter().any(|r| r.matches("title", &t.title))
            })
            .map(|(i, _)| i)
            .collect();
        if let Some(sort) = self.thread_sort {
            ids.sort_by(|&a, &b| {
                let a = &self.threads[a];
                let b = &self.threads[b];
                let order = match sort {
                    ThreadSort::Count => a.count.cmp(&b.count),
                    ThreadSort::Unread => a
                        .count
                        .saturating_sub(self.states.get(&a.url()).map(|s| s.read).unwrap_or(0))
                        .cmp(&b.count.saturating_sub(
                            self.states.get(&b.url()).map(|s| s.read).unwrap_or(0),
                        )),
                    ThreadSort::Momentum => a.momentum().total_cmp(&b.momentum()),
                    ThreadSort::Created => a.created_timestamp().cmp(&b.created_timestamp()),
                };
                if self.sort_desc {
                    order.reverse()
                } else {
                    order
                }
            });
        }
        ids
    }
    pub fn sort_threads(&mut self, sort: ThreadSort) {
        self.sort_desc = if self.thread_sort == Some(sort) {
            !self.sort_desc
        } else {
            true
        };
        self.thread_sort = Some(sort);
        self.thread_sel = 0;
        self.status = format!(
            "{}の{}順 / 同じ見出し・キーで反転",
            sort.label(),
            if self.sort_desc { "降順" } else { "昇順" }
        );
    }

    pub fn post_indices(&self) -> Vec<usize> {
        let Some(tab) = self.current() else {
            return vec![];
        };
        let hidden = hidden_posts(&tab.data.posts, &self.rules, self.config.ng_chain);
        let q = self.post_filter.to_lowercase();
        tab.data
            .posts
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                !hidden[*i]
                    && (q.is_empty()
                        || format!("{} {} {}", p.name, p.id, p.body)
                            .to_lowercase()
                            .contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }
    fn request(&mut self, key: String, req: Request) {
        if !self.offline && self.busy.insert(key) && self.network.tx.send(req).is_err() {
            self.status = "通信ワーカーが停止しました".into();
            self.busy.clear();
        }
    }
    pub fn request_menu(&mut self) {
        self.request("menu".into(), Request::Menu(self.config.menu_url.clone()));
    }
    pub fn load_board(&mut self) -> Result<()> {
        let ids = self.board_indices();
        let Some(&i) = ids.get(self.board_sel) else {
            return Ok(());
        };
        let board = self.boards[i].clone();
        self.view = View::Board;
        self.thread_filter.clear();
        self.thread_sel = 0;
        self.threads = self
            .store
            .get(&format!("subjects:{}", board.url))?
            .unwrap_or_default();
        self.request(format!("subjects:{}", board.url), Request::Subjects(board));
        self.focus = Focus::Threads;
        if self.offline && self.threads.is_empty() {
            self.status = "この板の保存済みスレ一覧はありません。v でログへ".into();
        }
        Ok(())
    }
    pub fn open_input(&mut self, input: &str) -> Result<()> {
        match parse_url(input)? {
            Target::Board(board) => {
                self.select_board(&board)?;
                self.load_board()?;
            }
            Target::Thread(thread) => {
                self.select_board(&thread.board)?;
                self.load_board()?;
                self.open_thread(thread)?;
            }
        }
        Ok(())
    }
    fn select_board(&mut self, board: &Board) -> Result<()> {
        let index = if let Some(index) = self.boards.iter().position(|b| b.url == board.url) {
            index
        } else {
            self.boards.push(board.clone());
            self.store.set("boards", &self.boards)?;
            self.boards.len() - 1
        };
        self.board_filter.clear();
        self.board_sel = self
            .board_indices()
            .iter()
            .position(|&i| i == index)
            .unwrap_or(0);
        self.expanded
            .insert(format!("category:{}", self.boards[index].category));
        self.board_cursor = self
            .board_tree()
            .iter()
            .position(|row| matches!(row, BoardRow::Board(i, _) if *i == index))
            .unwrap_or(0);
        self.store.set("board-expanded", &self.expanded)?;
        Ok(())
    }
    pub fn select_tab(&mut self, index: usize) -> Result<()> {
        if index >= self.tabs.len() {
            return Ok(());
        }
        self.tab = index;
        self.requested_thread = Some(self.tabs[index].data.thread.url());
        self.focus = Focus::Posts;
        self.post_filter.clear();
        self.mark_read()?;
        self.save_session()
    }
    fn cycle_tab(&mut self, step: isize) -> Result<()> {
        if !self.tabs.is_empty() {
            self.select_tab(
                (self.tab as isize + step).rem_euclid(self.tabs.len() as isize) as usize,
            )?;
        }
        Ok(())
    }
    pub fn close_tab(&mut self, index: usize) -> Result<()> {
        if index >= self.tabs.len() {
            return Ok(());
        }
        let closed = self.tabs.remove(index).data.thread.url();
        self.pending_open.remove(&closed);
        let active_closed = index == self.tab;
        if index < self.tab {
            self.tab -= 1;
        }
        self.tab = self.tab.min(self.tabs.len().saturating_sub(1));
        if active_closed {
            self.post_filter.clear();
            if self.tabs.is_empty() {
                self.focus = Focus::Threads;
                self.requested_thread = None;
            } else {
                self.select_tab(self.tab)?;
            }
        }
        self.tab_hits.clear();
        self.save_session()
    }
    pub fn open_thread(&mut self, thread: Thread) -> Result<()> {
        if !self.boards.iter().any(|b| b.url == thread.board.url) {
            self.boards.push(thread.board.clone());
            self.store.set("boards", &self.boards)?;
        }
        let url = thread.url();
        if let Some(i) = self.tabs.iter().position(|t| t.data.thread.url() == url) {
            return self.select_tab(i);
        }
        let cached = self
            .store
            .load_thread(&url)?
            .filter(|d| !d.posts.is_empty());
        let needs_open = cached.is_none();
        if let Some(data) = cached {
            self.open_data(data)?;
        } else if self.offline {
            self.status = "このスレのログは保存されていません".into();
        }
        if !self.offline && needs_open {
            self.requested_thread = Some(url.clone());
            self.pending_open.insert(url.clone());
        }
        self.request(url, Request::Thread(thread));
        Ok(())
    }
    fn open_data(&mut self, data: ThreadData) -> Result<()> {
        self.open_data_with_activation(data, true)
    }
    fn open_data_with_activation(&mut self, data: ThreadData, activate: bool) -> Result<()> {
        let url = data.thread.url();
        if let Some(i) = self.tabs.iter().position(|t| t.data.thread.url() == url) {
            let tab = &mut self.tabs[i];
            if tab.follow {
                tab.selected = data.posts.len().saturating_sub(1);
                tab.jump_bottom = true;
            }
            tab.data = data;
            tab.selected = tab.selected.min(tab.data.posts.len().saturating_sub(1));
            tab.refreshed = Instant::now();
            if activate {
                self.select_tab(i)?;
            } else if i == self.tab
                && self.focus == Focus::Posts
                && matches!(self.mode, Mode::Normal)
            {
                self.mark_read()?;
            }
            return Ok(());
        }
        let state = self.states.entry(url.clone()).or_default();
        let boundary = state.read;
        state.visited = now();
        let selected = state
            .read
            .saturating_sub(1)
            .min(data.posts.len().saturating_sub(1));
        self.tabs.push(Tab {
            data,
            selected,
            scroll: 0,
            boundary,
            auto: false,
            follow: false,
            refreshed: Instant::now(),
            rendered: None,
            jump_bottom: false,
        });
        if activate {
            self.select_tab(self.tabs.len() - 1)?;
        }
        self.save_session()?;
        Ok(())
    }
    pub fn mark_read(&mut self) -> Result<()> {
        if let Some(tab) = self.current().filter(|t| !t.data.posts.is_empty()) {
            let url = tab.data.thread.url();
            let number = tab.selected + 1;
            let state = self.states.entry(url.clone()).or_default();
            state.read = state.read.max(number);
            state.visited = now();
            self.store.state(&url, state)?;
        }
        Ok(())
    }
    fn save_session(&self) -> Result<()> {
        self.store.set(
            "session",
            &self
                .tabs
                .iter()
                .map(|t| t.data.thread.url())
                .collect::<Vec<_>>(),
        )?;
        self.store.set(
            "active-thread",
            &self.current().map(|t| t.data.thread.url()),
        )
    }
    pub fn set_view(&mut self, view: View) -> Result<()> {
        self.view = view;
        let mut threads = self.store.library()?;
        if view == View::Favorites {
            threads.retain(|t| self.states.get(&t.url()).is_some_and(|s| s.favorite));
        }
        if view == View::History {
            threads.retain(|t| self.states.get(&t.url()).is_some_and(|s| s.visited > 0));
            threads.sort_by_key(|t| {
                std::cmp::Reverse(self.states.get(&t.url()).map(|s| s.visited).unwrap_or(0))
            });
        }
        self.threads = threads;
        self.thread_sel = 0;
        self.thread_filter.clear();
        self.focus = Focus::Threads;
        Ok(())
    }
    pub fn tick(&mut self) -> Result<()> {
        while let Ok(response) = self.posting.rx.try_recv() {
            self.store.set("posting-cookies", &response.jar)?;
            match response.result {
                Ok(page) => {
                    if page.success {
                        let mut draft = response.draft;
                        draft.body.clear();
                        self.store.set(&draft.key(), &draft)?;
                        if let Some(thread) = draft.thread.clone() {
                            self.store.set(&format!("draft:{}", thread.url()), "")?;
                            self.request(thread.url(), Request::Thread(thread));
                        }
                        self.request(
                            format!("subjects:{}", draft.board.url),
                            Request::Subjects(draft.board.clone()),
                        );
                        self.compose = Some(draft);
                    }
                    self.status = "サーバー応答を確認してください。自動再送はしません".into();
                    self.mode = Mode::Server(page, 0, 0, false);
                }
                Err(message) => {
                    self.status = message;
                    self.mode = Mode::Draft(Editor::new(
                        self.compose
                            .as_ref()
                            .map(|d| d.body.clone())
                            .unwrap_or_default(),
                    ));
                }
            }
        }
        while let Ok(response) = self.network.rx.try_recv() {
            match response {
                Response::Menu(data) => {
                    self.busy.remove("menu");
                    match data {
                        Ok(mut boards) => {
                            let selected = self
                                .board_indices()
                                .get(self.board_sel)
                                .map(|&i| self.boards[i].url.clone());
                            let cursor = self.board_tree().get(self.board_cursor).cloned();
                            let cursor_url = match &cursor {
                                Some(BoardRow::Board(index, _)) => {
                                    Some(self.boards[*index].url.clone())
                                }
                                _ => None,
                            };
                            for b in &self.boards {
                                if !boards.iter().any(|n| n.url == b.url) {
                                    boards.push(b.clone());
                                }
                            }
                            self.boards = boards;
                            self.store.set("boards", &self.boards)?;
                            let indices = self.board_indices();
                            self.board_sel = selected
                                .and_then(|url| {
                                    indices.iter().position(|&i| self.boards[i].url == url)
                                })
                                .unwrap_or(self.board_sel.min(indices.len().saturating_sub(1)));
                            if let Some(url) = cursor_url {
                                if let Some(index) = self.boards.iter().position(|b| b.url == url) {
                                    if !self.board_tree().iter().any(
                                        |row| matches!(row, BoardRow::Board(i, _) if *i == index),
                                    ) {
                                        self.expanded.insert(format!(
                                            "category:{}",
                                            self.boards[index].category
                                        ));
                                        self.store.set("board-expanded", &self.expanded)?;
                                    }
                                    self.board_cursor = self.board_tree().iter().position(|row| matches!(row, BoardRow::Board(i, _) if *i == index)).unwrap_or(0);
                                }
                            } else if let Some(BoardRow::Category(key, _, _)) = cursor {
                                self.board_cursor = self.board_tree().iter().position(|row| matches!(row, BoardRow::Category(k, _, _) if *k == key)).unwrap_or(0);
                            }
                            self.status = format!("板一覧を更新: {}板", self.boards.len());
                        }
                        Err(e) => {
                            self.status = format!("板一覧: {e} / u で板URLを直接指定できます")
                        }
                    }
                }
                Response::Subjects(board, data) => {
                    self.busy.remove(&format!("subjects:{}", board.url));
                    match data {
                        Ok(threads) => {
                            if let Some(saved) = self.boards.iter_mut().find(|b| b.url == board.url)
                            {
                                saved.name = board.name.clone();
                                self.store.set("boards", &self.boards)?;
                            }

                            self.store
                                .set(&format!("subjects:{}", board.url), &threads)?;
                            let ids = self.board_indices();
                            if self.view == View::Board
                                && ids
                                    .get(self.board_sel)
                                    .is_some_and(|&i| self.boards[i].url == board.url)
                            {
                                let selected = self.selected_thread_url();
                                self.threads = threads;
                                self.restore_thread_selection(selected);
                            }
                            self.status = format!("{} のスレ一覧を更新", board.name);
                        }
                        Err(e) => {
                            self.status = format!("スレ一覧: {e} / 保存済みログは閲覧できます")
                        }
                    }
                }
                Response::Thread(thread, data) => {
                    self.receive_thread(thread, data)?;
                }
            }
        }
        if !self.offline {
            let due: Vec<_> = self
                .tabs
                .iter()
                .filter(|t| {
                    t.auto && t.refreshed.elapsed().as_secs() >= self.config.refresh_seconds.max(10)
                })
                .map(|t| t.data.thread.clone())
                .collect();
            for thread in due {
                if let Some(t) = self
                    .tabs
                    .iter_mut()
                    .find(|t| t.data.thread.url() == thread.url())
                {
                    t.refreshed = Instant::now();
                }
                self.request(thread.url(), Request::Thread(thread));
            }
        }
        Ok(())
    }
    fn selected_thread_url(&self) -> Option<String> {
        self.thread_indices()
            .get(self.thread_sel)
            .map(|&i| self.threads[i].url())
    }
    fn restore_thread_selection(&mut self, selected: Option<String>) {
        self.thread_sel = selected
            .and_then(|url| {
                self.thread_indices()
                    .iter()
                    .position(|&i| self.threads[i].url() == url)
            })
            .unwrap_or(
                self.thread_sel
                    .min(self.thread_indices().len().saturating_sub(1)),
            );
    }
    fn receive_thread(&mut self, thread: Thread, result: Result<ThreadData, String>) -> Result<()> {
        let url = thread.url();
        self.busy.remove(&url);
        let requested = self.pending_open.remove(&url);
        match result {
            Ok(data) => {
                self.store.save_thread(&data)?;
                let selected = self.selected_thread_url();
                for listed in &mut self.threads {
                    if listed.url() == url {
                        *listed = data.thread.clone();
                    }
                }
                self.restore_thread_selection(selected);
                self.status = format!("{} / {}レス取得", data.thread.title, data.posts.len());
                let open = self.tabs.iter().any(|t| t.data.thread.url() == url);
                if open || requested {
                    let activate = requested
                        && self.requested_thread.as_deref() == Some(&url)
                        && matches!(self.mode, Mode::Normal);
                    self.open_data_with_activation(data, activate)?;
                }
            }
            Err(error) => self.status = format!("スレ取得: {error} / 保存済みログは保持しています"),
        }
        Ok(())
    }
    pub fn move_selection(&mut self, amount: isize) -> Result<()> {
        match self.focus {
            Focus::Boards => {
                self.board_cursor = clamp_move(self.board_cursor, amount, self.board_tree().len());
                if let Some(BoardRow::Board(i, _)) = self.board_tree().get(self.board_cursor) {
                    self.board_sel = self
                        .board_indices()
                        .iter()
                        .position(|n| n == i)
                        .unwrap_or(0);
                }
            }
            Focus::Threads => {
                self.thread_sel = clamp_move(self.thread_sel, amount, self.thread_indices().len())
            }
            Focus::Posts => {
                let ids = self.post_indices();
                let selected = self.current().map(|t| t.selected).unwrap_or(0);
                let position = ids.iter().position(|&i| i == selected).unwrap_or(0);
                if let Some(&i) = ids.get(clamp_move(position, amount, ids.len())) {
                    if let Some(t) = self.current_mut() {
                        t.selected = i;
                        t.follow = false;
                    }
                    self.mark_read()?;
                }
            }
        }
        Ok(())
    }
    fn prompt(&mut self, kind: Prompt, initial: String) {
        self.mode = Mode::Input(kind, Editor::new(initial));
    }
    fn favorite(&mut self) -> Result<()> {
        if self.focus == Focus::Boards {
            if let Some(BoardRow::Board(i, _)) = self.board_tree().get(self.board_cursor).cloned() {
                let url = self.boards[i].url.clone();
                if self.board_favorites.contains(&url) {
                    self.board_favorites.retain(|u| u != &url);
                } else {
                    self.board_favorites.push(url);
                }
                self.store.set("board_favorites", &self.board_favorites)?;
                self.expanded.insert("favorites".into());
                self.store.set("board-expanded", &self.expanded)?;
                self.board_cursor = 0;
            }
        } else {
            let thread = if self.focus == Focus::Posts {
                self.current().map(|t| t.data.thread.clone())
            } else {
                self.thread_indices()
                    .get(self.thread_sel)
                    .map(|&i| self.threads[i].clone())
            };
            if let Some(thread) = thread {
                if self.store.load_thread(&thread.url())?.is_none() {
                    self.store.save_thread(&ThreadData {
                        thread: thread.clone(),
                        posts: vec![],
                    })?;
                }
                let state = self.states.entry(thread.url()).or_default();
                state.favorite = !state.favorite;
                self.store.state(&thread.url(), state)?;
                self.status = if state.favorite {
                    "お気に入りに追加"
                } else {
                    "お気に入りから削除"
                }
                .into();
            }
        }
        Ok(())
    }
    fn popup_posts(&mut self, kind: char) {
        let Some(tab) = self.current() else {
            return;
        };
        let Some(p) = tab.data.posts.get(tab.selected) else {
            return;
        };
        let posts = match kind {
            'a' => tab
                .data
                .posts
                .iter()
                .filter(|x| p.refs.contains(&x.number))
                .collect::<Vec<_>>(),
            'i' => tab
                .data
                .posts
                .iter()
                .filter(|x| !p.id.is_empty() && x.id == p.id)
                .collect(),
            _ => tab
                .data
                .posts
                .iter()
                .filter(|x| x.refs.contains(&p.number))
                .collect(),
        };
        let hidden = hidden_posts(&tab.data.posts, &self.rules, self.config.ng_chain);
        let items = posts
            .into_iter()
            .filter(|p| !hidden[p.number - 1])
            .map(|p| PopupItem {
                label: format!("{} {} ID:{}\n{}", p.number, p.name, p.id, p.body),
                post: Some(p.number - 1),
                url: None,
            })
            .collect();
        self.mode = Mode::Popup(
            match kind {
                'a' => "参照レス",
                'i' => "同じID",
                _ => "このレスへの返信",
            }
            .into(),
            items,
            0,
        );
    }
    fn apply_prompt(&mut self, kind: Prompt, input: String) -> Result<()> {
        match kind {
            Prompt::Name | Prompt::Mail | Prompt::Subject => {
                if let Some(d) = &mut self.compose {
                    match kind {
                        Prompt::Name => d.name = input,
                        Prompt::Mail => d.mail = input,
                        _ => d.subject = input,
                    }
                    let body = d.body.clone();
                    self.save_draft(&Editor::new(body.clone()))?;
                    self.mode = Mode::Draft(Editor::new(body));
                }
            }
            Prompt::Search => match self.focus {
                Focus::Boards => {
                    self.board_filter = input;
                    self.board_sel = 0;
                    self.board_cursor = 0;
                }
                Focus::Threads => {
                    self.thread_filter = input;
                    self.thread_sel = 0;
                }
                Focus::Posts => {
                    self.post_filter = input;
                    if let Some(&i) = self.post_indices().first() {
                        if let Some(t) = self.current_mut() {
                            t.selected = i;
                            t.scroll = 0;
                        }
                        self.mark_read()?;
                    }
                }
            },
            Prompt::Url => self.open_input(&input)?,
            Prompt::Jump => {
                let n = input.parse::<usize>()?;
                if let Some(t) = self.current_mut() {
                    if n == 0 || n > t.data.posts.len() {
                        bail!("存在するレス番号を指定してください");
                    }
                    t.selected = n - 1;
                    t.scroll = 0;
                    t.follow = false;
                }
                self.post_filter.clear();
                self.mark_read()?;
            }
            Prompt::Ng => {
                if input.trim() == "clear" {
                    self.rules.clear();
                } else {
                    let (field, pattern) = input.split_once(':').ok_or_else(|| {
                        anyhow::anyhow!("id:文字列 / name:文字列 / body:文字列 / title:文字列")
                    })?;
                    let (field, expires) = if let Some((f, minutes)) = field.split_once('@') {
                        (f, Some(now() + minutes.parse::<u64>()?.saturating_mul(60)))
                    } else {
                        (field, None)
                    };
                    if !["id", "name", "body", "title"].contains(&field) || pattern.is_empty() {
                        bail!("NGの対象または文字列が不正です");
                    }
                    let regex =
                        pattern.starts_with('/') && pattern.ends_with('/') && pattern.len() > 1;
                    let pattern = if regex {
                        &pattern[1..pattern.len() - 1]
                    } else {
                        pattern
                    };
                    if regex {
                        regex::Regex::new(pattern)?;
                    }
                    self.rules.push(NgRule {
                        field: field.into(),
                        pattern: pattern.into(),
                        regex,
                        expires,
                    });
                }
                self.store.set("ng", &self.rules)?;
                self.status = format!("NG設定を保存: {}件", self.rules.len());
            }
            Prompt::Export => {
                let tab = self
                    .current()
                    .ok_or_else(|| anyhow::anyhow!("スレを開いてください"))?;
                let path = std::path::PathBuf::from(input);
                if path.exists() {
                    bail!("同名のファイルが存在します。別の名前を指定してください");
                }
                let text = if path.extension().is_some_and(|e| e == "json") {
                    serde_json::to_string_pretty(&tab.data)?
                } else {
                    format!(
                        "{}\n{}\n\n{}",
                        tab.data.thread.title,
                        tab.data.thread.url(),
                        tab.data
                            .posts
                            .iter()
                            .map(|p| format!("{} {} {}\n{}\n", p.number, p.name, p.date, p.body))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                };
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)?;
                file.write_all(text.as_bytes())?;
                self.status = format!("書き出しました: {}", path.display());
            }
            Prompt::Refresh => {
                self.config.refresh_seconds = input.parse::<u64>()?.max(10);
                self.store.save_config(&self.config)?;
                self.status = format!("自動更新間隔: {}秒", self.config.refresh_seconds);
            }
            Prompt::Logs => {
                self.threads = self.store.search_logs(&input)?;
                self.view = View::Logs;
                self.thread_filter.clear();
                self.thread_sel = 0;
                self.focus = Focus::Threads;
                self.status = format!(
                    "保存済みログを全文検索: {} / {}件",
                    input,
                    self.threads.len()
                );
            }
        }
        Ok(())
    }
    fn draft_key(&self) -> Option<String> {
        self.current()
            .map(|t| format!("draft:{}", t.data.thread.url()))
    }
    fn save_draft(&mut self, editor: &Editor) -> Result<()> {
        if let Some(d) = &mut self.compose {
            d.body = editor.text.clone();
            self.store.set(&d.key(), d)?;
            self.store.set(
                "posting-profile",
                &crate::posting::Profile {
                    name: d.name.clone(),
                    mail: d.mail.clone(),
                },
            )?;
            if let Some(t) = &d.thread {
                self.store
                    .set(&format!("draft:{}", t.url()), &editor.text)?;
            }
        } else if let Some(key) = self.draft_key() {
            self.store.set(&key, &editor.text)?;
        }
        Ok(())
    }
    fn begin_post(&mut self, board: Board, thread: Option<Thread>) -> Result<()> {
        let profile = self
            .store
            .get::<crate::posting::Profile>("posting-profile")?
            .unwrap_or_default();
        let mut d = crate::posting::Draft {
            board,
            thread,
            subject: String::new(),
            name: profile.name,
            mail: profile.mail,
            body: String::new(),
        };
        if let Some(saved) = self.store.get::<crate::posting::Draft>(&d.key())? {
            d = saved;
        } else if let Some(t) = &d.thread {
            d.body = self
                .store
                .get::<String>(&format!("draft:{}", t.url()))?
                .unwrap_or_default();
        }
        self.mode = Mode::Draft(Editor::new(d.body.clone()));
        self.compose = Some(d);
        Ok(())
    }
    fn submit_post(&mut self, e: &Editor) -> Result<()> {
        self.save_draft(e)?;
        if self.demo || self.offline {
            bail!("デモ・オフラインでは送信しません。下書きは保存済みです");
        }
        let d = self
            .compose
            .clone()
            .ok_or_else(|| anyhow::anyhow!("投稿先がありません"))?;
        if d.thread.is_none() && d.subject.trim().is_empty() {
            bail!("F4でスレタイトルを入力してください");
        }
        if d.board.site() != Site::FiveCh {
            bail!(
                "エッヂ・したらばは現在閲覧対応です。認証・投稿処理は未対応で、送信していません。下書きは保存済みです"
            );
        }
        let jar = self
            .store
            .get::<crate::posting::Jar>("posting-cookies")?
            .unwrap_or_default();
        self.posting
            .tx
            .send(crate::posting::Request::Start(d, jar))?;
        self.mode = Mode::Sending;
        self.status = "サーバー応答を待っています…".into();
        Ok(())
    }
    pub fn key(&mut self, mut key: KeyEvent) -> Result<()> {
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Mode::Draft(ref e) | Mode::Confirm(ref e) = self.mode {
                let text = e.text.clone();
                self.save_draft(&Editor::new(text))?;
            }
            self.running = false;
            return Ok(());
        }
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        match mode {
            Mode::Help(mut scroll) => {
                let limit = crate::ui::help_scroll_limit(self.viewport);
                scroll = scroll.min(limit);
                match key.code {
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => {
                        scroll = scroll.saturating_add(1).min(limit)
                    }
                    KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        scroll = scroll.saturating_add(10).min(limit)
                    }
                    KeyCode::PageUp => scroll = scroll.saturating_sub(10),
                    KeyCode::Home => scroll = 0,
                    KeyCode::End => scroll = limit,
                    _ => {}
                }
                self.mode = Mode::Help(scroll);
                return Ok(());
            }
            Mode::Input(kind, mut e) => {
                match key.code {
                    KeyCode::Esc => {
                        if matches!(kind, Prompt::Name | Prompt::Mail | Prompt::Subject) {
                            self.mode = Mode::Draft(Editor::new(
                                self.compose
                                    .as_ref()
                                    .map(|d| d.body.clone())
                                    .unwrap_or_default(),
                            ));
                        }
                    }
                    KeyCode::Enter => {
                        if let Err(error) = self.apply_prompt(kind, e.text.clone()) {
                            self.mode = Mode::Input(kind, e);
                            return Err(error);
                        }
                    }
                    _ => {
                        e.key(key, false);
                        self.mode = Mode::Input(kind, e);
                    }
                }
                return Ok(());
            }
            Mode::Popup(title, items, mut selected) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {}
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = clamp_move(selected, 1, items.len());
                        self.mode = Mode::Popup(title, items, selected);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = clamp_move(selected, -1, items.len());
                        self.mode = Mode::Popup(title, items, selected);
                    }
                    KeyCode::Enter => {
                        if let Some(item) = items.get(selected) {
                            if let Some(i) = item.post {
                                self.post_filter.clear();
                                if let Some(t) = self.current_mut() {
                                    t.selected = i;
                                    t.scroll = 0;
                                    t.follow = false;
                                }
                                self.mark_read()?;
                            }
                            if let Some(url) = &item.url {
                                net::open_url(url)?;
                            }
                        }
                    }
                    _ => self.mode = Mode::Popup(title, items, selected),
                }
                return Ok(());
            }
            Mode::Draft(mut e) => {
                if key.code == KeyCode::Esc {
                    self.save_draft(&e)?;
                    self.status = "下書きを保存しました".into();
                } else if (key.code == KeyCode::Enter
                    && key.modifiers.contains(KeyModifiers::CONTROL))
                    || key.code == KeyCode::F(6)
                {
                    self.save_draft(&e)?;
                    if e.text.trim().is_empty() {
                        self.mode = Mode::Draft(e);
                        bail!("本文を入力してください");
                    }
                    self.mode = Mode::Confirm(e);
                } else if matches!(key.code, KeyCode::F(2) | KeyCode::F(3) | KeyCode::F(4))
                    && self.compose.is_some()
                {
                    self.save_draft(&e)?;
                    let d = self.compose.as_ref().unwrap();
                    let (kind, text) = match key.code {
                        KeyCode::F(2) => (Prompt::Name, d.name.clone()),
                        KeyCode::F(3) => (Prompt::Mail, d.mail.clone()),
                        _ => (Prompt::Subject, d.subject.clone()),
                    };
                    self.prompt(kind, text);
                } else {
                    e.key(key, true);
                    self.save_draft(&e)?;
                    self.mode = Mode::Draft(e);
                }
                return Ok(());
            }
            Mode::Confirm(e) => {
                if key.code == KeyCode::Enter {
                    if let Err(error) = self.submit_post(&e) {
                        self.mode = Mode::Draft(e);
                        return Err(error);
                    }
                } else {
                    self.mode = Mode::Draft(e);
                }
                return Ok(());
            }
            Mode::Sending => {
                self.mode = Mode::Sending;
                return Ok(());
            }
            Mode::Server(mut page, mut scroll, mut selected, mut raw) => {
                let choices: Vec<(usize, usize)> = page
                    .forms
                    .iter()
                    .enumerate()
                    .flat_map(|(i, f)| (0..f.choices.len()).map(move |j| (i, j)))
                    .collect();
                match key.code {
                    KeyCode::Esc => {
                        if !page.success {
                            self.mode = Mode::Draft(Editor::new(
                                self.compose
                                    .as_ref()
                                    .map(|d| d.body.clone())
                                    .unwrap_or_default(),
                            ));
                        }
                        return Ok(());
                    }
                    KeyCode::Down | KeyCode::Char('j') => scroll = scroll.saturating_add(1),
                    KeyCode::Up | KeyCode::Char('k') => scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown => scroll = scroll.saturating_add(10),
                    KeyCode::PageUp => scroll = scroll.saturating_sub(10),
                    KeyCode::Char('v') => {
                        raw = !raw;
                        scroll = 0;
                    }
                    KeyCode::Tab => {
                        selected = if choices.is_empty() {
                            0
                        } else {
                            (selected + 1) % choices.len()
                        }
                    }
                    KeyCode::Char(c @ '1'..='9') => {
                        if let Some(&(i, _)) = choices.get(selected)
                            && let Some(check) =
                                page.forms[i].checks.get_mut((c as u8 - b'1') as usize)
                        {
                            check.checked = !check.checked;
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(&(i, j)) = choices.get(selected) {
                            let d = self
                                .compose
                                .clone()
                                .ok_or_else(|| anyhow::anyhow!("下書きがありません"))?;
                            let jar = self
                                .store
                                .get::<crate::posting::Jar>("posting-cookies")?
                                .unwrap_or_default();
                            self.posting.tx.send(crate::posting::Request::Confirm(
                                d,
                                page.forms[i].clone(),
                                j,
                                jar,
                            ))?;
                            self.mode = Mode::Sending;
                        } else if !page.success {
                            self.mode = Mode::Draft(Editor::new(
                                self.compose
                                    .as_ref()
                                    .map(|d| d.body.clone())
                                    .unwrap_or_default(),
                            ));
                        }
                        return Ok(());
                    }
                    _ => {}
                }
                self.mode = Mode::Server(page, scroll, selected, raw);
                return Ok(());
            }
            Mode::Normal => {}
        }
        if self.focus == Focus::Boards && key.code == KeyCode::Left {
            let key = match self.board_tree().get(self.board_cursor) {
                Some(BoardRow::Category(key, _, _)) => Some(key.clone()),
                Some(BoardRow::Board(i, fav)) => Some(if *fav {
                    "favorites".into()
                } else {
                    format!("category:{}", self.boards[*i].category)
                }),
                None => None,
            };
            if let Some(key) = key {
                self.expanded.remove(&key);
                self.board_cursor = self
                    .board_tree()
                    .iter()
                    .position(|r| matches!(r,BoardRow::Category(k,_,_) if *k==key))
                    .unwrap_or(0);
                self.store.set("board-expanded", &self.expanded)?;
            }
            return Ok(());
        }
        if self.focus == Focus::Boards
            && key.code == KeyCode::Right
            && let Some(BoardRow::Category(k, _, _)) = self.board_tree().get(self.board_cursor)
            && self.expanded.contains(k)
        {
            return Ok(());
        }
        if self.focus == Focus::Boards
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Up | KeyCode::Down)
        {
            if let Some(BoardRow::Board(source, true)) =
                self.board_tree().get(self.board_cursor).cloned()
                && let Some(from) = self
                    .board_favorites
                    .iter()
                    .position(|u| *u == self.boards[source].url)
            {
                let to = clamp_move(
                    from,
                    if key.code == KeyCode::Up { -1 } else { 1 },
                    self.board_favorites.len(),
                );
                if let Some(target) = self
                    .boards
                    .iter()
                    .position(|b| b.url == self.board_favorites[to])
                {
                    self.reorder_favorite(source, target)?;
                }
            }
            return Ok(());
        }
        if let KeyCode::Char(c) = key.code
            && (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
            && let Some(mapped) = self.config.keys.get(&c.to_string())
            && mapped.chars().count() == 1
        {
            key.code = KeyCode::Char(mapped.chars().next().unwrap());
        }
        match key.code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Char('?') | KeyCode::F(1) => self.mode = Mode::Help(0),
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Boards => Focus::Threads,
                    Focus::Threads => Focus::Posts,
                    Focus::Posts => Focus::Boards,
                }
            }
            KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Boards => Focus::Posts,
                    Focus::Threads => Focus::Boards,
                    Focus::Posts => Focus::Threads,
                }
            }
            KeyCode::Char('1') => self.focus = Focus::Boards,
            KeyCode::Char('2') => self.focus = Focus::Threads,
            KeyCode::Char('3') => self.focus = Focus::Posts,
            KeyCode::Char('j') | KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if let Some(t) = self.current_mut() {
                        t.scroll = t.scroll.saturating_add(1);
                        t.follow = false;
                    }
                } else {
                    self.move_selection(1)?;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if let Some(t) = self.current_mut() {
                        t.scroll = t.scroll.saturating_sub(1);
                        t.follow = false;
                    }
                } else {
                    self.move_selection(-1)?;
                }
            }
            KeyCode::PageDown | KeyCode::Char(' ') => self.move_selection(5)?,
            KeyCode::PageUp => self.move_selection(-5)?,
            KeyCode::Home => self.move_selection(-1000000)?,
            KeyCode::End | KeyCode::Char('G') if self.focus == Focus::Posts => {
                self.latest_post()?
            }
            KeyCode::End | KeyCode::Char('G') => self.move_selection(1000000)?,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => match self.focus {
                Focus::Boards => self.activate_board()?,
                Focus::Threads => {
                    if let Some(&i) = self.thread_indices().get(self.thread_sel) {
                        self.open_thread(self.threads[i].clone())?;
                    }
                }
                Focus::Posts => self.popup_posts('a'),
            },
            KeyCode::Left | KeyCode::Char('h') => {
                self.focus = match self.focus {
                    Focus::Posts => Focus::Threads,
                    _ => Focus::Boards,
                }
            }
            KeyCode::Esc => {
                self.board_filter.clear();
                self.thread_filter.clear();
                self.post_filter.clear();
            }
            KeyCode::Char('/') => {
                let text = match self.focus {
                    Focus::Boards => &self.board_filter,
                    Focus::Threads => &self.thread_filter,
                    Focus::Posts => &self.post_filter,
                }
                .clone();
                self.prompt(Prompt::Search, text);
            }
            KeyCode::Char('u') => self.prompt(Prompt::Url, String::new()),
            KeyCode::Char('g') => self.prompt(Prompt::Jump, String::new()),
            KeyCode::Char('L') => self.prompt(Prompt::Logs, String::new()),
            KeyCode::Char('r') => {
                match self.focus {
                    Focus::Boards => self.request_menu(),
                    Focus::Threads => {
                        if self.view == View::Board {
                            if let Some(&index) = self.board_indices().get(self.board_sel) {
                                let board = self.boards[index].clone();
                                if self.offline {
                                    let selected = self.selected_thread_url();
                                    if let Some(saved) =
                                        self.store.get(&format!("subjects:{}", board.url))?
                                    {
                                        self.threads = saved;
                                        self.restore_thread_selection(selected);
                                    }
                                    self.status = "オフラインでは更新できません。保存済み一覧を表示しています".into();
                                } else {
                                    self.request(
                                        format!("subjects:{}", board.url),
                                        Request::Subjects(board),
                                    );
                                }
                            }
                        } else {
                            self.set_view(self.view)?;
                        }
                    }
                    Focus::Posts => {
                        if let Some(t) = self.current() {
                            let t = t.data.thread.clone();
                            self.request(t.url(), Request::Thread(t));
                        }
                    }
                }
            }
            KeyCode::Char('f') => self.favorite()?,
            KeyCode::Char('v') => self.set_view(match self.view {
                View::Board => View::Favorites,
                View::Favorites => View::History,
                View::History => View::Logs,
                View::Logs => {
                    self.load_board()?;
                    return Ok(());
                }
            })?,
            KeyCode::Char('s') => self.sort_threads(ThreadSort::Momentum),
            KeyCode::Char('c') => self.sort_threads(ThreadSort::Count),
            KeyCode::Char('U') => self.sort_threads(ThreadSort::Unread),
            KeyCode::Char('d') => self.sort_threads(ThreadSort::Created),
            KeyCode::Char('n') => {
                if let Some(t) = self.current_mut() {
                    t.selected = t.boundary.min(t.data.posts.len().saturating_sub(1));
                    t.scroll = 0;
                    t.follow = false;
                }
                self.post_filter.clear();
                self.focus = Focus::Posts;
                self.mark_read()?;
            }
            KeyCode::Char('a') | KeyCode::Char('i') | KeyCode::Char('t') => {
                if let KeyCode::Char(c) = key.code {
                    self.popup_posts(c);
                }
            }
            KeyCode::Char('o') => {
                if let Some(t) = self.current()
                    && let Some(p) = t.data.posts.get(t.selected)
                {
                    let items = crate::parse::links(&p.body)
                        .into_iter()
                        .map(|url| PopupItem {
                            label: url.clone(),
                            post: None,
                            url: Some(url),
                        })
                        .collect();
                    self.mode = Mode::Popup("リンク / Enterでブラウザ表示".into(), items, 0);
                }
            }
            KeyCode::Char('b') => {
                if let Some(t) = self.current() {
                    net::open_url(&t.data.thread.url())?;
                }
            }
            KeyCode::Char('B') => {
                let board = match self.focus {
                    Focus::Posts => self.current().map(|t| t.data.thread.board.clone()),
                    Focus::Threads => self
                        .thread_indices()
                        .get(self.thread_sel)
                        .map(|&i| self.threads[i].board.clone())
                        .or_else(|| {
                            self.board_indices()
                                .get(self.board_sel)
                                .map(|&i| self.boards[i].clone())
                        }),
                    Focus::Boards => match self.board_tree().get(self.board_cursor) {
                        Some(BoardRow::Board(i, _)) => Some(self.boards[*i].clone()),
                        _ => None,
                    },
                };
                if let Some(board) = board {
                    self.begin_post(board, None)?;
                    self.prompt(
                        Prompt::Subject,
                        self.compose.as_ref().unwrap().subject.clone(),
                    );
                }
            }
            KeyCode::Char('N') => {
                let initial = self
                    .current()
                    .and_then(|t| t.data.posts.get(t.selected))
                    .map(|p| format!("id:{}", p.id))
                    .unwrap_or_else(|| "body:".into());
                self.prompt(Prompt::Ng, initial);
            }
            KeyCode::Char('w') | KeyCode::Char('R') => {
                if let Some(t) = self.current() {
                    let thread = t.data.thread.clone();
                    let n = t.selected + 1;
                    self.begin_post(thread.board.clone(), Some(thread))?;
                    if key.code == KeyCode::Char('R') {
                        let mut body = self.compose.as_ref().unwrap().body.clone();
                        body.push_str(&format!("\n>>{n}\n"));
                        self.save_draft(&Editor::new(body.clone()))?;
                        self.mode = Mode::Draft(Editor::new(body));
                    }
                }
            }
            KeyCode::Char('e') => self.prompt(
                Prompt::Export,
                self.store
                    .dir
                    .join(format!("thread-{}.txt", now()))
                    .display()
                    .to_string(),
            ),
            KeyCode::Char('A') => {
                if self.offline {
                    self.status = "オフラインでは自動更新できません".into();
                } else if let Some(t) = self.current_mut() {
                    t.auto = !t.auto;
                    t.follow = t.auto;
                    if t.follow {
                        t.selected = t.data.posts.len().saturating_sub(1);
                    }
                    self.status = if t.auto {
                        "自動更新・新着追従ON"
                    } else {
                        "自動更新・新着追従OFF"
                    }
                    .into();
                    self.mark_read()?;
                }
            }
            KeyCode::Char('F') => {
                if let Some(t) = self.current_mut() {
                    t.follow = !t.follow;
                    if t.follow {
                        t.selected = t.data.posts.len().saturating_sub(1);
                    }
                    self.mark_read()?;
                }
            }
            KeyCode::Char('I') => {
                self.prompt(Prompt::Refresh, self.config.refresh_seconds.to_string())
            }
            KeyCode::Char('W') => {
                self.config.wrap = !self.config.wrap;
                self.store.save_config(&self.config)?;
                if let Some(t) = self.current_mut() {
                    t.scroll = 0;
                }
            }
            KeyCode::Char('T') => {
                self.config.theme = if self.config.theme == "dark" {
                    "light"
                } else {
                    "dark"
                }
                .into();
                self.store.save_config(&self.config)?;
            }
            KeyCode::Char('z') => {
                self.config.layout = match self.config.layout.as_str() {
                    "auto" => "three",
                    "three" => "two",
                    "two" => "single",
                    _ => "auto",
                }
                .into();
                self.drag = None;
                self.store.save_config(&self.config)?;
                self.status = format!(
                    "表示: {}",
                    match self.config.layout.as_str() {
                        "three" => "3ペイン",
                        "two" => "2列（スレ・レス上下）",
                        "single" => "1ペイン",
                        _ => "自動",
                    }
                );
            }
            KeyCode::Char('[') => {
                self.cycle_tab(-1)?;
            }
            KeyCode::Char(']') => {
                self.cycle_tab(1)?;
            }
            KeyCode::Char('x') => {
                self.close_tab(self.tab)?;
            }
            KeyCode::Char('p') => {
                if let Some(t) = self.current() {
                    let title = t.data.thread.title.clone();
                    let board = t.data.thread.board.clone();
                    self.open_input(&board.url)?;
                    self.thread_filter = regex::Regex::new(r"[★☆]?\s*\d+\s*$")
                        .unwrap()
                        .replace(&title, "")
                        .trim()
                        .to_string();
                    self.status = "次スレ候補: 同じ板のタイトルで絞り込み".into();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
fn clamp_move(current: usize, amount: isize, len: usize) -> usize {
    current
        .saturating_add_signed(amount)
        .min(len.saturating_sub(1))
}
pub fn run(app: &mut App) -> Result<()> {
    use crossterm::{
        event::{
            DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        },
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    };
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(
                std::io::stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                DisableBracketedPaste
            );
        }
    }
    enable_raw_mode()?;
    let _restore = Restore;
    execute!(
        std::io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |p| {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste
        );
        old(p);
    }));
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    while app.running {
        if let Err(e) = app.tick() {
            app.status = e.to_string();
        }
        terminal.draw(|f| crate::ui::draw(f, app))?;
        if event::poll(Duration::from_millis(100))? {
            let result = match event::read()? {
                Event::Key(key) => app.key(key),
                Event::Paste(text) => {
                    let mode = std::mem::replace(&mut app.mode, Mode::Normal);
                    match mode {
                        Mode::Input(p, mut e) => {
                            e.insert(&crate::parse::clean(&text).replace('\n', " "));
                            app.mode = Mode::Input(p, e);
                        }
                        Mode::Draft(mut e) => {
                            e.insert(&crate::parse::clean(&text));
                            app.save_draft(&e)?;
                            app.mode = Mode::Draft(e);
                        }
                        m => app.mode = m,
                    }
                    Ok(())
                }
                Event::Mouse(m) => app.mouse(m),
                _ => Ok(()),
            };
            if let Err(e) = result {
                app.status = e.to_string();
            }
        }
    }
    app.save_session()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_app(label: &str) -> Result<(std::path::PathBuf, App)> {
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let app = App::new(Store::open(&dir)?, true, true)?;
        Ok((dir, app))
    }
    #[test]
    fn active_tab_and_empty_session_survive_restart() -> Result<()> {
        let (dir, mut app) = test_app("active-tab")?;
        let data = app.current().unwrap().data.clone();
        for index in 1..25 {
            let mut data = data.clone();
            data.thread.key = format!("179129{index:04}");
            app.store.save_thread(&data)?;
            app.open_thread(data.thread)?;
        }
        app.select_tab(23)?;
        let active = app.current().unwrap().data.thread.url();
        drop(app);
        let mut restored = App::new(Store::open(&dir)?, false, true)?;
        assert_eq!(restored.tabs.len(), 25);
        assert_eq!(restored.current().unwrap().data.thread.url(), active);
        while !restored.tabs.is_empty() {
            restored.close_tab(0)?;
        }
        drop(restored);
        let empty = App::new(Store::open(&dir)?, false, true)?;
        assert!(empty.tabs.is_empty());
        drop(empty);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn background_refresh_preserves_focus_unread_and_closed_tabs() -> Result<()> {
        let (dir, mut app) = test_app("background")?;
        let first = app.current().unwrap().data.clone();
        let first_url = first.thread.url();
        let read = app.states[&first_url].read;
        app.open_thread(app.threads[1].clone())?;
        let active = app.current().unwrap().data.thread.url();
        app.focus = Focus::Threads;
        app.thread_filter = "試".into();
        app.mode = Mode::Input(Prompt::Search, Editor::new("入力中".into()));
        app.tabs[0].follow = true;
        let mut update = first;
        let mut post = update.posts.last().unwrap().clone();
        post.number += 1;
        update.posts.push(post);
        update.thread.count = update.posts.len();
        app.receive_thread(update.thread.clone(), Ok(update.clone()))?;
        assert_eq!(app.current().unwrap().data.thread.url(), active);
        assert_eq!(app.focus, Focus::Threads);
        assert_eq!(app.thread_filter, "試");
        assert!(matches!(app.mode, Mode::Input(Prompt::Search, _)));
        assert_eq!(app.states[&first_url].read, read);
        assert_eq!(app.tabs[0].selected, 8);
        app.close_tab(0)?;
        assert_eq!(app.current().unwrap().data.thread.url(), active);
        app.receive_thread(update.thread.clone(), Ok(update))?;
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.store.load_thread(&first_url)?.unwrap().posts.len(), 9);
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn cached_thread_refresh_does_not_reactivate_the_posts_pane() -> Result<()> {
        let (dir, mut app) = test_app("cached-refresh")?;
        let thread = app.threads[1].clone();
        let data = app.store.load_thread(&thread.url())?.unwrap();
        let (tx, requests) = std::sync::mpsc::channel();
        let (_responses, rx) = std::sync::mpsc::channel();
        app.network = Network { tx, rx };
        app.offline = false;
        app.open_thread(thread.clone())?;
        assert_eq!(app.current().unwrap().data.thread.url(), thread.url());
        assert!(app.pending_open.is_empty());
        assert!(matches!(requests.try_recv()?, Request::Thread(_)));
        app.focus = Focus::Boards;
        app.post_filter = "検索を維持".into();
        app.receive_thread(thread, Ok(data))?;
        assert_eq!(app.focus, Focus::Boards);
        assert_eq!(app.post_filter, "検索を維持");
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn out_of_order_requests_activate_only_the_latest_thread() -> Result<()> {
        let (dir, mut app) = test_app("out-of-order")?;
        let original = app.current().unwrap().data.thread.url();
        let mut a = app.current().unwrap().data.clone();
        let mut b = a.clone();
        a.thread.key = "1791200001".into();
        b.thread.key = "1791200002".into();
        app.pending_open.extend([a.thread.url(), b.thread.url()]);
        app.requested_thread = Some(b.thread.url());
        app.receive_thread(a.thread.clone(), Ok(a))?;
        assert_eq!(app.current().unwrap().data.thread.url(), original);
        let latest = b.thread.url();
        app.receive_thread(b.thread.clone(), Ok(b))?;
        assert_eq!(app.current().unwrap().data.thread.url(), latest);
        assert_eq!(app.tabs.len(), 3);
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn refresh_preserves_search_and_selection_after_sort_changes() -> Result<()> {
        let (dir, mut app) = test_app("refresh-selection")?;
        app.focus = Focus::Threads;
        app.thread_filter = "試".into();
        app.sort_threads(ThreadSort::Count);
        app.thread_sel = 1;
        let selected = app.selected_thread_url();
        app.key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE))?;
        assert_eq!(app.thread_filter, "試");
        assert_eq!(app.selected_thread_url(), selected);
        let (tx, _requests) = std::sync::mpsc::channel();
        let (responses, rx) = std::sync::mpsc::channel();
        app.network = Network { tx, rx };
        let mut threads = app.threads.clone();
        threads[1].count = 100;
        threads.reverse();
        responses.send(Response::Subjects(app.boards[0].clone(), Ok(threads)))?;
        app.tick()?;
        assert_eq!(app.thread_filter, "試");
        assert_eq!(app.selected_thread_url(), selected);
        assert_eq!(app.thread_sel, 0);
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn menu_refresh_preserves_selected_board_and_category_cursor() -> Result<()> {
        let (dir, mut app) = test_app("menu-selection")?;
        let board = parse_url("https://bbs.eddibb.cc/liveedge/")?;
        let Target::Board(board) = board else {
            panic!("expected board");
        };
        app.select_board(&board)?;
        let selected = app.boards[app.board_indices()[app.board_sel]].url.clone();
        let (tx, _requests) = std::sync::mpsc::channel();
        let (responses, rx) = std::sync::mpsc::channel();
        app.network = Network { tx, rx };
        let mut boards = app.boards.clone();
        boards.reverse();
        boards
            .iter_mut()
            .find(|b| b.url == selected)
            .unwrap()
            .category = "新しいカテゴリ".into();
        responses.send(Response::Menu(Ok(boards)))?;
        app.tick()?;
        assert_eq!(app.boards[app.board_indices()[app.board_sel]].url, selected);
        let Some(BoardRow::Board(index, _)) = app.board_tree().get(app.board_cursor).cloned()
        else {
            panic!("selected board should stay visible");
        };
        assert_eq!(app.boards[index].url, selected);
        assert!(app.expanded.contains("category:新しいカテゴリ"));
        app.board_cursor = app.board_tree().iter().position(|row| matches!(row, BoardRow::Category(key, _, _) if key == "category:新しいカテゴリ")).unwrap();
        let mut boards = app.boards.clone();
        boards.reverse();
        responses.send(Response::Menu(Ok(boards)))?;
        app.tick()?;
        assert!(
            matches!(app.board_tree().get(app.board_cursor), Some(BoardRow::Category(key, _, _)) if key == "category:新しいカテゴリ")
        );
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn url_navigation_expands_and_selects_the_right_category() -> Result<()> {
        let (dir, mut app) = test_app("url-category")?;
        app.board_filter = "デモ".into();
        app.open_input("https://jbbs.shitaraba.net/game/12345/")?;
        assert!(app.expanded.contains("category:したらば"));
        assert!(app.board_filter.is_empty());
        let Some(BoardRow::Board(index, _)) = app.board_tree().get(app.board_cursor).cloned()
        else {
            panic!("board should be visible and selected");
        };
        assert_eq!(
            app.boards[index].url,
            "https://jbbs.shitaraba.net/game/12345/"
        );
        app.open_input("https://bbs.eddibb.cc/liveedge/1791209783/")?;
        assert!(app.expanded.contains("category:エッヂ"));
        let Some(BoardRow::Board(index, _)) = app.board_tree().get(app.board_cursor).cloned()
        else {
            panic!();
        };
        assert_eq!(app.boards[index].site(), Site::Edge);
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn unicode_editor() {
        let mut e = Editor::new("日本語".into());
        e.key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE), true);
        e.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE), true);
        e.insert("あ");
        assert_eq!(e.text, "日あ語");
    }
    #[test]
    fn refresh_preserves_follow_and_moves_to_new_post() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-refresh-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        let t = app.current_mut().unwrap();
        t.auto = true;
        t.follow = true;
        let mut data = t.data.clone();
        let mut post = data.posts.last().unwrap().clone();
        post.number += 1;
        post.body = "新着".into();
        data.posts.push(post);
        data.thread.count = data.posts.len();
        app.open_data(data)?;
        assert_eq!(app.current().unwrap().selected, 8);
        assert!(app.current().unwrap().auto && app.current().unwrap().follow);
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn creation_sort_includes_year_seconds_and_numeric_timestamp() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-created-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        let base = app.threads[0].clone();
        app.threads = [
            "2026-12-31T15:00:01Z",
            "2025-12-31T14:59:59Z",
            "2025-12-31T15:00:00Z",
            "2026-12-31T15:00:00Z",
        ]
        .into_iter()
        .map(|time| {
            let mut thread = base.clone();
            thread.key = chrono::DateTime::parse_from_rfc3339(time)
                .unwrap()
                .timestamp()
                .to_string();
            thread
        })
        .collect();
        assert_eq!(app.threads[1].created_label(), "12/31 23:59");
        assert_eq!(app.threads[2].created_label(), "01/01 00:00");
        assert_eq!(
            app.threads[0].created_label(),
            app.threads[2].created_label()
        );
        assert_eq!(
            app.threads[0].created_label(),
            app.threads[3].created_label()
        );
        app.sort_threads(ThreadSort::Created);
        assert_eq!(app.thread_indices(), vec![0, 3, 2, 1]);
        app.sort_threads(ThreadSort::Created);
        assert_eq!(app.thread_indices(), vec![1, 2, 3, 0]);
        app.threads.truncate(2);
        app.threads[0].key = "999999999".into();
        app.threads[1].key = "1000000000".into();
        app.sort_threads(ThreadSort::Created);
        assert_eq!(app.thread_indices(), vec![1, 0]);
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn external_sites_preserve_separate_logs_and_drafts() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-sites-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        for url in [
            "https://bbs.eddibb.cc/liveedge/1791209783/",
            "https://jbbs.shitaraba.net/bbs/read.cgi/game/12345/1791209783/",
        ] {
            let Target::Thread(thread) = parse_url(url)? else {
                panic!("thread URL expected");
            };
            let mut data = app.current().unwrap().data.clone();
            data.thread = thread.clone();
            app.store.save_thread(&data)?;
            app.open_thread(thread.clone())?;
            assert_eq!(app.current().unwrap().data.thread.url(), url);
            assert!(app.boards.iter().any(|b| b.url == thread.board.url));
            for target in [Some(thread.clone()), None] {
                app.begin_post(thread.board.clone(), target)?;
                app.compose.as_mut().unwrap().subject = "テスト用下書き".into();
                app.demo = false;
                app.offline = false;
                let error = app
                    .submit_post(&Editor::new("送信しない本文".into()))
                    .unwrap_err();
                assert!(error.to_string().contains("送信していません"));
                assert!(matches!(app.mode, Mode::Draft(_)));
                let draft = app.compose.as_ref().unwrap();
                assert_eq!(
                    app.store
                        .get::<crate::posting::Draft>(&draft.key())?
                        .unwrap()
                        .body,
                    "送信しない本文"
                );
                app.demo = true;
                app.offline = true;
            }
        }
        assert_ne!(app.tabs[1].data.thread.url(), app.tabs[2].data.thread.url());
        drop(app);
        let store = Store::open(&dir)?;
        assert!(
            store
                .load_thread("https://bbs.eddibb.cc/liveedge/1791209783/")?
                .is_some()
        );
        assert!(
            store
                .load_thread("https://jbbs.shitaraba.net/bbs/read.cgi/game/12345/1791209783/")?
                .is_some()
        );
        drop(store);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn chain_ng() {
        let (_, d) = demo();
        let rules = vec![NgRule {
            field: "body".into(),
            pattern: "ようこそ".into(),
            regex: false,
            expires: None,
        }];
        let hidden = hidden_posts(&d[0].posts, &rules, true);
        assert!(hidden[0] && hidden[2] && hidden[3] && hidden[6]);
        assert!(!hidden[1]);
    }
}
