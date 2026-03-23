use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame,
};
use super::{App, PopupState};

const TABS: &[&str] = &["Skills", "Agents", "Instructions", "Hooks", "Workflows", "Git", "Repos"];

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Tab bar
    let tab_titles: Vec<Line> = TABS.iter().map(|t| Line::from(*t)).collect();
    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).title("ai-manager"))
        .select(app.tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[0]);

    // Content
    match app.tab {
        0..=4 => draw_resource_tab(f, app, chunks[1]),
        5 => draw_git_tab(f, app, chunks[1]),
        6 => draw_repo_tab(f, app, chunks[1]),
        _ => {}
    }

    // Status bar
    let status_text = if app.filter_mode {
        format!("Filter: {}_  (Esc to exit)", app.filter)
    } else if let Some(msg) = &app.message {
        msg.clone()
    } else {
        "Tab: switch tab | ↑↓/PgUp/PgDn: navigate | Enter: toggle | s: sync/local | u: update | /: filter | q: quit".to_string()
    };
    let status = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(status, chunks[2]);

    // Popup
    if let Some(popup) = app.popup.clone() {
        draw_popup(f, &popup);
    }
}

fn draw_resource_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    // Update page size based on visible area (subtract 2 for borders)
    app.page_size = (area.height.saturating_sub(2)) as usize;

    // Available width for content: area width minus borders (2) and highlight symbol "▶ " (2)
    let content_width = area.width.saturating_sub(4) as usize;

    let items: Vec<ListItem> = app.items.iter().map(|item| {
        let installed_sym = if item.is_installed {
            if item.link_exists { "✓" } else { "✗" }
        } else { "·" };
        let sync = if item.is_installed {
            if item.is_local { " [local]" } else { " [sync]" }
        } else { "" };
        let style = if item.is_installed {
            if item.link_exists { Style::default().fg(Color::Green) }
            else { Style::default().fg(Color::Red) }
        } else {
            Style::default().fg(Color::Gray)
        };

        // Calculate how much space is left for source_value
        // Format: "X key = source_value[sync]"
        let prefix_len = 2 + item.key.len() + 3; // "X " + key + " = "
        let suffix_len = sync.len();
        let available = content_width.saturating_sub(prefix_len + suffix_len);
        let source_display = truncate_str(&item.source_value, available);

        ListItem::new(
            Line::from(vec![
                Span::styled(format!("{} ", installed_sym), style),
                Span::styled(item.key.clone(), Style::default().fg(Color::White)),
                Span::styled(format!(" = {}", source_display), Style::default().fg(Color::Cyan)),
                Span::styled(sync.to_string(), Style::default().fg(Color::Yellow)),
            ])
        )
    }).collect();

    let tab_name = TABS.get(app.tab).copied().unwrap_or("Resources");
    let title = if app.filter.is_empty() {
        tab_name.to_string()
    } else {
        format!("{} [filter: {}]", tab_name, app.filter)
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_git_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let text = if app.git_status.is_empty() {
        "Press 'r' to refresh git status".to_string()
    } else {
        app.git_status.clone()
    };
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Git Status"))
        .wrap(ratatui::widgets::Wrap { trim: false });
    f.render_widget(p, area);
}

fn draw_repo_tab(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    use crate::repo;
    let all_repos: Vec<_> = app.shared_config.repos.iter()
        .chain(app.local_config.repos.iter())
        .collect();
    let items: Vec<ListItem> = all_repos.iter().map(|repo| {
        let cloned = if repo::is_cloned(&app.root, &repo.name) { "✓" } else { " " };
        ListItem::new(Line::from(vec![
            Span::styled(format!("[{}] ", cloned), Style::default().fg(Color::Green)),
            Span::styled(repo.name.clone(), Style::default().fg(Color::White)),
            Span::styled(format!(" — {}", repo.url), Style::default().fg(Color::Gray)),
        ]))
    }).collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Repos"))
        .highlight_style(Style::default().bg(Color::DarkGray));
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_popup(f: &mut Frame, popup: &PopupState) {
    use ratatui::layout::Rect;
    let area = f.area();
    let popup_width = 60.min(area.width.saturating_sub(4));
    let popup_height = 7;
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    let block = Block::default()
        .title("Enter key name")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::DarkGray));

    let inner = block.inner(popup_area);
    f.render_widget(ratatui::widgets::Clear, popup_area);
    f.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let hint = Paragraph::new(format!("Source: {}", popup.source_value));
    f.render_widget(hint, chunks[0]);

    let input = Paragraph::new(format!("Key: {}_", popup.current_key));
    f.render_widget(input, chunks[1]);

    let help = Paragraph::new("Enter: confirm | Esc: cancel").style(Style::default().fg(Color::Gray));
    f.render_widget(help, chunks[2]);
}

/// Truncate a string to fit within `max_chars` Unicode characters, appending "…" if truncated.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}
