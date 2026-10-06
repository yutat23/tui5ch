use crate::app::{App, Focus, Mode, Prompt, View};
use anyhow::Result;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState, Wrap,
    },
};
use std::path::Path;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy)]
struct Palette {
    bg: Color,
    fg: Color,
    muted: Color,
    border: Color,
    accent: Color,
    gold: Color,
    selected: Color,
}
fn palette(light: bool) -> Palette {
    if light {
        Palette {
            bg: Color::Rgb(245, 247, 250),
            fg: Color::Rgb(30, 40, 55),
            muted: Color::Rgb(90, 105, 120),
            border: Color::Rgb(180, 190, 205),
            accent: Color::Rgb(0, 110, 130),
            gold: Color::Rgb(150, 95, 0),
            selected: Color::Rgb(218, 235, 240),
        }
    } else {
        Palette {
            bg: Color::Rgb(15, 22, 33),
            fg: Color::Rgb(219, 229, 238),
            muted: Color::Rgb(124, 145, 164),
            border: Color::Rgb(48, 65, 83),
            accent: Color::Rgb(99, 211, 206),
            gold: Color::Rgb(240, 193, 111),
            selected: Color::Rgb(28, 55, 68),
        }
    }
}
fn block(title: String, active: bool, p: Palette) -> Block<'static> {
    Block::default()
        .title(Line::from(Span::styled(
            title,
            Style::default()
                .fg(if active { p.accent } else { p.muted })
                .add_modifier(Modifier::BOLD),
        )))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if active { p.accent } else { p.border }))
        .style(Style::default().bg(p.bg).fg(p.fg))
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let p = palette(app.config.theme == "light");
    let area = f.area();
    app.viewport = area;
    app.board_hits.clear();
    app.post_actions.clear();
    app.refresh_hits.clear();
    app.post_scrollbar = None;
    app.post_starts.clear();
    app.thread_hits.clear();
    app.sort_hits.clear();
    app.areas = [Rect::default(); 3];
    app.dividers = [Rect::default(); 3];
    f.render_widget(
        Block::default().style(Style::default().bg(p.bg).fg(p.fg)),
        area,
    );
    if area.width < 30 || area.height < 10 {
        f.render_widget(
            Paragraph::new("端末を30列×10行以上に広げてください\nq: 終了")
                .style(Style::default().fg(p.accent)),
            area,
        );
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(
                app.config
                    .header_height
                    .clamp(3, area.height.saturating_sub(12).max(3)),
            ),
            Constraint::Min(3),
            Constraint::Length(4),
        ])
        .split(area);
    let mode = if app.demo {
        " DEMO "
    } else if app.offline {
        " OFFLINE "
    } else {
        " LIVE "
    };
    let header = Line::from(vec![
        Span::styled(
            " tui5ch ",
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " / 5ちゃんねる terminal reader ",
            Style::default().fg(p.muted),
        ),
        Span::styled(mode, Style::default().fg(p.gold)),
        Span::styled(
            if app.busy.is_empty() {
                String::new()
            } else {
                format!(" 取得中 {}", app.busy.len())
            },
            Style::default().fg(p.gold),
        ),
    ]);
    let tabs = if app.tabs.is_empty() {
        Line::from(Span::styled(
            " 板を選択 → スレを選択 → 読む   /   u: URLを直接開く",
            Style::default().fg(p.muted),
        ))
    } else {
        Line::from(
            app.tabs
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    Span::styled(
                        format!(
                            " {}:{} ",
                            i + 1,
                            t.data.thread.title.chars().take(22).collect::<String>()
                        ),
                        Style::default()
                            .fg(if i == app.tab { p.accent } else { p.muted })
                            .bg(if i == app.tab { p.selected } else { p.bg }),
                    )
                })
                .collect::<Vec<_>>(),
        )
    };
    f.render_widget(
        Paragraph::new(header),
        Rect::new(rows[0].x, rows[0].y, rows[0].width, 1),
    );
    f.render_widget(
        Paragraph::new(tabs).wrap(Wrap { trim: false }),
        Rect::new(
            rows[0].x,
            rows[0].y + 1,
            rows[0].width,
            rows[0].height.saturating_sub(2),
        ),
    );
    app.dividers[2] = Rect::new(
        rows[0].x,
        rows[0].bottom().saturating_sub(1),
        rows[0].width,
        1,
    );
    f.render_widget(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(p.border)),
        app.dividers[2],
    );
    let content = rows[1];
    app.areas = [Rect::default(); 3];
    let three = area.width >= 60
        && content.height >= 8
        && match app.config.layout.as_str() {
            "three" | "two" => true,
            "single" => false,
            _ => area.width >= 95,
        };
    if !three {
        match app.focus {
            Focus::Boards => {
                app.areas[0] = content;
                boards(f, app, content, p);
            }
            Focus::Threads => {
                app.areas[1] = content;
                threads(f, app, content, p);
            }
            Focus::Posts => {
                app.areas[2] = content;
                posts(f, app, content, p);
            }
        }
    } else if app.config.layout == "two" {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(
                    app.config
                        .board_width
                        .clamp(14, content.width.saturating_sub(38).max(14)),
                ),
                Constraint::Min(38),
            ])
            .split(content);
        let right = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(
                    app.config
                        .thread_height
                        .clamp(4, content.height.saturating_sub(4).max(4)),
                ),
                Constraint::Min(4),
            ])
            .split(columns[1]);
        app.areas = [columns[0], right[0], right[1]];
        app.dividers[0] = Rect::new(columns[0].right() - 1, content.y, 2, content.height);
        app.dividers[1] = Rect::new(columns[1].x, right[0].bottom() - 1, columns[1].width, 2);
        boards(f, app, columns[0], p);
        threads(f, app, right[0], p);
        posts(f, app, right[1], p);
    } else {
        let board_width = app
            .config
            .board_width
            .clamp(14, content.width.saturating_sub(46).max(14));
        let thread_width = app
            .config
            .thread_width
            .clamp(18, content.width.saturating_sub(board_width + 28).max(18));
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(board_width),
                Constraint::Length(thread_width),
                Constraint::Min(28),
            ])
            .split(content);
        app.areas = [columns[0], columns[1], columns[2]];
        app.dividers[0] = Rect::new(columns[0].right() - 1, content.y, 2, content.height);
        app.dividers[1] = Rect::new(columns[1].right() - 1, content.y, 2, content.height);
        boards(f, app, columns[0], p);
        threads(f, app, columns[1], p);
        posts(f, app, columns[2], p);
    }
    let foot = Text::from(vec![
        Line::from(Span::styled(
            format!(" {}", app.status.replace('\n', " ")),
            Style::default().fg(p.gold),
        )),
        Line::from(vec![
            Span::styled(" Tab", Style::default().fg(p.accent)),
            Span::raw(" ペイン  "),
            Span::styled("j/k", Style::default().fg(p.accent)),
            Span::raw(" 移動  "),
            Span::styled("/", Style::default().fg(p.accent)),
            Span::raw(" 検索  "),
            Span::styled("r", Style::default().fg(p.accent)),
            Span::raw(" 更新  "),
            Span::styled("z", Style::default().fg(p.accent)),
            Span::raw(" 表示切替  "),
            Span::styled("?", Style::default().fg(p.accent)),
            Span::raw(" ヘルプ  q 終了"),
        ]),
        Line::from(vec![
            Span::styled(" w", Style::default().fg(p.accent)),
            Span::raw(" 書込  "),
            Span::styled("R", Style::default().fg(p.accent)),
            Span::raw(" 引用  "),
            Span::styled("B", Style::default().fg(p.accent)),
            Span::raw(" スレ立て  "),
            Span::styled("境界DnD", Style::default().fg(p.accent)),
            Span::raw(" サイズ変更"),
        ]),
    ]);
    f.render_widget(
        Paragraph::new(foot)
            .style(Style::default().fg(p.muted))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(p.border)),
            ),
        rows[2],
    );
    for (area, focus) in app
        .areas
        .into_iter()
        .zip([Focus::Boards, Focus::Threads, Focus::Posts])
    {
        if area.width >= 10 && area.height >= 2 {
            let button = Rect::new(area.right() - 9, area.y, 8, 1);
            f.render_widget(Clear, button);
            f.render_widget(
                Paragraph::new(" [更新] ").style(
                    Style::default()
                        .bg(p.bg)
                        .fg(p.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                button,
            );
            app.refresh_hits.push((button, focus));
        }
    }
    modal(f, app, p);
}
fn boards(f: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let rows = app.board_tree();
    app.board_cursor = app.board_cursor.min(rows.len().saturating_sub(1));
    let items: Vec<_> = rows
        .iter()
        .map(|row| match row {
            crate::app::BoardRow::Category(key, name, count) => {
                ListItem::new(Line::from(Span::styled(
                    format!(
                        "{} {} ({})",
                        if app.expanded.contains(key) || !app.board_filter.is_empty() {
                            "▾"
                        } else {
                            "▸"
                        },
                        name,
                        count
                    ),
                    Style::default().fg(p.gold),
                )))
            }
            crate::app::BoardRow::Board(i, favorite) => ListItem::new(Line::from(format!(
                "  {}{}",
                if *favorite { "★ " } else { "" },
                app.boards[*i].name
            ))),
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.board_cursor));
    let b = block(
        " 1 板 / カテゴリを開閉 ".into(),
        app.focus == Focus::Boards,
        p,
    );
    let inner = b.inner(area);
    f.render_stateful_widget(
        List::new(items)
            .block(b)
            .highlight_style(Style::default().bg(p.selected).fg(p.accent))
            .highlight_symbol("›"),
        area,
        &mut state,
    );
    for (row, index) in (state.offset()..rows.len()).enumerate() {
        let y = inner.y + row as u16;
        if y >= inner.bottom() {
            break;
        }
        app.board_hits
            .push((Rect::new(inner.x, y, inner.width, 1), index));
    }
}
fn threads(f: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let ids = app.thread_indices();
    let label = match app.view {
        View::Board => "スレ一覧",
        View::Favorites => "お気に入り",
        View::History => "閲覧履歴",
        View::Logs => "保存済みログ",
    };
    let title = format!(" 2 {label}  {} / vで切替 ", ids.len());
    if ids.is_empty() {
        f.render_widget(Paragraph::new("\n  スレがありません。Enterで板を開く / Escで検索解除\n  uでスレURLを直接指定できます。").block(block(title,app.focus==Focus::Threads,p)).style(Style::default().fg(p.muted)),area);
        return;
    }
    let narrow = area.width < 60;
    let b = block(title, app.focus == Focus::Threads, p);
    let inner = b.inner(area);
    f.render_widget(b, area);
    let parts = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
    let buttons = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(parts[0]);
    for (i, sort) in [
        crate::app::ThreadSort::Count,
        crate::app::ThreadSort::Unread,
        crate::app::ThreadSort::Momentum,
        crate::app::ThreadSort::Created,
    ]
    .into_iter()
    .enumerate()
    {
        let text = format!(
            "{}{}",
            sort.label(),
            if app.thread_sort == Some(sort) {
                if app.sort_desc { "↓" } else { "↑" }
            } else {
                ""
            }
        );
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(if app.thread_sort == Some(sort) {
                p.accent
            } else {
                p.gold
            })),
            buttons[i],
        );
        app.sort_hits.push((buttons[i], sort));
    }
    let area = parts[1];
    if narrow {
        let items = ids
            .iter()
            .map(|&i| {
                let t = &app.threads[i];
                let state = app.states.get(&t.url()).cloned().unwrap_or_default();
                let mut lines = wrap_text(
                    &format!("{}{}", if state.favorite { "★ " } else { "" }, t.title),
                    area.width.saturating_sub(4) as usize,
                    true,
                )
                .into_iter()
                .take(2)
                .map(|line| Line::from(Span::styled(line, Style::default().fg(p.fg))))
                .collect::<Vec<_>>();
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}レス +{} 勢い{:.0}",
                        t.count,
                        t.count.saturating_sub(state.read),
                        t.momentum()
                    ),
                    Style::default().fg(p.muted),
                )));
                lines.push(Line::from(Span::styled(
                    format!("作成 {}", t.created_label()),
                    Style::default().fg(p.muted),
                )));
                ListItem::new(lines)
            })
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(app.thread_sel));
        f.render_stateful_widget(
            List::new(items)
                .highlight_style(Style::default().bg(p.selected))
                .highlight_symbol("› "),
            area,
            &mut state,
        );
        let mut y = area.y;
        for (index, &id) in ids.iter().enumerate().skip(state.offset()) {
            if y >= area.bottom() {
                break;
            }
            let t = &app.threads[id];
            let favorite = app.states.get(&t.url()).is_some_and(|s| s.favorite);
            let height = (wrap_text(
                &format!("{}{}", if favorite { "★ " } else { "" }, t.title),
                area.width.saturating_sub(4) as usize,
                true,
            )
            .len()
            .min(2)
                + 2) as u16;
            let h = height.min(area.bottom() - y);
            app.thread_hits
                .push((Rect::new(area.x, y, area.width, h), index));
            y += h;
        }
        return;
    }
    let rows = ids.iter().map(|&i| {
        let t = &app.threads[i];
        let state = app.states.get(&t.url()).cloned().unwrap_or_default();
        let unread = t.count.saturating_sub(state.read);
        Row::new(vec![
            if state.favorite {
                "★".into()
            } else {
                String::new()
            },
            t.title.clone(),
            t.count.to_string(),
            if unread > 0 {
                format!("+{unread}")
            } else {
                "—".into()
            },
            format!("{:.0}", t.momentum()),
            t.created_label(),
        ])
        .style(Style::default().fg(if unread > 0 { p.fg } else { p.muted }))
    });
    let widths = [
        Constraint::Length(2),
        Constraint::Min(12),
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(11),
    ];
    let columns = Layout::horizontal(widths).spacing(1).split(Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(1),
        area.height.min(1),
    ));
    for (column, sort) in [
        (2, crate::app::ThreadSort::Count),
        (3, crate::app::ThreadSort::Unread),
        (4, crate::app::ThreadSort::Momentum),
        (5, crate::app::ThreadSort::Created),
    ] {
        app.sort_hits.push((columns[column], sort));
    }
    let table = Table::new(rows, widths)
        .header(
            Row::new(["", "タイトル", "レス", "未読", "勢い", "作成日時"])
                .style(Style::default().fg(p.muted)),
        )
        .row_highlight_style(Style::default().bg(p.selected).fg(p.accent))
        .highlight_symbol("›");
    let mut state = TableState::default();
    state.select(Some(app.thread_sel));
    f.render_stateful_widget(table, area, &mut state);
    for (row, index) in (state.offset()..ids.len()).enumerate() {
        let y = area.y + 1 + row as u16;
        if y >= area.bottom() {
            break;
        }
        app.thread_hits
            .push((Rect::new(area.x, y, area.width, 1), index));
    }
}
fn posts(f: &mut Frame, app: &mut App, area: Rect, p: Palette) {
    let ids = app.post_indices();
    let active = app.focus == Focus::Posts;
    let Some(tab) = app.current() else {
        f.render_widget(Paragraph::new("\n  スレを開くとここにレスを表示します。\n\n  Enter: スレを開く  /  u: スレURLを入力\n  v: お気に入り・履歴・ログを切替").block(block(" 3 レス ".into(),active,p)).style(Style::default().fg(p.muted)),area);
        return;
    };
    let title = format!(
        " 3 レス {}/{} {}{}{} ",
        tab.selected + 1,
        tab.data.posts.len(),
        if tab.auto { "AUTO " } else { "" },
        if tab.follow { "FOLLOW " } else { "" },
        if app.config.wrap { "" } else { "AA" }
    );
    let b = block(title, active, p);
    let outer_inner = b.inner(area);
    let buttons = Layout::horizontal([
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Min(0),
    ])
    .split(Rect::new(
        outer_inner.x,
        outer_inner.y,
        outer_inner.width,
        outer_inner.height.min(1),
    ));
    let auto = tab.auto;
    let follow = tab.follow;
    f.render_widget(b, area);
    for (i, (text, key)) in [
        (format!("A 自動:{}", if auto { "ON" } else { "OFF" }), 'A'),
        (format!("F 追従:{}", if follow { "ON" } else { "OFF" }), 'F'),
        (format!("I {}秒", app.config.refresh_seconds), 'I'),
    ]
    .into_iter()
    .enumerate()
    {
        f.render_widget(
            Paragraph::new(text).style(Style::default().fg(p.gold)),
            buttons[i],
        );
        app.post_actions.push((buttons[i], key));
    }
    let latest = Rect::new(
        outer_inner.x,
        outer_inner.y + outer_inner.height.min(1),
        outer_inner.width.min(20),
        outer_inner.height.saturating_sub(1).min(1),
    );
    f.render_widget(
        Paragraph::new("[最新へ End/G]").style(Style::default().fg(p.accent)),
        latest,
    );
    app.post_actions.push((latest, 'G'));
    let outer_inner = Rect::new(
        outer_inner.x,
        outer_inner.y + outer_inner.height.min(2),
        outer_inner.width,
        outer_inner.height.saturating_sub(2),
    );
    let tab = app.current().unwrap();
    let heading = wrap_text(&tab.data.thread.title, outer_inner.width as usize, true);
    let heading_height = (heading.len().min(4) as u16)
        .min(outer_inner.height.saturating_sub(2))
        .max(1)
        .min(outer_inner.height);
    let heading_area = Rect::new(
        outer_inner.x,
        outer_inner.y,
        outer_inner.width,
        heading_height,
    );
    let separator = if outer_inner.height > heading_height {
        1
    } else {
        0
    };
    let inner = Rect::new(
        outer_inner.x,
        outer_inner.y + heading_height + separator,
        outer_inner.width.saturating_sub(1),
        outer_inner
            .height
            .saturating_sub(heading_height + separator),
    );
    f.render_widget(
        Paragraph::new(
            heading
                .into_iter()
                .take(heading_height as usize)
                .map(Line::from)
                .collect::<Vec<_>>(),
        )
        .style(Style::default().fg(p.accent).add_modifier(Modifier::BOLD)),
        heading_area,
    );
    if separator > 0 {
        f.render_widget(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(p.border)),
            Rect::new(outer_inner.x, heading_area.bottom(), outer_inner.width, 1),
        );
    }
    app.post_height = inner.height as usize;
    if ids.is_empty() {
        f.render_widget(
            Paragraph::new("\n  表示対象のレスがありません。Escで検索解除 / NでNG設定")
                .style(Style::default().fg(p.muted)),
            inner,
        );
        return;
    }
    let tab = app.current().unwrap();
    let mut lines = Vec::new();
    let mut selected_start = None;
    let mut post_starts = Vec::new();
    let mut replies = vec![0; tab.data.posts.len()];
    for post in &tab.data.posts {
        for &n in &post.refs {
            if let Some(count) = replies.get_mut(n.saturating_sub(1)) {
                *count += 1;
            }
        }
    }
    for &i in &ids {
        post_starts.push((lines.len(), i));
        let post = &tab.data.posts[i];
        let selected = i == tab.selected;
        if selected {
            selected_start = Some(lines.len());
        }
        if post.number == tab.boundary + 1 {
            lines.push(Line::from(Span::styled(
                "──── ここから未読 ────",
                Style::default().fg(p.gold),
            )));
        }
        lines.push(
            Line::from(vec![
                Span::styled(
                    format!("{} {} ", if selected { "›" } else { " " }, post.number),
                    Style::default()
                        .fg(if selected { p.accent } else { p.gold })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(post.name.clone(), Style::default().fg(p.accent)),
                Span::styled(
                    format!(
                        "  {}{}",
                        post.date,
                        if replies[i] > 0 {
                            format!("  ↩{}", replies[i])
                        } else {
                            String::new()
                        }
                    ),
                    Style::default().fg(p.muted),
                ),
            ])
            .style(if selected {
                Style::default().bg(p.selected)
            } else {
                Style::default()
            }),
        );
        for line in wrap_text(
            &post.body,
            inner.width.saturating_sub(2) as usize,
            app.config.wrap,
        ) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(line, Style::default().fg(p.fg)),
            ]));
        }
        lines.push(Line::from(""));
    }
    let total = lines.len();
    let height = inner.height as usize;
    let tab = app.current_mut().unwrap();
    if !tab.jump_bottom && (tab.rendered != Some((tab.selected, inner.width)) || tab.follow) {
        if let Some(start) = selected_start
            && (start < tab.scroll || start >= tab.scroll + height.saturating_sub(2))
        {
            tab.scroll = start;
        }
        tab.rendered = Some((tab.selected, inner.width));
    }
    tab.scroll = if tab.follow || tab.jump_bottom {
        total.saturating_sub(height)
    } else {
        tab.scroll.min(total.saturating_sub(height))
    };
    tab.jump_bottom = false;
    tab.rendered = Some((tab.selected, inner.width));
    let scroll = tab.scroll;
    let max_scroll = total.saturating_sub(height);
    app.post_starts = post_starts;
    app.post_text_width = inner.width;
    if inner.height > 0 {
        let thumb_len = if total <= height {
            inner.height
        } else {
            (height * height / total).max(1) as u16
        };
        let travel = inner.height - thumb_len;
        let thumb_top = inner.y
            + (scroll * travel as usize)
                .checked_div(max_scroll)
                .unwrap_or(0) as u16;
        let rect = Rect::new(inner.right(), inner.y, 1, inner.height);
        for row in rect.y..rect.bottom() {
            f.render_widget(
                Paragraph::new(if row >= thumb_top && row < thumb_top + thumb_len {
                    "█"
                } else {
                    "│"
                })
                .style(Style::default().fg(
                    if row >= thumb_top && row < thumb_top + thumb_len {
                        p.accent
                    } else {
                        p.border
                    },
                )),
                Rect::new(rect.x, row, 1, 1),
            );
        }
        app.post_scrollbar = Some(crate::app::PostScrollbar {
            rect,
            max_scroll,
            thumb_top,
            thumb_len,
        });
    }
    let visible = lines
        .into_iter()
        .skip(scroll)
        .take(height)
        .collect::<Vec<_>>();
    f.render_widget(
        Paragraph::new(Text::from(visible)).style(Style::default().fg(p.fg)),
        inner,
    );
}
pub fn wrap_text(text: &str, width: usize, wrap: bool) -> Vec<String> {
    let width = width.max(1);
    let mut result = vec![];
    for line in text.split('\n') {
        if !wrap {
            result.push(line.to_owned());
            continue;
        }
        let mut current = String::new();
        let mut used = 0;
        for c in line.chars() {
            let cw = c.width().unwrap_or(0);
            if used + cw > width && !current.is_empty() {
                result.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(c);
            used += cw;
        }
        result.push(current);
    }
    result
}
fn center(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(4));
    let h = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
fn help_text(p: Palette) -> Text<'static> {
    let content = r#"# ペインを選ぶ
`Tab` / `Shift+Tab`  操作対象を次・前のペインへ
`1` 板一覧   `2` スレ一覧   `3` レス
`j` / `k`・`↑` / `↓`  選択を移動   `Enter` 開く・参照
`Space` / `PgDn` 5件進む   `PgUp` 5件戻る
`Home` 先頭   `End` / `G` 末尾（レスでは最新本文の末尾）
`Ctrl+↑` / `Ctrl+↓`  本文を1行スクロール

# 上部のスレタブを切り替える
`[` 前のスレ   `]` 次のスレ   `x` 現在のスレタブを閉じる
上部の「1:タイトル」「2:タイトル」はタブ番号です。
`[` / `]` はIMEを英数入力にして押してください。
`1` / `2` / `3` はペイン選択用です。

# マウス・表示
板・スレをクリックして開く / 各ペイン右上の「更新」で手動更新
カテゴリはクリック・`Enter`で開閉、`→`で展開、`←`で閉じる
板を右クリック・`f`でお気に入りを切り替え
お気に入り内の順序: ドラッグ、または `Ctrl+↑` / `Ctrl+↓`
レス欄の「最新へ」: 最新の本文末尾 / スクロールバー: クリック・ドラッグ
ホイール: 板・スレは項目移動、レスは3行スクロール
境界をドラッグ: 板・スレの幅、上下表示の高さ、タブ欄の高さ
`z` 自動 → 3ペイン → 上下表示 → 1ペイン
`W` AA折り返し   `T` 配色を切り替え

# 検索・整理
`/` 選択ペインを検索   `Esc` 検索解除
`u` 板・スレURLを開く   `f` お気に入りを切り替え
`v` 板 → お気に入り → 履歴 → 保存済みログ
`c` レス数   `U` 未読数   `s` 勢い   `d` 作成日時で並べ替え
同じキー・見出しで昇順と降順を切り替え
`p` 次スレ候補   `L` 保存済みログの全文検索

# 読む・更新
`r` 操作中のペインを更新   `n` 未読へ   `g` レス番号へ
`a` 参照レス   `i` 同じID   `t` 被返信レス
`A` 自動更新＋新着追従   `F` 追従のみ   `I` 更新間隔
レス欄の「A 自動」「F 追従」「I 秒数」もクリックできます。
`o` リンクを開く   `b` スレをブラウザで開く
`e` ログを書き出す

# NG・書き込み
`N` NG設定: id:文字列 / name:文字列 / body:文字列 / title:文字列
正規表現: body:/正規表現/   期限: id@60:文字列   全解除: clear
`w` 返信   `R` 選択レスを引用して返信   `B` スレ立て
下書き: `F2` 名前   `F3` メール   `F4` スレタイトル
`Enter` 改行   `←` / `→` カーソル   `Esc` 保存して閉じる
`F6` / `Ctrl+Enter` 内容を確認 → `Enter` 送信（TUI内）

# ヘルプ・終了
`j` / `k`・`↑` / `↓` ヘルプをスクロール   `PgUp` / `PgDn` ページ移動
`?` / `Esc` / `q` ヘルプを閉じる   `Ctrl+C` アプリを終了
通常画面では `q` でアプリを終了"#;
    let key_style = Style::default()
        .fg(p.accent)
        .bg(p.selected)
        .add_modifier(Modifier::BOLD);
    Text::from(
        content
            .lines()
            .map(|line| {
                if let Some(title) = line.strip_prefix("# ") {
                    return Line::from(Span::styled(
                        title.to_owned(),
                        Style::default().fg(p.gold).add_modifier(Modifier::BOLD),
                    ));
                }
                Line::from(
                    line.split('`')
                        .enumerate()
                        .map(|(i, part)| {
                            Span::styled(
                                part.to_owned(),
                                if i % 2 == 1 {
                                    key_style
                                } else {
                                    Style::default().fg(p.fg)
                                },
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
}
fn help_lines(p: Palette, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut output = Vec::new();
    for line in help_text(p).lines {
        let mut spans = Vec::new();
        let mut used = 0;
        for span in line.spans {
            for c in span.content.chars() {
                let w = c.width().unwrap_or(0);
                if used + w > width && !spans.is_empty() {
                    output.push(Line::from(std::mem::take(&mut spans)));
                    used = 0;
                }
                spans.push(Span::styled(c.to_string(), span.style));
                used += w;
            }
        }
        output.push(Line::from(spans));
    }
    output
}
pub fn help_scroll_limit(area: Rect) -> usize {
    let rect = center(area, 92, 34);
    help_lines(palette(false), rect.width.saturating_sub(2) as usize)
        .len()
        .saturating_sub(rect.height.saturating_sub(2) as usize)
}
fn modal(f: &mut Frame, app: &App, p: Palette) {
    let area = f.area();
    match &app.mode {
        Mode::Normal => {}
        Mode::Help(scroll) => {
            let rect = center(area, 92, 34);
            f.render_widget(Clear, rect);
            f.render_widget(
                Paragraph::new(help_lines(p, rect.width.saturating_sub(2) as usize))
                    .scroll(((*scroll).min(u16::MAX as usize) as u16, 0))
                    .block(block(
                        " 操作一覧 / j/k:スクロール / Esc:閉じる ".into(),
                        true,
                        p,
                    ))
                    .style(Style::default().fg(p.fg)),
                rect,
            );
        }
        Mode::Input(kind, e) => {
            let (title, hint) = match kind {
                Prompt::Name => ("名前", "Enter 確定"),
                Prompt::Mail => ("メール", "sageなど / Enter 確定"),
                Prompt::Subject => ("スレタイトル", "スレ立て時に必須 / Enter 確定"),
                Prompt::Search => ("検索", "Enterで絞り込み / Escでキャンセル"),
                Prompt::Url => ("URLを開く", "5ch / エッヂ / したらばの板・スレURL"),
                Prompt::Jump => ("レス番号", "移動先のレス番号を入力"),
                Prompt::Ng => (
                    "NGルール",
                    "id:文字列 / body:/正規表現/ / name@60:文字列 / clear",
                ),
                Prompt::Export => (
                    "ログを書き出す",
                    "保存先パス（.jsonならJSON / その他はテキスト）",
                ),
                Prompt::Refresh => ("自動更新間隔", "秒数を入力（最小10秒）"),
                Prompt::Logs => ("保存済みログの全文検索", "本文・タイトル・ID・名前から検索"),
            };
            let rect = center(area, 88, 7);
            f.render_widget(Clear, rect);
            let b = block(format!(" {title} "), true, p);
            let inner = b.inner(rect);
            f.render_widget(b, rect);
            f.render_widget(
                Paragraph::new(Text::from(vec![
                    Line::from(Span::styled(hint, Style::default().fg(p.muted))),
                    Line::from(""),
                    Line::from(""),
                    Line::from("Enter 決定 / Esc キャンセル / Ctrl+U クリア"),
                ])),
                inner,
            );
            let col = unicode_width::UnicodeWidthStr::width(&e.text[..e.cursor]);
            let left = col.saturating_sub(inner.width.saturating_sub(1) as usize);
            if inner.height > 2 {
                let input_area = Rect::new(inner.x, inner.y + 2, inner.width, 1);
                f.render_widget(
                    Paragraph::new(e.text.clone())
                        .scroll((0, left.min(u16::MAX as usize) as u16))
                        .style(Style::default().fg(p.accent)),
                    input_area,
                );
                f.set_cursor_position((inner.x + (col - left) as u16, inner.y + 2));
            }
        }
        Mode::Popup(title, items, selected) => {
            let rect = center(area, 90, area.height.saturating_sub(6));
            f.render_widget(Clear, rect);
            if items.is_empty() {
                f.render_widget(
                    Paragraph::new("\n 該当するレス・リンクがありません。Escで閉じる")
                        .block(block(format!(" {title} "), true, p)),
                    rect,
                );
            } else {
                let list = List::new(items.iter().map(|item| {
                    ListItem::new(Text::from(
                        wrap_text(&item.label, rect.width.saturating_sub(5) as usize, true)
                            .into_iter()
                            .map(Line::from)
                            .chain(std::iter::once(Line::from("")))
                            .collect::<Vec<_>>(),
                    ))
                }))
                .block(block(
                    format!(" {title} / Enterで移動・開く / Escで閉じる "),
                    true,
                    p,
                ))
                .highlight_style(Style::default().bg(p.selected))
                .highlight_symbol("› ");
                let mut state = ListState::default();
                state.select(Some(*selected));
                f.render_stateful_widget(list, rect, &mut state);
            }
        }
        Mode::Sending => {
            let rect = center(area, 85, 8);
            f.render_widget(Clear, rect);
            f.render_widget(Paragraph::new("サーバーの応答を待っています…\n通信が途切れた場合は投稿済みか確認してから再送してください。").block(block(" 投稿処理中 ".into(),true,p)),rect);
        }
        Mode::Server(page, scroll, selected, raw) => {
            let rect = center(area, 95, area.height.saturating_sub(2));
            f.render_widget(Clear, rect);
            let b = block(format!(" サーバー応答 HTTP {} ", page.status), true, p);
            let inner = b.inner(rect);
            f.render_widget(b, rect);
            let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(7)]).split(inner);
            f.render_widget(
                Paragraph::new(if *raw {
                    crate::parse::clean(&page.html)
                } else {
                    page.text.clone()
                })
                .wrap(Wrap { trim: false })
                .scroll(((*scroll).min(u16::MAX as usize) as u16, 0)),
                parts[0],
            );
            let mut labels = vec![page.url.clone()];
            let mut index = 0;
            for form in &page.forms {
                for c in &form.checks {
                    labels.push(format!(
                        "[{}] {}",
                        if c.checked { "x" } else { " " },
                        c.label
                    ));
                }
                for c in &form.choices {
                    labels.push(format!(
                        "{} {}",
                        if index == *selected { ">" } else { " " },
                        c.label
                    ));
                    index += 1;
                }
            }
            labels.push(
                "↑↓/PgUp/PgDn スクロール / Tab 選択 / 1〜9 同意切替 / v HTML / Enter 実行 / Esc 戻る"
                    .into(),
            );
            f.render_widget(
                Paragraph::new(labels.join("\n"))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(p.gold)),
                parts[1],
            );
        }
        Mode::Draft(e) | Mode::Confirm(e) => {
            let confirming = matches!(app.mode, Mode::Confirm(_));
            let rect = center(area, 90, area.height.saturating_sub(4));
            f.render_widget(Clear, rect);
            let b = block(
                if confirming {
                    " 投稿内容を確認 "
                } else {
                    " 投稿の下書き / 自動保存 "
                }
                .into(),
                true,
                p,
            );
            let inner = b.inner(rect);
            f.render_widget(b, rect);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(5)])
                .split(inner);
            let chunks = if let Some(d) = &app.compose {
                let parts =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(chunks[0]);
                f.render_widget(
                    Paragraph::new(format!(
                        "名前: {}  メール: {}\n板: {}\nタイトル: {}",
                        d.name,
                        d.mail,
                        d.board.name,
                        d.thread
                            .as_ref()
                            .map(|t| t.title.as_str())
                            .unwrap_or(&d.subject)
                    ))
                    .style(Style::default().fg(p.gold)),
                    parts[0],
                );
                vec![parts[1], chunks[1]]
            } else {
                chunks.to_vec()
            };
            let before = &e.text[..e.cursor];
            let row = before.chars().filter(|&c| c == '\n').count();
            let last = before.rsplit('\n').next().unwrap_or("");
            let col = unicode_width::UnicodeWidthStr::width(last);
            let top = row.saturating_sub(chunks[0].height.saturating_sub(1) as usize);
            let left = col.saturating_sub(chunks[0].width.saturating_sub(1) as usize);
            f.render_widget(
                Paragraph::new(e.text.clone())
                    .scroll((
                        top.min(u16::MAX as usize) as u16,
                        left.min(u16::MAX as usize) as u16,
                    ))
                    .style(Style::default().fg(p.fg)),
                chunks[0],
            );
            let hint = if app
                .compose
                .as_ref()
                .is_some_and(|d| d.board.site() != crate::model::Site::FiveCh)
            {
                "このサイトは閲覧のみ対応しています。返信・スレ立ては未対応です。\n本文・名前・メールは下書きとして保存できます。\nEsc 保存して閉じる / F2 名前 / F3 メール / F4 スレタイトル"
            } else if confirming {
                "Enterでこの内容を5chに送信します。\nサーバーの確認が返った場合は、TUI内で確認して続行します。\nEnter: 送信 / その他: 編集に戻る"
            } else {
                "Enter 改行 / ←→ カーソル / Home・End 行頭・行末\nF2 名前 / F3 メール / F4 スレタイトル\nEsc 保存して閉じる / F6・Ctrl+Enter 投稿内容を確認"
            };
            f.render_widget(
                Paragraph::new(hint)
                    .style(Style::default().fg(p.gold))
                    .block(
                        Block::default()
                            .borders(Borders::TOP)
                            .border_style(Style::default().fg(p.border)),
                    ),
                chunks[1],
            );
            if !confirming && chunks[0].height > 0 && chunks[0].width > 0 {
                f.set_cursor_position((
                    chunks[0].x + (col - left) as u16,
                    chunks[0].y + (row - top) as u16,
                ));
            }
        }
    }
}
pub fn snapshot(app: &mut App, path: &Path) -> Result<()> {
    let backend = ratatui::backend::TestBackend::new(120, 36);
    let mut terminal = ratatui::Terminal::new(backend)?;
    terminal.draw(|f| draw(f, app))?;
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..36 {
        let mut x = 0;
        while x < 120 {
            let c = &buffer[(x, y)];
            text.push_str(c.symbol());
            x += unicode_width::UnicodeWidthStr::width(c.symbol()).max(1) as u16;
        }
        text.push('\n');
    }
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn japanese_width_and_aa() {
        assert_eq!(wrap_text("日本語abc", 4, true), vec!["日本", "語ab", "c"]);
        assert_eq!(wrap_text("  /\\\n (oo)", 2, false), vec!["  /\\", " (oo)"]);
    }
    #[test]
    fn reading_workflow_and_responsive_rendering() -> Result<()> {
        use crate::{app::Editor, store::Store};
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        let key = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        app.move_selection(2)?;
        assert_eq!(app.current().unwrap().selected, 2);
        let url = app.current().unwrap().data.thread.url();
        assert_eq!(app.states[&url].read, 3);
        app.key(key('f'))?;
        assert!(app.states[&url].favorite);
        app.key(key('a'))?;
        assert!(matches!(&app.mode,Mode::Popup(_,items,_) if items.len()==1));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
        app.mode = Mode::Input(Prompt::Search, Editor::new("NGの追加".into()));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
        assert_eq!(app.post_indices(), vec![4]);
        for (w, h) in [(120, 36), (70, 22), (30, 10)] {
            let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h))?;
            terminal.draw(|f| draw(f, &mut app))?;
            let symbols = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(symbols.contains("tui5ch"));
        }
        app.mode = Mode::Draft(Editor::new("保存される日本語の下書き".into()));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
        drop(app);
        let restored = App::new(Store::open(&dir)?, true, true)?;
        assert!(restored.states[&url].favorite);
        assert_eq!(
            restored
                .store
                .get::<String>(&format!("draft:{url}"))?
                .unwrap(),
            "保存される日本語の下書き"
        );
        drop(restored);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn tui_post_composition_and_offline_guard() -> Result<()> {
        use crate::store::Store;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-compose-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        app.key(key(KeyCode::Char('w')))?;
        assert!(app.compose.as_ref().unwrap().thread.is_some());
        app.key(key(KeyCode::F(2)))?;
        app.mode = Mode::Input(Prompt::Name, crate::app::Editor::new("名前".into()));
        app.key(key(KeyCode::Enter))?;
        app.mode = Mode::Draft(crate::app::Editor::new("投稿内容".into()));
        app.key(key(KeyCode::F(6)))?;
        assert!(matches!(app.mode, Mode::Confirm(_)));
        assert!(app.key(key(KeyCode::Enter)).is_err());
        assert!(matches!(app.mode, Mode::Draft(_)));
        app.key(key(KeyCode::Esc))?;
        app.key(key(KeyCode::Char('B')))?;
        assert!(matches!(app.mode, Mode::Input(Prompt::Subject, _)));
        app.mode = Mode::Input(
            Prompt::Subject,
            crate::app::Editor::new("新スレタイトル".into()),
        );
        app.key(key(KeyCode::Enter))?;
        assert_eq!(app.compose.as_ref().unwrap().subject, "新スレタイトル");
        assert_eq!(app.compose.as_ref().unwrap().name, "名前");
        for (w, h) in [(120, 36), (60, 20), (30, 10)] {
            let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h))?;
            t.draw(|f| draw(f, &mut app))?;
        }
        app.key(key(KeyCode::Esc))?;
        let saved = app
            .store
            .get::<crate::posting::Draft>(&app.compose.as_ref().unwrap().key())?
            .unwrap();
        assert_eq!(saved.subject, "新スレタイトル");
        app.mode = Mode::Server(
            crate::posting::Page {
                html: "<title>確認</title>".into(),
                text: "確認".into(),
                url: "https://agree.5ch.io/test/bbs.cgi".into(),
                forms: vec![],
                success: false,
                status: 200,
            },
            0,
            0,
            false,
        );
        app.key(key(KeyCode::Char('v')))?;
        assert!(matches!(app.mode, Mode::Server(_, 0, 0, true)));
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn mouse_tree_favorite_order_sort_and_scroll() -> Result<()> {
        use crate::{
            app::{BoardRow, ThreadSort},
            model::{Board, ThreadState},
            store::Store,
        };
        use crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-mouse-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        app.config.layout = "three".into();
        for n in 0..25 {
            app.boards.push(Board {
                name: format!("板{n}"),
                category: format!("カテゴリ{n}"),
                url: format!("https://agree.5ch.io/test{n}/"),
            });
        }
        app.store.set("boards", &app.boards)?;
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36))?;
        let mouse = |kind, r: Rect| MouseEvent {
            kind,
            column: r.x + 1,
            row: r.y,
            modifiers: KeyModifiers::NONE,
        };
        app.board_cursor = 1;
        terminal.draw(|f| draw(f, &mut app))?;
        let r = app.board_hits.iter().find(|(_, i)| *i == 1).unwrap().0;
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), r))?;
        assert!(matches!(app.board_tree()[2], BoardRow::Board(0, false)));
        terminal.draw(|f| draw(f, &mut app))?;
        let r = app.board_hits.iter().find(|(_, i)| *i == 2).unwrap().0;
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), r))?;
        app.mouse(mouse(MouseEventKind::Up(MouseButton::Left), r))?;
        assert_eq!(app.focus, Focus::Threads);
        assert_eq!(app.threads.len(), 2);
        app.focus = Focus::Boards;
        app.board_cursor = app.board_tree().len() - 1;
        terminal.draw(|f| draw(f, &mut app))?;
        assert!(app.board_hits[0].1 > 0);
        let (r, index) = app.board_hits.last().copied().unwrap();
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), r))?;
        assert_eq!(app.board_cursor, index);
        app.board_favorites = app.boards[..3].iter().map(|b| b.url.clone()).collect();
        app.expanded.insert("favorites".into());
        app.store.set("board-expanded", &app.expanded)?;
        app.store.set("board_favorites", &app.board_favorites)?;
        app.board_cursor = 1;
        app.focus = Focus::Boards;
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL))?;
        assert_eq!(app.board_favorites[1], app.boards[0].url);
        terminal.draw(|f| draw(f, &mut app))?;
        let from = app.board_hits.iter().find(|(_, i)| *i == 2).unwrap().0;
        let to = app.board_hits.iter().find(|(_, i)| *i == 3).unwrap().0;
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), from))?;
        app.mouse(mouse(MouseEventKind::Drag(MouseButton::Left), to))?;
        app.mouse(mouse(MouseEventKind::Up(MouseButton::Left), to))?;
        assert_eq!(app.board_favorites[2], app.boards[0].url);
        app.states.insert(
            app.threads[0].url(),
            ThreadState {
                read: 8,
                ..Default::default()
            },
        );
        app.states
            .insert(app.threads[1].url(), ThreadState::default());
        terminal.draw(|f| draw(f, &mut app))?;
        let r = app
            .sort_hits
            .iter()
            .find(|(_, s)| *s == ThreadSort::Unread)
            .unwrap()
            .0;
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), r))?;
        assert_eq!(app.thread_indices(), vec![1, 0]);
        app.key(KeyEvent::new(KeyCode::Char('U'), KeyModifiers::NONE))?;
        assert_eq!(app.thread_indices(), vec![0, 1]);
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE))?;
        assert_eq!(app.thread_indices(), vec![0, 1]);
        app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE))?;
        assert_eq!(app.thread_indices(), vec![0, 1]);
        for (w, h) in [(120, 36), (200, 36)] {
            let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h))?;
            app.config.layout = if w == 200 { "single" } else { "three" }.into();
            app.focus = Focus::Threads;
            app.thread_sel = 1;
            t.draw(|f| draw(f, &mut app))?;
            if w == 200 {
                let r = app
                    .sort_hits
                    .iter()
                    .rev()
                    .find(|(_, sort)| *sort == ThreadSort::Count)
                    .unwrap()
                    .0;
                assert_eq!(t.backend().buffer()[(r.x, r.y)].symbol(), "レ");
                let date = app
                    .sort_hits
                    .iter()
                    .rev()
                    .find(|(_, sort)| *sort == ThreadSort::Created)
                    .unwrap()
                    .0;
                assert_eq!(t.backend().buffer()[(date.x, date.y)].symbol(), "作");
                let line = (date.x..date.right())
                    .map(|x| t.backend().buffer()[(x, date.y + 1)].symbol())
                    .collect::<String>();
                assert_eq!(line, app.threads[app.thread_indices()[0]].created_label());
            }
            let r = app.thread_hits.iter().find(|(_, i)| *i == 1).unwrap().0;
            app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), r))?;
            assert_eq!(
                app.current().unwrap().data.thread.url(),
                app.threads[1].url()
            );
        }
        let expected = app.board_favorites.clone();
        drop(app);
        let restored = App::new(Store::open(&dir)?, true, true)?;
        assert_eq!(restored.board_favorites, expected);
        assert!(restored.expanded.contains("favorites"));
        drop(restored);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn latest_and_draggable_scrollbar_reach_entire_body() -> Result<()> {
        use crate::store::Store;
        use crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-scrollbar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        app.current_mut()
            .unwrap()
            .data
            .posts
            .last_mut()
            .unwrap()
            .body = (0..200).map(|i| format!("最終本文{i}\n")).collect();
        for layout in ["three", "two", "single"] {
            app.config.layout = layout.into();
            app.focus = Focus::Posts;
            let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36))?;
            app.key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE))?;
            terminal.draw(|f| draw(f, &mut app))?;
            let bar = app.post_scrollbar.unwrap();
            assert!(bar.max_scroll > 0);
            assert_eq!(app.current().unwrap().scroll, bar.max_scroll);
            assert_eq!(bar.thumb_top + bar.thumb_len, bar.rect.bottom());
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>();
            assert!(text.contains("199"));
            let mouse = |kind, row| MouseEvent {
                kind,
                column: bar.rect.x,
                row,
                modifiers: KeyModifiers::NONE,
            };
            app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), bar.rect.y))?;
            app.mouse(mouse(MouseEventKind::Up(MouseButton::Left), bar.rect.y))?;
            assert_eq!(app.current().unwrap().scroll, 0);
            assert!(!app.current().unwrap().follow);
            terminal.draw(|f| draw(f, &mut app))?;
            let bar = app.post_scrollbar.unwrap();
            app.mouse(mouse(
                MouseEventKind::Down(MouseButton::Left),
                bar.thumb_top,
            ))?;
            app.mouse(mouse(
                MouseEventKind::Drag(MouseButton::Left),
                bar.rect.bottom() + 10,
            ))?;
            app.mouse(mouse(
                MouseEventKind::Up(MouseButton::Left),
                bar.rect.bottom() + 10,
            ))?;
            assert_eq!(app.current().unwrap().scroll, bar.max_scroll);
            terminal.draw(|f| draw(f, &mut app))?;
            let bar = app.post_scrollbar.unwrap();
            let before = app.current().unwrap().scroll;
            app.mouse(mouse(
                MouseEventKind::Down(MouseButton::Left),
                bar.thumb_top,
            ))?;
            app.mouse(mouse(MouseEventKind::Up(MouseButton::Left), bar.thumb_top))?;
            assert_eq!(app.current().unwrap().scroll, before);
            app.mouse(mouse(MouseEventKind::ScrollUp, bar.thumb_top))?;
            assert_eq!(app.current().unwrap().scroll, before.saturating_sub(3));
            let r = app.post_actions.iter().find(|(_, c)| *c == 'G').unwrap().0;
            app.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: r.x,
                row: r.y,
                modifiers: KeyModifiers::NONE,
            })?;
            terminal.draw(|f| draw(f, &mut app))?;
            assert_eq!(
                app.current().unwrap().scroll,
                app.post_scrollbar.unwrap().max_scroll
            );
        }
        app.post_filter = "存在しない本文".into();
        let mut tiny = ratatui::Terminal::new(ratatui::backend::TestBackend::new(30, 10))?;
        tiny.draw(|f| draw(f, &mut app))?;
        assert!(app.post_scrollbar.is_none());
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn refresh_buttons_work_in_each_layout_and_empty_list() -> Result<()> {
        use crate::store::Store;
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-refresh-click-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36))?;
        for layout in ["three", "two", "single"] {
            app.config.layout = layout.into();
            for focus in [Focus::Boards, Focus::Threads, Focus::Posts] {
                app.focus = focus;
                if focus == Focus::Threads {
                    app.threads.clear();
                }
                terminal.draw(|f| draw(f, &mut app))?;
                let r = app
                    .refresh_hits
                    .iter()
                    .find(|(_, f)| *f == focus)
                    .unwrap()
                    .0;
                assert_eq!(terminal.backend().buffer()[(r.x + 2, r.y)].symbol(), "更");
                app.mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: r.x + 2,
                    row: r.y,
                    modifiers: KeyModifiers::NONE,
                })?;
                assert_eq!(app.focus, focus);
                assert!(app.drag.is_none());
                if focus == Focus::Threads {
                    assert_eq!(app.threads.len(), 2);
                }
            }
        }
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn automatic_follow_toolbar_scrolls_to_end() -> Result<()> {
        use crate::store::Store;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-follow-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        app.offline = false;
        app.key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE))?;
        assert!(app.current().unwrap().auto && app.current().unwrap().follow);
        let t = app.current_mut().unwrap();
        t.data.posts.last_mut().unwrap().body =
            (0..100).map(|i| format!("末尾の行{i}\n")).collect();
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36))?;
        terminal.draw(|f| draw(f, &mut app))?;
        assert!(app.current().unwrap().scroll > 0);
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("99"));
        let r = app.post_actions.iter().find(|(_, c)| *c == 'A').unwrap().0;
        app.mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: r.x,
            row: r.y,
            modifiers: KeyModifiers::NONE,
        })?;
        assert!(!app.current().unwrap().auto && !app.current().unwrap().follow);
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn drag_resize_layout_toggle_and_persistence() -> Result<()> {
        use crate::store::Store;
        use crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        let dir = std::env::temp_dir().join(format!(
            "tui5ch-layout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        let mut app = App::new(Store::open(&dir)?, true, true)?;
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36))?;
        terminal.draw(|f| draw(f, &mut app))?;
        let mouse = |kind, column, row| MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        for (index, dx, dy) in [(0, 9, 0), (1, 3, 0), (2, 0, 2)] {
            let divider = app.dividers[index];
            let column = if index == 0 {
                divider.x
            } else {
                divider.x + divider.width / 2
            };
            app.mouse(mouse(
                MouseEventKind::Down(MouseButton::Left),
                column,
                divider.y,
            ))?;
            app.mouse(mouse(
                MouseEventKind::Drag(MouseButton::Left),
                column + dx,
                divider.y + dy,
            ))?;
            app.mouse(mouse(
                MouseEventKind::Up(MouseButton::Left),
                column + dx,
                divider.y + dy,
            ))?;
            terminal.draw(|f| draw(f, &mut app))?;
        }
        assert_eq!(app.areas[0].width, 36);
        assert_eq!(app.areas[1].width, 39);
        assert!(
            app.areas
                .iter()
                .all(|a| a.y == app.areas[0].y && a.height == app.areas[0].height)
        );
        let title = app.current().unwrap().data.thread.title.clone();
        app.move_selection(1000)?;
        terminal.draw(|f| draw(f, &mut app))?;
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .replace(' ', "")
                .contains(&title.replace(' ', ""))
        );
        assert_eq!(app.config.header_height, 5);
        let saved = app.store.config()?;
        assert_eq!(
            (saved.board_width, saved.thread_width, saved.header_height),
            (36, 39, 5)
        );
        let z = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        app.key(z)?;
        let mut narrow = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))?;
        narrow.draw(|f| draw(f, &mut app))?;
        assert!(app.areas.iter().all(|a| a.width > 0));
        app.key(z)?;
        narrow.draw(|f| draw(f, &mut app))?;
        assert_eq!(app.config.layout, "two");
        assert_eq!(app.areas[1].x, app.areas[2].x);
        assert!(app.areas[1].y < app.areas[2].y);
        let divider = app.dividers[1];
        let column = divider.x + divider.width / 2;
        let original = app.areas[1].height;
        app.mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            column,
            divider.y,
        ))?;
        app.mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            column,
            divider.y + 1,
        ))?;
        app.mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            column,
            divider.y + 1,
        ))?;
        narrow.draw(|f| draw(f, &mut app))?;
        assert_eq!(app.areas[1].height, original + 1);
        assert_eq!(app.store.config()?.thread_height, original + 1);
        app.key(z)?;
        narrow.draw(|f| draw(f, &mut app))?;
        assert_eq!(app.areas.iter().filter(|a| a.width > 0).count(), 1);
        app.key(z)?;
        assert_eq!(app.config.layout, "auto");
        app.config.board_width = u16::MAX;
        app.config.thread_width = u16::MAX;
        app.config.header_height = u16::MAX;
        for (w, h) in [(120, 36), (60, 16), (30, 10)] {
            let mut t = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h))?;
            t.draw(|f| draw(f, &mut app))?;
        }
        drop(app);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
