use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use shhup::{
    HARDENED_SSHD_CONFIG, SSHD_DROPIN_PATH, connection_hint, default_key_comment, default_key_path,
    validate_username,
};

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

const MENU_ITEMS: [&str; 4] = [
    "Generate an Ed25519 SSH key",
    "Create a non-root SSH user",
    "Install hardened sshd settings",
    "Show connection command",
];

fn main() -> Result<()> {
    let mut terminal = start_terminal()?;
    let result = run_app(&mut terminal);
    stop_terminal(&mut terminal)?;
    result
}

#[derive(Debug, Clone)]
enum Screen {
    Home,
    GenerateKey {
        path: String,
        comment: String,
        field: usize,
    },
    CreateUser {
        username: String,
    },
    Connection {
        username: String,
        host: String,
        key_path: String,
        field: usize,
    },
    Confirm {
        action: Action,
    },
    Message {
        title: String,
        body: String,
    },
}

#[derive(Debug, Clone)]
enum Action {
    GenerateKey { path: String, comment: String },
    CreateUser { username: String },
    HardenSshd,
}

#[derive(Debug)]
struct App {
    selected: usize,
    screen: Screen,
    should_quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            selected: 0,
            screen: Screen::Home,
            should_quit: false,
        }
    }
}

fn run_app(terminal: &mut Tui) -> Result<()> {
    let mut app = App::default();

    while !app.should_quit {
        terminal.draw(|frame| draw(frame, &app))?;

        if event::poll(Duration::from_millis(200))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };

            if key.kind != KeyEventKind::Press {
                continue;
            }

            handle_key(terminal, &mut app, key)?;
        }
    }

    Ok(())
}

fn handle_key(terminal: &mut Tui, app: &mut App, key: KeyEvent) -> Result<()> {
    let screen = app.screen.clone();
    match screen {
        Screen::Home => handle_home_key(app, key),
        Screen::GenerateKey {
            path,
            comment,
            field,
        } => handle_generate_key(app, key, path, comment, field),
        Screen::CreateUser { username } => handle_create_user(app, key, username),
        Screen::Connection {
            username,
            host,
            key_path,
            field,
        } => handle_connection(app, key, username, host, key_path, field),
        Screen::Confirm { action } => handle_confirm(terminal, app, key, action),
        Screen::Message { .. } => {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.screen = Screen::Home,
                _ => {}
            }
            Ok(())
        }
    }
}

fn handle_home_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Down | KeyCode::Char('j') => app.selected = (app.selected + 1) % MENU_ITEMS.len(),
        KeyCode::Up | KeyCode::Char('k') => {
            app.selected = app.selected.checked_sub(1).unwrap_or(MENU_ITEMS.len() - 1);
        }
        KeyCode::Enter => match app.selected {
            0 => {
                app.screen = Screen::GenerateKey {
                    path: default_key_path().display().to_string(),
                    comment: default_key_comment(),
                    field: 0,
                };
            }
            1 => {
                app.screen = Screen::CreateUser {
                    username: "deploy".to_string(),
                };
            }
            2 => {
                app.screen = Screen::Confirm {
                    action: Action::HardenSshd,
                }
            }
            3 => {
                app.screen = Screen::Connection {
                    username: "deploy".to_string(),
                    host: "server.example.com".to_string(),
                    key_path: default_key_path().display().to_string(),
                    field: 0,
                };
            }
            _ => {}
        },
        _ => {}
    }
    Ok(())
}

fn handle_generate_key(
    app: &mut App,
    key: KeyEvent,
    mut path: String,
    mut comment: String,
    mut field: usize,
) -> Result<()> {
    match key.code {
        KeyCode::Esc => app.screen = Screen::Home,
        KeyCode::Tab | KeyCode::Down | KeyCode::Up => field = 1 - field,
        KeyCode::Backspace => {
            active_input_mut(field, &mut path, &mut comment).pop();
        }
        KeyCode::Char(c) => active_input_mut(field, &mut path, &mut comment).push(c),
        KeyCode::Enter => {
            if path.trim().is_empty() {
                app.screen = message(
                    "Missing key path",
                    "Enter the destination path for the private key.",
                );
            } else if comment.trim().is_empty() {
                app.screen = message(
                    "Missing key comment",
                    "Enter a comment that identifies this key.",
                );
            } else {
                app.screen = Screen::Confirm {
                    action: Action::GenerateKey { path, comment },
                };
            }
            return Ok(());
        }
        _ => {}
    }

    app.screen = Screen::GenerateKey {
        path,
        comment,
        field,
    };
    Ok(())
}

fn handle_create_user(app: &mut App, key: KeyEvent, mut username: String) -> Result<()> {
    match key.code {
        KeyCode::Esc => app.screen = Screen::Home,
        KeyCode::Backspace => {
            username.pop();
        }
        KeyCode::Char(c) => username.push(c),
        KeyCode::Enter => match validate_username(&username) {
            Ok(()) => {
                app.screen = Screen::Confirm {
                    action: Action::CreateUser { username },
                };
            }
            Err(error) => app.screen = message("Invalid username", error),
        },
        _ => {
            app.screen = Screen::CreateUser { username };
        }
    }
    Ok(())
}

fn handle_connection(
    app: &mut App,
    key: KeyEvent,
    mut username: String,
    mut host: String,
    mut key_path: String,
    mut field: usize,
) -> Result<()> {
    match key.code {
        KeyCode::Esc => app.screen = Screen::Home,
        KeyCode::Tab | KeyCode::Down => field = (field + 1) % 3,
        KeyCode::Up => field = field.checked_sub(1).unwrap_or(2),
        KeyCode::Backspace => {
            connection_input_mut(field, &mut username, &mut host, &mut key_path).pop();
        }
        KeyCode::Char(c) => {
            connection_input_mut(field, &mut username, &mut host, &mut key_path).push(c)
        }
        KeyCode::Enter => {
            if let Err(error) = validate_username(&username) {
                app.screen = message("Invalid username", error);
            } else if host.trim().is_empty() || key_path.trim().is_empty() {
                app.screen = message("Missing value", "Host and key path are required.");
            } else {
                let hint = connection_hint(&username, &host, &key_path);
                app.screen = message("Connection command", hint);
            }
            return Ok(());
        }
        _ => {}
    }

    app.screen = Screen::Connection {
        username,
        host,
        key_path,
        field,
    };
    Ok(())
}

fn handle_confirm(terminal: &mut Tui, app: &mut App, key: KeyEvent, action: Action) -> Result<()> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let result = execute_action(terminal, &action);
            app.screen = match result {
                Ok(body) => message("Done", body),
                Err(error) => message("Action failed", error.to_string()),
            };
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.screen = Screen::Home,
        _ => {}
    }
    Ok(())
}

fn execute_action(terminal: &mut Tui, action: &Action) -> Result<String> {
    match action {
        Action::GenerateKey { path, comment } => {
            if Path::new(path).exists() {
                return Err(anyhow!("key path already exists: {path}"));
            }
            if let Some(parent) = Path::new(path).parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                        .with_context(|| format!("failed to secure {}", parent.display()))?;
                }
            }

            run_interactive(
                terminal,
                Command::new("ssh-keygen")
                    .args(["-t", "ed25519", "-a", "100", "-f", path, "-C", comment]),
            )?;
            Ok(format!(
                "Generated keypair at {path}. Add {path}.pub to the target user's authorized_keys."
            ))
        }
        Action::CreateUser { username } => {
            validate_username(username).map_err(|error| anyhow!(error))?;
            run_interactive(
                terminal,
                Command::new("sudo").args(["useradd", "-m", "-s", "/bin/bash", username]),
            )?;
            run_interactive(terminal, Command::new("sudo").args(["passwd", username]))?;
            Ok(format!(
                "Created non-root user `{username}` and opened passwd to set its password."
            ))
        }
        Action::HardenSshd => {
            let temp_path =
                std::env::temp_dir().join(format!("shhup-{}-sshd.conf", std::process::id()));
            fs::write(&temp_path, HARDENED_SSHD_CONFIG)
                .with_context(|| format!("failed to write {}", temp_path.display()))?;

            run_interactive(
                terminal,
                Command::new("sudo").args([
                    "install",
                    "-d",
                    "-m",
                    "0755",
                    "/etc/ssh/sshd_config.d",
                ]),
            )?;
            run_interactive(
                terminal,
                Command::new("sudo")
                    .arg("install")
                    .arg("-m")
                    .arg("0644")
                    .arg(&temp_path)
                    .arg(SSHD_DROPIN_PATH),
            )?;
            run_interactive(terminal, Command::new("sudo").args(["sshd", "-t"]))?;

            if run_interactive(
                terminal,
                Command::new("sudo").args(["systemctl", "reload", "sshd"]),
            )
            .is_err()
            {
                run_interactive(
                    terminal,
                    Command::new("sudo").args(["service", "ssh", "reload"]),
                )?;
            }

            let _ = fs::remove_file(temp_path);
            Ok(format!(
                "Installed hardened SSH settings at {SSHD_DROPIN_PATH} and reloaded sshd."
            ))
        }
    }
}

fn run_interactive(terminal: &mut Tui, command: &mut Command) -> Result<()> {
    suspend_terminal(terminal)?;
    let status = command.status();
    resume_terminal(terminal)?;

    let status = status.context("failed to start command")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("command exited with status {status}"))
    }
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new("shhup")
            .bold()
            .block(Block::default().borders(Borders::BOTTOM)),
        chunks[0],
    );

    match &app.screen {
        Screen::Home => draw_home(frame, app, chunks[1]),
        Screen::GenerateKey {
            path,
            comment,
            field,
        } => draw_form(
            frame,
            chunks[1],
            "Generate SSH key",
            &["Private key path", "Key comment"],
            &[path, comment],
            *field,
        ),
        Screen::CreateUser { username } => draw_form(
            frame,
            chunks[1],
            "Create non-root user",
            &["Username"],
            &[username],
            0,
        ),
        Screen::Connection {
            username,
            host,
            key_path,
            field,
        } => draw_form(
            frame,
            chunks[1],
            "Connection command",
            &["Username", "Host", "Private key path"],
            &[username, host, key_path],
            *field,
        ),
        Screen::Confirm { action } => draw_confirm(frame, chunks[1], action),
        Screen::Message { title, body } => draw_message(frame, chunks[1], title, body),
    }

    frame.render_widget(
        Paragraph::new("q/Esc quit or back  Enter select/submit  Tab switch fields  y/n confirm")
            .style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn draw_home(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let items = MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let style = if idx == app.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(format!("  {item}"), style)))
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title("Secure SSH setup")
                .borders(Borders::ALL),
        ),
        area,
    );
}

fn draw_form(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    labels: &[&str],
    values: &[&str],
    active: usize,
) {
    let lines = labels
        .iter()
        .zip(values.iter())
        .enumerate()
        .flat_map(|(idx, (label, value))| {
            let marker = if idx == active { ">" } else { " " };
            let style = if idx == active {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            [
                Line::from(Span::styled(format!("{marker} {label}"), style)),
                Line::from(format!("  {value}")),
                Line::from(""),
            ]
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_confirm(frame: &mut Frame<'_>, area: Rect, action: &Action) {
    let body = match action {
        Action::GenerateKey { path, comment } => format!(
            "Generate an Ed25519 SSH key at:\n\n{path}\n\nComment:\n{comment}\n\nssh-keygen will ask for a passphrase. Continue? y/n"
        ),
        Action::CreateUser { username } => format!(
            "Create non-root Linux user `{username}` with a home directory and /bin/bash shell, then set its password. Continue? y/n"
        ),
        Action::HardenSshd => format!(
            "Install this sshd drop-in at {SSHD_DROPIN_PATH}:\n\n{HARDENED_SSHD_CONFIG}\nContinue? y/n"
        ),
    };
    frame.render_widget(
        Paragraph::new(body)
            .block(Block::default().title("Confirm").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_message(frame: &mut Frame<'_>, area: Rect, title: &str, body: &str) {
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(body.to_string())
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn message(title: impl Into<String>, body: impl Into<String>) -> Screen {
    Screen::Message {
        title: title.into(),
        body: body.into(),
    }
}

fn active_input_mut<'a>(
    field: usize,
    first: &'a mut String,
    second: &'a mut String,
) -> &'a mut String {
    if field == 0 { first } else { second }
}

fn connection_input_mut<'a>(
    field: usize,
    username: &'a mut String,
    host: &'a mut String,
    key_path: &'a mut String,
) -> &'a mut String {
    match field {
        0 => username,
        1 => host,
        _ => key_path,
    }
}

fn start_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    Terminal::new(CrosstermBackend::new(stdout)).context("failed to initialize terminal")
}

fn stop_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn suspend_terminal(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)?;
    Ok(())
}

fn resume_terminal(terminal: &mut Tui) -> Result<()> {
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen, Hide)?;
    terminal.clear()?;
    Ok(())
}
