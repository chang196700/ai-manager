mod events;
pub mod ui;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, widgets::ListState, Terminal};
use std::io::stdout;
use std::path::PathBuf;
use crossterm::event::{KeyCode, KeyModifiers};

use crate::config::{self, Config};
use crate::resource::{self, OpDesc, ResourceType};
use crate::git;

#[derive(Debug, Clone)]
pub struct ResourceItem {
    pub key: String,
    pub source_value: String,
    pub is_installed: bool,
    pub is_local: bool,
    pub link_exists: bool,
}

#[derive(Debug, Clone)]
pub struct PopupState {
    pub source_value: String,
    pub current_key: String,
    pub rtype: ResourceType,
}

pub struct App {
    pub root: PathBuf,
    pub shared_config: Config,
    pub local_config: Config,
    pub tab: usize,
    pub list_state: ListState,
    pub filter: String,
    pub filter_mode: bool,
    pub items: Vec<ResourceItem>,
    pub git_status: String,
    pub pending_ops: Vec<OpDesc>,
    pub popup: Option<PopupState>,
    pub message: Option<String>,
    pub page_size: usize,
}

impl App {
    pub fn new(root: PathBuf) -> Result<Self> {
        let shared_config = config::load_shared(&root)?;
        let local_config = config::load_local(&root)?;
        let mut app = App {
            root,
            shared_config,
            local_config,
            tab: 0,
            list_state: ListState::default(),
            filter: String::new(),
            filter_mode: false,
            items: Vec::new(),
            git_status: String::new(),
            pending_ops: Vec::new(),
            popup: None,
            message: None,
            page_size: 10,
        };
        app.refresh_items()?;
        if !app.items.is_empty() {
            app.list_state.select(Some(0));
        }
        Ok(app)
    }

    pub fn current_rtype(&self) -> Option<ResourceType> {
        match self.tab {
            0 => Some(ResourceType::Skills),
            1 => Some(ResourceType::Agents),
            2 => Some(ResourceType::Instructions),
            3 => Some(ResourceType::Hooks),
            4 => Some(ResourceType::Workflows),
            _ => None,
        }
    }

    pub fn refresh_items(&mut self) -> Result<()> {
        let rtype = match self.current_rtype() {
            Some(r) => r,
            None => return Ok(()),
        };

        let merged = config::merge(self.shared_config.clone(), self.local_config.clone());
        let filter_lower = self.filter.to_lowercase();

        // Installed items
        let mut items: Vec<ResourceItem> = resource::list_installed(&self.root, &merged, rtype)
            .into_iter()
            .map(|(key, value, link_exists)| {
                let is_local = rtype.config_map(&self.local_config).contains_key(&key);
                ResourceItem {
                    key,
                    source_value: value,
                    is_installed: true,
                    is_local,
                    link_exists,
                }
            })
            .collect();

        // Available (uninstalled) items
        if let Ok(available) = resource::list_available(&self.root, &merged, rtype) {
            for av in available {
                items.push(ResourceItem {
                    key: av.suggested_key,
                    source_value: av.source_value,
                    is_installed: false,
                    is_local: false,
                    link_exists: false,
                });
            }
        }

        // Apply filter
        if !filter_lower.is_empty() {
            items.retain(|item| {
                item.key.to_lowercase().contains(&filter_lower) ||
                item.source_value.to_lowercase().contains(&filter_lower)
            });
        }

        self.items = items;

        // Keep selection in bounds
        if let Some(sel) = self.list_state.selected() {
            if self.items.is_empty() {
                self.list_state.select(None);
            } else if sel >= self.items.len() {
                self.list_state.select(Some(self.items.len().saturating_sub(1)));
            }
        } else if !self.items.is_empty() {
            self.list_state.select(Some(0));
        }

        Ok(())
    }

    pub fn reload_config(&mut self) -> Result<()> {
        self.shared_config = config::load_shared(&self.root)?;
        self.local_config = config::load_local(&self.root)?;
        self.refresh_items()
    }
}

pub fn run(root: PathBuf) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, root);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, root: PathBuf) -> Result<()> {
    let mut app = App::new(root)?;

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        match events::read_event()? {
            events::Event::Tick => {}
            events::Event::Key(key) => {
                // Handle popup first
                if app.popup.is_some() {
                    handle_popup_key(&mut app, key.code)?;
                    continue;
                }

                // Filter mode
                if app.filter_mode {
                    handle_filter_key(&mut app, key.code)?;
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Tab => {
                        app.tab = (app.tab + 1) % 7;
                        app.list_state.select(Some(0));
                        app.refresh_items()?;
                    }
                    KeyCode::BackTab => {
                        app.tab = if app.tab == 0 { 6 } else { app.tab - 1 };
                        app.list_state.select(Some(0));
                        app.refresh_items()?;
                    }
                    KeyCode::Up => {
                        let i = match app.list_state.selected() {
                            Some(i) => if i == 0 { app.items.len().saturating_sub(1) } else { i - 1 },
                            None => 0,
                        };
                        app.list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                        let i = match app.list_state.selected() {
                            Some(i) => if i + 1 >= app.items.len() { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.list_state.select(Some(i));
                    }
                    KeyCode::PageUp => {
                        let i = match app.list_state.selected() {
                            Some(i) => i.saturating_sub(app.page_size),
                            None => 0,
                        };
                        app.list_state.select(Some(i));
                    }
                    KeyCode::PageDown => {
                        let last = app.items.len().saturating_sub(1);
                        let i = match app.list_state.selected() {
                            Some(i) => (i + app.page_size).min(last),
                            None => 0,
                        };
                        app.list_state.select(Some(i));
                    }
                    KeyCode::Enter => {
                        handle_enter(&mut app)?;
                    }
                    KeyCode::Char('s') => {
                        handle_toggle_sync(&mut app)?;
                    }
                    KeyCode::Char('u') => {
                        handle_update_repos(&mut app)?;
                    }
                    KeyCode::Char('r') => {
                        if app.tab == 5 {
                            app.git_status = git::status(&app.root).unwrap_or_else(|e| e.to_string());
                        } else {
                            app.reload_config()?;
                        }
                    }
                    KeyCode::Char('/') => {
                        app.filter_mode = true;
                        app.filter.clear();
                    }
                    KeyCode::Esc => {
                        app.filter.clear();
                        app.filter_mode = false;
                        app.refresh_items()?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn handle_popup_key(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Esc => {
            app.popup = None;
        }
        KeyCode::Enter => {
            if let Some(popup) = app.popup.take() {
                if !popup.current_key.is_empty() {
                    let root = app.root.clone();
                    let source_value = popup.source_value.clone();
                    let current_key = popup.current_key.clone();
                    let rtype = popup.rtype;
                    let op = resource::add_resource(
                        &root,
                        &mut app.shared_config,
                        &mut app.local_config,
                        rtype,
                        &source_value,
                        &current_key,
                        false,
                    );
                    match op {
                        Ok(op) => {
                            config::save_shared(&app.root, &app.shared_config)?;
                            let _ = git::auto_commit(&app.root, &[op]);
                            app.message = Some(format!("Added '{}'", current_key));
                            app.refresh_items()?;
                        }
                        Err(e) => {
                            app.message = Some(format!("Error: {}", e));
                        }
                    }
                }
            }
        }
        KeyCode::Backspace => {
            if let Some(popup) = &mut app.popup {
                popup.current_key.pop();
            }
        }
        KeyCode::Char(c) => {
            if let Some(popup) = &mut app.popup {
                popup.current_key.push(c);
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_filter_key(app: &mut App, key: KeyCode) -> Result<()> {
    match key {
        KeyCode::Esc | KeyCode::Enter => {
            app.filter_mode = false;
            app.refresh_items()?;
        }
        KeyCode::Backspace => {
            app.filter.pop();
            app.refresh_items()?;
        }
        KeyCode::Char(c) => {
            app.filter.push(c);
            app.refresh_items()?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_enter(app: &mut App) -> Result<()> {
    let sel = match app.list_state.selected() {
        Some(i) => i,
        None => return Ok(()),
    };
    if sel >= app.items.len() { return Ok(()); }

    let item = app.items[sel].clone();
    let rtype = match app.current_rtype() {
        Some(r) => r,
        None => return Ok(()),
    };

    if item.is_installed {
        // Uninstall
        let root = app.root.clone();
        let key = item.key.clone();
        match resource::remove_resource(
            &root,
            &mut app.shared_config,
            &mut app.local_config,
            rtype,
            &key,
        ) {
            Ok(op) => {
                config::save_shared(&app.root, &app.shared_config)?;
                config::save_local(&app.root, &app.local_config)?;
                let _ = git::auto_commit(&app.root, &[op]);
                app.message = Some(format!("Removed '{}'", key));
                app.refresh_items()?;
            }
            Err(e) => {
                app.message = Some(format!("Error: {}", e));
            }
        }
    } else {
        // Install - show popup to confirm/edit key
        app.popup = Some(PopupState {
            source_value: item.source_value.clone(),
            current_key: item.key.clone(),
            rtype,
        });
    }
    Ok(())
}

fn handle_toggle_sync(app: &mut App) -> Result<()> {
    let sel = match app.list_state.selected() {
        Some(i) => i,
        None => return Ok(()),
    };
    if sel >= app.items.len() { return Ok(()); }
    let item = app.items[sel].clone();
    if !item.is_installed { return Ok(()); }

    let rtype = match app.current_rtype() {
        Some(r) => r,
        None => return Ok(()),
    };

    if item.is_local {
        // Move from local to shared
        rtype.config_map_mut(&mut app.local_config).shift_remove(&item.key);
        rtype.config_map_mut(&mut app.shared_config).insert(item.key.clone(), item.source_value.clone());
        config::save_shared(&app.root, &app.shared_config)?;
        config::save_local(&app.root, &app.local_config)?;
        app.message = Some(format!("Moved '{}' to synced config", item.key));
    } else {
        // Move from shared to local
        rtype.config_map_mut(&mut app.shared_config).shift_remove(&item.key);
        rtype.config_map_mut(&mut app.local_config).insert(item.key.clone(), item.source_value.clone());
        config::save_shared(&app.root, &app.shared_config)?;
        config::save_local(&app.root, &app.local_config)?;
        app.message = Some(format!("Moved '{}' to local config", item.key));
    }
    app.refresh_items()?;
    Ok(())
}

fn handle_update_repos(app: &mut App) -> Result<()> {
    app.message = Some("Updating repos...".to_string());
    let merged = config::merge(app.shared_config.clone(), app.local_config.clone());
    for r in &merged.repos {
        let name = r.name.clone();
        let url = r.url.clone();
        if crate::repo::is_cloned(&app.root, &name) {
            let _ = crate::repo::update_repo(&app.root, &name);
        } else {
            let _ = crate::repo::clone_repo(&app.root, &name, &url);
        }
    }
    app.message = Some("Repos updated.".to_string());
    app.refresh_items()?;
    Ok(())
}
