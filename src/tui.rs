use std::{
    env,
    io::{self, Stdout},
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use crate::{
    git::{self, GitIdentity},
    profiles::{Profile, ProfileStore, Profiles},
};

const FIELDS: [&str; 6] = [
    "Profile name",
    "Git author name",
    "Git author email",
    "Commit signing",
    "Signing key",
    "SSH host alias",
];
const MIN_WIDTH: u16 = 52;
const MIN_HEIGHT: u16 = 22;
const WIDE_WIDTH: u16 = 90;

struct Theme;

impl Theme {
    const ACCENT: Color = Color::Cyan;
    const SUCCESS: Color = Color::Green;
    const WARNING: Color = Color::Yellow;
    const ERROR: Color = Color::Red;
    const MUTED: Color = Color::DarkGray;

    fn border() -> Style {
        Style::default().fg(Self::MUTED)
    }

    fn focused_border() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    fn label() -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    fn muted() -> Style {
        Style::default().fg(Self::MUTED)
    }

    fn key() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    fn selected() -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    fn focused_label() -> Style {
        Self::selected()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StatusMessage {
    level: StatusLevel,
    text: String,
}

impl StatusMessage {
    fn icon(&self) -> &'static str {
        match self.level {
            StatusLevel::Info => "•",
            StatusLevel::Success => "✓",
            StatusLevel::Warning => "!",
            StatusLevel::Error => "✕",
        }
    }

    fn style(&self) -> Style {
        Style::default().fg(match self.level {
            StatusLevel::Info => Theme::MUTED,
            StatusLevel::Success => Theme::SUCCESS,
            StatusLevel::Warning => Theme::WARNING,
            StatusLevel::Error => Theme::ERROR,
        })
    }
}

pub fn run(store: ProfileStore) -> Result<()> {
    let mut stdout = io::stdout();
    let _guard = TerminalGuard::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("could not initialize terminal")?;
    terminal.clear().context("could not clear terminal")?;

    let mut app = App::new(store)?;
    let result = run_loop(&mut terminal, &mut app);
    terminal.show_cursor().ok();
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal
            .draw(|frame| render(frame, app))
            .context("could not draw TUI")?;
        if event::poll(Duration::from_millis(250)).context("could not poll terminal events")?
            && let Event::Key(key) = event::read().context("could not read terminal event")?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key);
        }
    }
    Ok(())
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter(stdout: &mut Stdout) -> Result<Self> {
        enable_raw_mode().context("could not enable terminal raw mode")?;
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            disable_raw_mode().ok();
            return Err(error).context("could not enter alternate screen");
        }
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        execute!(stdout, Show, LeaveAlternateScreen).ok();
        disable_raw_mode().ok();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FormMode {
    Add,
    Edit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Form {
    mode: FormMode,
    field: usize,
    profile_name: String,
    author_name: String,
    email: String,
    signing_enabled: bool,
    signing_key: String,
    ssh_host: String,
}

impl Form {
    fn add() -> Self {
        Self {
            mode: FormMode::Add,
            field: 0,
            profile_name: String::new(),
            author_name: String::new(),
            email: String::new(),
            signing_enabled: false,
            signing_key: String::new(),
            ssh_host: String::new(),
        }
    }

    fn edit(name: String, profile: Profile) -> Self {
        Self {
            mode: FormMode::Edit,
            field: 1,
            profile_name: name,
            author_name: profile.name,
            email: profile.email,
            signing_enabled: profile.signing_key.is_some(),
            signing_key: profile.signing_key.unwrap_or_default(),
            ssh_host: profile.ssh_host.unwrap_or_default(),
        }
    }

    fn profile(&self) -> Profile {
        Profile {
            name: self.author_name.clone(),
            email: self.email.clone(),
            signing_key: self.signing_enabled.then(|| self.signing_key.clone()),
            ssh_host: (!self.ssh_host.is_empty()).then(|| self.ssh_host.clone()),
        }
    }

    fn next_field(&mut self) {
        loop {
            self.field = (self.field + 1) % FIELDS.len();
            if self.field_is_editable(self.field) {
                return;
            }
        }
    }

    fn previous_field(&mut self) {
        loop {
            self.field = self.field.checked_sub(1).unwrap_or(FIELDS.len() - 1);
            if self.field_is_editable(self.field) {
                return;
            }
        }
    }

    fn field_is_editable(&self, field: usize) -> bool {
        !(self.mode == FormMode::Edit && field == 0) && (field != 4 || self.signing_enabled)
    }

    fn value_mut(&mut self) -> Option<&mut String> {
        match self.field {
            0 if self.mode == FormMode::Add => Some(&mut self.profile_name),
            1 => Some(&mut self.author_name),
            2 => Some(&mut self.email),
            4 if self.signing_enabled => Some(&mut self.signing_key),
            5 => Some(&mut self.ssh_host),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Modal {
    Form(Form),
    SshHostPicker {
        form: Form,
        aliases: Vec<String>,
        selected: usize,
    },
    ConfirmDelete(String),
}

struct App {
    store: ProfileStore,
    profiles: Profiles,
    names: Vec<String>,
    selected: usize,
    current: Option<GitIdentity>,
    current_error: Option<String>,
    current_dir: String,
    scope: String,
    status: Option<StatusMessage>,
    modal: Option<Modal>,
    should_quit: bool,
}

impl App {
    fn new(store: ProfileStore) -> Result<Self> {
        let profiles = store.load()?;
        let names = profiles.profiles.keys().cloned().collect();
        let current_dir = env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| "unknown directory".to_owned());
        let scope = git::current_scope_label().unwrap_or("LOCAL").to_owned();
        let mut app = Self {
            store,
            profiles,
            names,
            selected: 0,
            current: None,
            current_error: None,
            current_dir,
            scope,
            status: None,
            modal: None,
            should_quit: false,
        };
        app.refresh_identity();
        Ok(app)
    }

    fn selected_name(&self) -> Option<&str> {
        self.names.get(self.selected).map(String::as_str)
    }

    fn selected_profile(&self) -> Option<&Profile> {
        self.selected_name()
            .and_then(|name| self.profiles.profiles.get(name))
    }

    fn active_name(&self) -> Option<&str> {
        let identity = self.current.as_ref()?;
        self.profiles
            .profiles
            .iter()
            .find(|(_, profile)| identity.matches(profile))
            .map(|(name, _)| name.as_str())
    }

    fn refresh(&mut self, preferred: Option<&str>) -> Result<()> {
        self.profiles = self.store.load()?;
        self.names = self.profiles.profiles.keys().cloned().collect();
        self.selected = preferred
            .and_then(|name| self.names.iter().position(|item| item == name))
            .unwrap_or_else(|| self.selected.min(self.names.len().saturating_sub(1)));
        self.refresh_identity();
        Ok(())
    }

    fn refresh_identity(&mut self) {
        match git::read_current_identity() {
            Ok(identity) => {
                self.current = Some(identity);
                self.current_error = None;
            }
            Err(error) => {
                self.current = None;
                self.current_error = Some(format!("{error:#}"));
                if self.status.is_none() {
                    self.status = Some(StatusMessage {
                        level: StatusLevel::Warning,
                        text: "Git identity is unavailable in this directory".to_owned(),
                    });
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.modal.take() {
            Some(Modal::Form(mut form)) => {
                if key.code == KeyCode::Char(' ') && form.field == 5 {
                    self.open_ssh_host_picker(form);
                } else if !self.handle_form_key(&mut form, key) {
                    self.modal = Some(Modal::Form(form));
                }
            }
            Some(Modal::SshHostPicker {
                form,
                aliases,
                selected,
            }) => self.handle_ssh_host_picker(form, aliases, selected, key),
            Some(Modal::ConfirmDelete(name)) => {
                self.handle_delete_key(name, key);
            }
            None => self.handle_main_key(key),
        }
    }

    fn handle_main_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.names.len().saturating_sub(1));
            }
            KeyCode::Char('a') => {
                self.status = None;
                self.modal = Some(Modal::Form(Form::add()));
            }
            KeyCode::Char('e') => {
                if let Some((name, profile)) = self
                    .selected_name()
                    .zip(self.selected_profile())
                    .map(|(name, profile)| (name.to_owned(), profile.clone()))
                {
                    self.status = None;
                    self.modal = Some(Modal::Form(Form::edit(name, profile)));
                }
            }
            KeyCode::Char('d') => {
                if let Some(name) = self.selected_name() {
                    self.modal = Some(Modal::ConfirmDelete(name.to_owned()));
                }
            }
            KeyCode::Enter | KeyCode::Char('u') => self.apply_selected(),
            _ => {}
        }
    }

    fn handle_form_key(&mut self, form: &mut Form, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => true,
            KeyCode::Tab | KeyCode::Down => {
                form.next_field();
                false
            }
            KeyCode::BackTab | KeyCode::Up => {
                form.previous_field();
                false
            }
            KeyCode::Char(' ') if form.field == 3 => {
                form.signing_enabled = !form.signing_enabled;
                false
            }
            KeyCode::Enter => {
                if form.field == 3 {
                    form.signing_enabled = !form.signing_enabled;
                    false
                } else {
                    self.save_form(form)
                }
            }
            KeyCode::Backspace => {
                if let Some(value) = form.value_mut() {
                    value.pop();
                }
                false
            }
            KeyCode::Char(character) => {
                if let Some(value) = form.value_mut() {
                    value.push(character);
                }
                false
            }
            _ => false,
        }
    }

    fn open_ssh_host_picker(&mut self, form: Form) {
        match git::ssh_host_aliases() {
            Ok(aliases) if aliases.is_empty() => {
                self.set_error("No literal SSH host aliases were found in ~/.ssh/config");
                self.modal = Some(Modal::Form(form));
            }
            Ok(aliases) => {
                let selected = aliases
                    .iter()
                    .position(|alias| alias == &form.ssh_host)
                    .unwrap_or(0);
                self.modal = Some(Modal::SshHostPicker {
                    form,
                    aliases,
                    selected,
                });
            }
            Err(error) => {
                self.set_error(error);
                self.modal = Some(Modal::Form(form));
            }
        }
    }

    fn handle_ssh_host_picker(
        &mut self,
        mut form: Form,
        aliases: Vec<String>,
        mut selected: usize,
        key: KeyEvent,
    ) {
        match key.code {
            KeyCode::Esc => self.modal = Some(Modal::Form(form)),
            KeyCode::Up | KeyCode::Char('k') => {
                selected = selected.saturating_sub(1);
                self.modal = Some(Modal::SshHostPicker {
                    form,
                    aliases,
                    selected,
                });
            }
            KeyCode::Down | KeyCode::Char('j') => {
                selected = (selected + 1).min(aliases.len().saturating_sub(1));
                self.modal = Some(Modal::SshHostPicker {
                    form,
                    aliases,
                    selected,
                });
            }
            KeyCode::Enter => {
                form.ssh_host = aliases[selected].clone();
                self.modal = Some(Modal::Form(form));
            }
            _ => {
                self.modal = Some(Modal::SshHostPicker {
                    form,
                    aliases,
                    selected,
                });
            }
        }
    }

    fn save_form(&mut self, form: &Form) -> bool {
        let result = match form.mode {
            FormMode::Add => self.store.add(form.profile_name.clone(), form.profile()),
            FormMode::Edit => self.store.update(&form.profile_name, form.profile()),
        };
        match result {
            Ok(()) => {
                let action = if form.mode == FormMode::Add {
                    "Added"
                } else {
                    "Updated"
                };
                let name = form.profile_name.clone();
                match self.refresh(Some(&name)) {
                    Ok(()) => self.set_success(format!("{action} profile '{name}'")),
                    Err(error) => self.set_error(error),
                }
                true
            }
            Err(error) => {
                self.set_error(error);
                false
            }
        }
    }

    fn handle_delete_key(&mut self, name: String, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') => match self.store.remove(&name) {
                Ok(()) => match self.refresh(None) {
                    Ok(()) => self.set_success(format!("Removed profile '{name}'")),
                    Err(error) => self.set_error(error),
                },
                Err(error) => self.set_error(error),
            },
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {}
            _ => self.modal = Some(Modal::ConfirmDelete(name)),
        }
    }

    fn apply_selected(&mut self) {
        let Some((name, profile)) = self
            .selected_name()
            .zip(self.selected_profile())
            .map(|(name, profile)| (name.to_owned(), profile.clone()))
        else {
            return;
        };
        match git::apply_profile(&profile) {
            Ok(path) => {
                self.refresh_identity();
                self.set_success(format!("Applied '{name}' to {path}"));
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_success(&mut self, message: String) {
        self.status = Some(StatusMessage {
            level: StatusLevel::Success,
            text: message,
        });
    }

    fn set_error(&mut self, error: impl std::fmt::Display) {
        self.status = Some(StatusMessage {
            level: StatusLevel::Error,
            text: format!("{error:#}"),
        });
    }
}

fn render(frame: &mut Frame, app: &App) {
    if frame.area().width < MIN_WIDTH || frame.area().height < MIN_HEIGHT {
        render_too_small(frame);
        return;
    }

    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, app, areas[0]);
    if frame.area().width >= WIDE_WIDTH {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(areas[1]);
        render_profiles(frame, app, columns[0]);
        render_details(frame, app, columns[1]);
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(areas[1]);
        render_profiles(frame, app, rows[0]);
        render_details(frame, app, rows[1]);
    }
    render_status(frame, app, areas[2]);
    render_help(frame, app, areas[3]);

    if let Some(modal) = &app.modal {
        match modal {
            Modal::Form(form) => render_form(frame, form, app.status.as_ref()),
            Modal::SshHostPicker {
                aliases, selected, ..
            } => render_ssh_host_picker(frame, aliases, *selected),
            Modal::ConfirmDelete(name) => render_confirmation(frame, name),
        }
    }
}

fn render_too_small(frame: &mut Frame) {
    let message = Paragraph::new(vec![
        Line::styled("gcs", Theme::key()),
        Line::from(""),
        Line::from("Terminal is too small"),
        Line::styled(
            format!("Resize to at least {MIN_WIDTH}×{MIN_HEIGHT}"),
            Theme::muted(),
        ),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Theme::border()),
    );
    frame.render_widget(message, frame.area());
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Theme::border());
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" gcs ", Theme::key()),
            Span::styled("Git identity profiles", Theme::muted()),
        ])),
        sections[0],
    );

    let active = match app.active_name() {
        Some(name) => vec![
            Span::styled("● Active ", Style::default().fg(Theme::SUCCESS)),
            Span::styled(name, Theme::label()),
        ],
        None => vec![Span::styled("○ Unmanaged", Theme::muted())],
    };
    let directory = compact_path(&app.current_dir, 28);
    let mut right = vec![
        Span::styled(format!(" {} ", app.scope), Theme::key()),
        Span::styled(format!("{directory}  "), Theme::muted()),
    ];
    right.extend(active);
    frame.render_widget(
        Paragraph::new(Line::from(right)).alignment(Alignment::Right),
        sections[1],
    );
}

fn render_profiles(frame: &mut Frame, app: &App, area: Rect) {
    if app.names.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::styled("No profiles yet", Theme::label()),
            Line::from(""),
            key_help("a", "Add your first profile"),
        ])
        .alignment(Alignment::Center)
        .block(panel(" Profiles "));
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .names
        .iter()
        .map(|name| {
            let active = app.current.as_ref().is_some_and(|identity| {
                app.profiles
                    .profiles
                    .get(name)
                    .is_some_and(|p| identity.matches(p))
            });
            let marker = if active {
                Span::styled("● ", Style::default().fg(Theme::SUCCESS))
            } else {
                Span::raw("  ")
            };
            ListItem::new(Line::from(vec![marker, Span::raw(name)]))
        })
        .collect();
    let title = format!(" Profiles ({}) ", app.names.len());
    let list = List::new(items)
        .block(panel(title))
        .highlight_style(Theme::selected())
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !app.names.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_details(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    if let Some(profile) = app.selected_profile() {
        lines.extend([
            section_title("Selected profile"),
            Line::from(""),
            Line::from(vec![
                Span::styled("Name         ", Theme::muted()),
                Span::raw(&profile.name),
            ]),
            Line::from(vec![
                Span::styled("Email        ", Theme::muted()),
                Span::raw(&profile.email),
            ]),
            Line::from(vec![
                Span::styled("Signing      ", Theme::muted()),
                Span::styled(
                    if profile.signing_key.is_some() {
                        "✓ Enabled"
                    } else {
                        "— Disabled"
                    },
                    if profile.signing_key.is_some() {
                        Style::default().fg(Theme::SUCCESS)
                    } else {
                        Theme::muted()
                    },
                ),
            ]),
            Line::from(vec![
                Span::styled("Signing key  ", Theme::muted()),
                optional_value(profile.signing_key.as_deref()),
            ]),
            Line::from(vec![
                Span::styled("SSH host     ", Theme::muted()),
                optional_value(profile.ssh_host.as_deref()),
            ]),
            Line::from(""),
        ]);
    }
    lines.extend([section_title("Current Git identity"), Line::from("")]);
    if let Some(identity) = &app.current {
        lines.extend([
            identity_line("Name", identity.name.as_deref()),
            identity_line("Email", identity.email.as_deref()),
            identity_line("Signing key", identity.signing_key.as_deref()),
        ]);
    } else {
        lines.push(Line::styled(
            app.current_error.as_deref().unwrap_or("Unavailable"),
            Style::default().fg(Theme::WARNING),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(" Details "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let fallback = StatusMessage {
        level: StatusLevel::Info,
        text: "Ready".to_owned(),
    };
    let status = app.status.as_ref().unwrap_or(&fallback);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", status.icon()), status.style()),
            Span::styled(&status.text, status.style()),
        ]))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Theme::border()),
        )
        .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.modal {
        Some(Modal::Form(_)) => help_line(&[
            ("Tab", "next"),
            ("↑/↓", "field"),
            ("Space", "toggle/select SSH host"),
            ("Enter", "save"),
            ("Esc", "cancel"),
        ]),
        Some(Modal::SshHostPicker { .. }) => {
            help_line(&[("↑/↓ j/k", "select"), ("Enter", "use"), ("Esc", "cancel")])
        }
        Some(Modal::ConfirmDelete(_)) => help_line(&[("y", "confirm"), ("n/Esc", "cancel")]),
        None => help_line(&[
            ("↑/↓ j/k", "navigate"),
            ("Enter/u", "apply"),
            ("a", "add"),
            ("e", "edit"),
            ("d", "delete"),
            ("q", "quit"),
        ]),
    };
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn render_form(frame: &mut Frame, form: &Form, status: Option<&StatusMessage>) {
    let area = centered_rect(78, 22, frame.area());
    render_modal_backdrop(frame);
    frame.render_widget(Clear, area);
    let title = if form.mode == FormMode::Add {
        " Add profile "
    } else {
        " Edit profile "
    };
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::focused_border())
        .title(Span::styled(title, Theme::key()))
        .inner(area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Theme::focused_border())
            .title(Span::styled(title, Theme::key())),
        area,
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(2),
        ])
        .split(inner);
    let values = [
        form.profile_name.as_str(),
        form.author_name.as_str(),
        form.email.as_str(),
        "",
        form.signing_key.as_str(),
        form.ssh_host.as_str(),
    ];
    for (index, row) in rows.iter().take(6).enumerate() {
        if index == 3 {
            let toggle = if form.signing_enabled {
                "◉ Enabled"
            } else {
                "○ Disabled"
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("Commit signing  ", Theme::muted()),
                    Span::styled(
                        toggle,
                        if form.field == index {
                            Theme::key()
                        } else {
                            Style::default()
                        },
                    ),
                ])),
                *row,
            );
            continue;
        }
        let read_only = form.mode == FormMode::Edit && index == 0;
        let disabled = index == 4 && !form.signing_enabled;
        render_input(
            frame,
            *row,
            if index == 5 {
                "SSH host alias (Space to choose)"
            } else {
                FIELDS[index]
            },
            values[index],
            form.field == index,
            read_only,
            disabled,
        );
    }

    if let Some(message) = status.filter(|message| message.level == StatusLevel::Error) {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("✕ ", message.style()),
                Span::styled(&message.text, message.style()),
            ]))
            .wrap(Wrap { trim: true }),
            rows[6],
        );
    }
}

fn render_ssh_host_picker(frame: &mut Frame, aliases: &[String], selected: usize) {
    let visible = aliases.len().min(8);
    let height = ssh_host_picker_height(aliases.len(), frame.area().height);
    let area = centered_rect(58, height, frame.area());
    render_modal_backdrop(frame);
    frame.render_widget(Clear, area);
    let start = selected.saturating_sub(visible.saturating_sub(1));
    let items = aliases[start..]
        .iter()
        .take(visible)
        .map(|alias| ListItem::new(alias.as_str()))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Theme::focused_border())
                .title(Span::styled(" Select SSH host alias ", Theme::key())),
        )
        .highlight_style(Theme::selected())
        .highlight_symbol("› ");
    let mut state = ListState::default();
    state.select(Some(selected - start));
    frame.render_stateful_widget(list, area, &mut state);

    if aliases.len() > visible {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Theme::focused_border())
            .track_style(Theme::muted());
        let scroll_positions = aliases.len() - visible + 1;
        let mut scrollbar_state = ScrollbarState::new(scroll_positions)
            .position(start)
            .viewport_content_length(visible);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn ssh_host_picker_height(alias_count: usize, available_height: u16) -> u16 {
    (alias_count.min(8) as u16 + 2).min(available_height)
}

fn render_input(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    focused: bool,
    read_only: bool,
    disabled: bool,
) {
    let suffix = if read_only { "  locked" } else { "" };
    let title = Line::from(vec![
        Span::styled(
            format!(" {label} "),
            if focused {
                Theme::focused_label()
            } else {
                Theme::muted()
            },
        ),
        Span::styled(suffix, Theme::muted()),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if focused {
            Theme::focused_border()
        } else {
            Theme::border()
        })
        .title(title);
    let inner = block.inner(area);
    let available = inner.width.saturating_sub(1) as usize;
    let shown = if disabled {
        "Disabled".to_owned()
    } else {
        tail_chars(value, available)
    };
    frame.render_widget(
        Paragraph::new(shown.as_str()).style(if disabled || read_only {
            Theme::muted()
        } else {
            Style::default()
        }),
        inner,
    );
    frame.render_widget(block, area);
    if focused && !read_only && !disabled {
        let cursor = shown.chars().count().min(available) as u16;
        frame.set_cursor_position((inner.x + cursor, inner.y));
    }
}

fn render_confirmation(frame: &mut Frame, name: &str) {
    let area = centered_rect(58, 9, frame.area());
    render_modal_backdrop(frame);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::styled("Delete this profile?", Theme::label()),
            Line::from(""),
            Line::styled(
                name,
                Style::default()
                    .fg(Theme::ERROR)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            help_line(&[("y", "confirm"), ("n/Esc", "cancel")]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Theme::WARNING))
                .title(Span::styled(
                    " Confirm delete ",
                    Style::default().fg(Theme::WARNING),
                )),
        ),
        area,
    );
}

fn render_modal_backdrop(frame: &mut Frame) {
    let area = frame.area();
    frame
        .buffer_mut()
        .set_style(area, Style::default().fg(Theme::MUTED));
}

fn panel<'a>(title: impl Into<Line<'a>>) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border())
        .title(title)
}

fn section_title(title: &str) -> Line<'_> {
    Line::styled(title, Theme::key())
}

fn optional_value(value: Option<&str>) -> Span<'_> {
    match value {
        Some(value) => Span::raw(value),
        None => Span::styled("—", Theme::muted()),
    }
}

fn identity_line<'a>(name: &'a str, value: Option<&'a str>) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{name:<12}"), Theme::muted()),
        optional_value(value),
    ])
}

fn key_help<'a>(key: &'a str, description: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("[{key}]"), Theme::key()),
        Span::styled(format!(" {description}"), Theme::muted()),
    ])
}

fn help_line<'a>(items: &[(&'a str, &'a str)]) -> Line<'a> {
    let mut spans = Vec::new();
    for (index, (key, description)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(format!("[{key}]"), Theme::key()));
        spans.push(Span::styled(format!(" {description}"), Theme::muted()));
    }
    Line::from(spans)
}

fn tail_chars(value: &str, width: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    let start = chars.len().saturating_sub(width);
    chars[start..].iter().collect()
}

fn compact_path(value: &str, width: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= width {
        return value.to_owned();
    }
    let suffix_width = width.saturating_sub(1);
    format!(
        "…{}",
        chars[chars.len() - suffix_width..]
            .iter()
            .collect::<String>()
    )
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(height) / 2),
            Constraint::Length(height.min(area.height)),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use tempfile::tempdir;

    fn app() -> App {
        let directory = tempdir().unwrap();
        let store = ProfileStore::new(directory.keep().join("config.toml"));
        App::new(store).unwrap()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, crossterm::event::KeyModifiers::NONE)
    }

    #[test]
    fn navigation_stays_within_bounds() {
        let mut app = app();
        app.store
            .add(
                "a".into(),
                Profile {
                    name: "A".into(),
                    email: "a@example.com".into(),
                    signing_key: None,
                    ssh_host: None,
                },
            )
            .unwrap();
        app.store
            .add(
                "b".into(),
                Profile {
                    name: "B".into(),
                    email: "b@example.com".into(),
                    signing_key: None,
                    ssh_host: None,
                },
            )
            .unwrap();
        app.refresh(None).unwrap();
        app.handle_key(key(KeyCode::Up));
        assert_eq!(app.selected, 0);
        app.handle_key(key(KeyCode::Char('j')));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn add_form_validates_and_saves() {
        let mut app = app();
        let mut form = Form::add();
        assert!(!app.save_form(&form));
        assert_eq!(app.status.as_ref().unwrap().level, StatusLevel::Error);
        form.profile_name = "work".into();
        form.author_name = "Ada".into();
        form.email = "ada@example.com".into();
        assert!(app.save_form(&form));
        assert!(app.profiles.profiles.contains_key("work"));
    }

    #[test]
    fn edit_form_keeps_profile_name_read_only() {
        let profile = Profile {
            name: "Ada".into(),
            email: "ada@example.com".into(),
            signing_key: None,
            ssh_host: None,
        };
        let mut form = Form::edit("work".into(), profile);
        form.previous_field();
        assert_ne!(form.field, 0);
    }

    #[test]
    fn form_maps_ssh_host_to_profile() {
        let mut form = Form::add();
        form.ssh_host = "github-work".into();
        assert_eq!(form.profile().ssh_host.as_deref(), Some("github-work"));
    }

    #[test]
    fn ssh_host_picker_applies_the_selected_alias() {
        let mut app = app();
        app.handle_ssh_host_picker(
            Form::add(),
            vec!["github-personal".into(), "github-work".into()],
            0,
            key(KeyCode::Down),
        );
        app.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            app.modal,
            Some(Modal::Form(Form {
                ref ssh_host,
                ..
            })) if ssh_host == "github-work"
        ));
    }

    #[test]
    fn ssh_host_picker_height_fits_visible_aliases() {
        assert_eq!(ssh_host_picker_height(3, 30), 5);
        assert_eq!(ssh_host_picker_height(12, 30), 10);
        assert_eq!(ssh_host_picker_height(12, 7), 7);
    }

    #[test]
    fn ssh_host_picker_shows_scrollbar_for_overflow() {
        let aliases = (1..=10)
            .map(|index| format!("github-{index}"))
            .collect::<Vec<_>>();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_ssh_host_picker(frame, &aliases, 0))
            .unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains('▲'));
        assert!(screen.contains('▼'));

        terminal
            .draw(|frame| render_ssh_host_picker(frame, &aliases, aliases.len() - 1))
            .unwrap();
        let area = centered_rect(
            58,
            ssh_host_picker_height(aliases.len(), 30),
            Rect::new(0, 0, 100, 30),
        );
        let bottom_of_track = (area.right() - 1, area.bottom() - 3);
        assert_eq!(
            terminal
                .backend()
                .buffer()
                .cell(bottom_of_track)
                .unwrap()
                .symbol(),
            "█"
        );
    }

    #[test]
    fn delete_requires_confirmation() {
        let mut app = app();
        app.store
            .add(
                "work".into(),
                Profile {
                    name: "Ada".into(),
                    email: "a@example.com".into(),
                    signing_key: None,
                    ssh_host: None,
                },
            )
            .unwrap();
        app.refresh(None).unwrap();
        app.handle_key(key(KeyCode::Char('d')));
        assert!(matches!(app.modal, Some(Modal::ConfirmDelete(_))));
        app.handle_key(key(KeyCode::Char('n')));
        assert!(app.store.get("work").is_ok());
    }

    #[test]
    fn renders_empty_and_populated_states() {
        let mut app = app();
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let empty = terminal.backend().to_string();
        assert!(empty.contains("No profiles yet"));

        app.store
            .add(
                "work".into(),
                Profile {
                    name: "Ada".into(),
                    email: "ada@example.com".into(),
                    signing_key: None,
                    ssh_host: None,
                },
            )
            .unwrap();
        app.refresh(None).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let populated = terminal.backend().to_string();
        assert!(populated.contains("work"));
        assert!(populated.contains("ada@example.com"));
    }

    #[test]
    fn renders_header_and_active_profile() {
        let mut app = app();
        app.current_dir =
            "/Users/runner/work/a-very-long-repository-name/a-very-long-repository-name".into();
        let profile = Profile {
            name: "Ada".into(),
            email: "ada@example.com".into(),
            signing_key: None,
            ssh_host: None,
        };
        app.store.add("work".into(), profile.clone()).unwrap();
        app.refresh(None).unwrap();
        app.current = Some(GitIdentity {
            name: Some(profile.name),
            email: Some(profile.email),
            ..GitIdentity::default()
        });

        let backend = TestBackend::new(110, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("Git identity profiles"));
        assert!(screen.contains("Active"));
        assert!(screen.contains("work"));
        assert!(screen.contains(&app.scope));
        assert!(screen.contains('…'));
    }

    #[test]
    fn narrow_and_minimum_layouts_render_safely() {
        let app = app();
        for (width, height) in [(70, 28), (40, 12)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal.draw(|frame| render(frame, &app)).unwrap();
            let screen = terminal.backend().to_string();
            if width < MIN_WIDTH {
                assert!(screen.contains("Terminal is too small"));
            } else {
                assert!(screen.contains("Profiles"));
                assert!(screen.contains("Details"));
            }
        }
    }

    #[test]
    fn status_and_long_form_values_are_rendered() {
        let mut app = app();
        app.status = Some(StatusMessage {
            level: StatusLevel::Error,
            text: "Could not save".into(),
        });
        let mut form = Form::add();
        form.email = "a-very-long-address-that-must-scroll@example.com".into();
        form.field = 2;
        app.modal = Some(Modal::Form(form));

        let backend = TestBackend::new(80, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let screen = terminal.backend().to_string();
        assert!(screen.contains("Could not save"));
        assert!(screen.contains("example.com"));
    }

    #[test]
    fn status_levels_use_distinct_icons() {
        let mut app = app();
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        for (level, icon) in [
            (StatusLevel::Info, "•"),
            (StatusLevel::Success, "✓"),
            (StatusLevel::Warning, "!"),
            (StatusLevel::Error, "✕"),
        ] {
            app.status = Some(StatusMessage {
                level,
                text: "Status".into(),
            });
            terminal.draw(|frame| render(frame, &app)).unwrap();
            assert!(terminal.backend().to_string().contains(icon));
        }
    }

    #[test]
    fn tail_chars_keeps_the_visible_suffix() {
        assert_eq!(tail_chars("long@example.com", 7), "ple.com");
    }

    #[test]
    fn compact_path_preserves_short_values_and_truncates_long_ones() {
        assert_eq!(compact_path("/tmp/repo", 20), "/tmp/repo");
        assert_eq!(compact_path("/very/long/path/to/repo", 10), "…h/to/repo");
    }
}
