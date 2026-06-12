use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
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
use sshup::{
    DEFAULT_SSH_PORT, HARDENED_SSHD_CONFIG, SSHD_DROPIN_PATH, connection_hint, default_key_comment,
    default_key_path, validate_port, validate_username,
};

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

const MENU_ITEMS: [&str; 8] = [
    "Install OpenSSH server",
    "Generate an Ed25519 SSH key",
    "Create a non-root SSH user",
    "Install hardened sshd settings on port 2222",
    "Change sshd port",
    "Install fail2ban",
    "Allow SSH through ufw",
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
    ConfigurePort {
        port: String,
    },
    ConfigureUfw {
        port: String,
    },
    Connection {
        username: String,
        host: String,
        key_path: String,
        port: String,
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
    InstallOpenSshServer,
    GenerateKey { path: String, comment: String },
    CreateUser { username: String },
    HardenSshd,
    ConfigurePort { port: u16 },
    InstallFail2Ban,
    ConfigureUfw { port: u16 },
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
    if is_force_quit_key(key) {
        app.should_quit = true;
        return Ok(());
    }

    let screen = app.screen.clone();
    match screen {
        Screen::Home => handle_home_key(app, key),
        Screen::GenerateKey {
            path,
            comment,
            field,
        } => handle_generate_key(app, key, path, comment, field),
        Screen::CreateUser { username } => handle_create_user(app, key, username),
        Screen::ConfigurePort { port } => handle_configure_port(app, key, port),
        Screen::ConfigureUfw { port } => handle_configure_ufw(app, key, port),
        Screen::Connection {
            username,
            host,
            key_path,
            port,
            field,
        } => handle_connection(app, key, username, host, key_path, port, field),
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
                app.screen = Screen::Confirm {
                    action: Action::InstallOpenSshServer,
                };
            }
            1 => {
                app.screen = Screen::GenerateKey {
                    path: default_key_path().display().to_string(),
                    comment: default_key_comment(),
                    field: 0,
                };
            }
            2 => {
                app.screen = Screen::CreateUser {
                    username: "deploy".to_string(),
                };
            }
            3 => {
                app.screen = Screen::Confirm {
                    action: Action::HardenSshd,
                }
            }
            4 => {
                app.screen = Screen::ConfigurePort {
                    port: DEFAULT_SSH_PORT.to_string(),
                };
            }
            5 => {
                app.screen = Screen::Confirm {
                    action: Action::InstallFail2Ban,
                };
            }
            6 => {
                app.screen = Screen::ConfigureUfw {
                    port: DEFAULT_SSH_PORT.to_string(),
                };
            }
            7 => {
                app.screen = Screen::Connection {
                    username: "deploy".to_string(),
                    host: "server.example.com".to_string(),
                    key_path: default_key_path().display().to_string(),
                    port: DEFAULT_SSH_PORT.to_string(),
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
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = Screen::Home;
            return Ok(());
        }
        KeyCode::Tab | KeyCode::BackTab | KeyCode::Down | KeyCode::Up => field = 1 - field,
        KeyCode::Backspace => {
            active_input_mut(field, &mut path, &mut comment).pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            active_input_mut(field, &mut path, &mut comment).clear();
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
        KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Home,
        KeyCode::Tab | KeyCode::BackTab | KeyCode::Down | KeyCode::Up => {
            app.screen = Screen::CreateUser { username };
        }
        KeyCode::Backspace => {
            username.pop();
            app.screen = Screen::CreateUser { username };
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            username.clear();
            app.screen = Screen::CreateUser { username };
        }
        KeyCode::Char(c) => {
            username.push(c);
            app.screen = Screen::CreateUser { username };
        }
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

fn handle_configure_port(app: &mut App, key: KeyEvent, mut port: String) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Home,
        KeyCode::Backspace => {
            port.pop();
            app.screen = Screen::ConfigurePort { port };
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            port.clear();
            app.screen = Screen::ConfigurePort { port };
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            port.push(c);
            app.screen = Screen::ConfigurePort { port };
        }
        KeyCode::Enter => match parse_port(&port) {
            Ok(port) => {
                app.screen = Screen::Confirm {
                    action: Action::ConfigurePort { port },
                };
            }
            Err(error) => app.screen = message("Invalid port", error),
        },
        _ => app.screen = Screen::ConfigurePort { port },
    }
    Ok(())
}

fn handle_configure_ufw(app: &mut App, key: KeyEvent, mut port: String) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Home,
        KeyCode::Backspace => {
            port.pop();
            app.screen = Screen::ConfigureUfw { port };
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            port.clear();
            app.screen = Screen::ConfigureUfw { port };
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            port.push(c);
            app.screen = Screen::ConfigureUfw { port };
        }
        KeyCode::Enter => match parse_port(&port) {
            Ok(port) => {
                app.screen = Screen::Confirm {
                    action: Action::ConfigureUfw { port },
                };
            }
            Err(error) => app.screen = message("Invalid port", error),
        },
        _ => app.screen = Screen::ConfigureUfw { port },
    }
    Ok(())
}

fn handle_connection(
    app: &mut App,
    key: KeyEvent,
    mut username: String,
    mut host: String,
    mut key_path: String,
    mut port: String,
    mut field: usize,
) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.screen = Screen::Home;
            return Ok(());
        }
        KeyCode::Tab | KeyCode::Down => field = (field + 1) % 4,
        KeyCode::BackTab | KeyCode::Up => field = field.checked_sub(1).unwrap_or(3),
        KeyCode::Backspace => {
            connection_input_mut(field, &mut username, &mut host, &mut key_path, &mut port).pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            connection_input_mut(field, &mut username, &mut host, &mut key_path, &mut port).clear();
        }
        KeyCode::Char(c) => {
            connection_input_mut(field, &mut username, &mut host, &mut key_path, &mut port).push(c)
        }
        KeyCode::Enter => {
            if let Err(error) = validate_username(&username) {
                app.screen = message("Invalid username", error);
            } else if host.trim().is_empty() || key_path.trim().is_empty() {
                app.screen = message("Missing value", "Host and key path are required.");
            } else {
                match parse_port(&port) {
                    Ok(port) => {
                        let hint = connection_hint(&username, &host, &key_path, port);
                        app.screen = message("Connection command", hint);
                    }
                    Err(error) => app.screen = message("Invalid port", error),
                }
            }
            return Ok(());
        }
        _ => {}
    }

    app.screen = Screen::Connection {
        username,
        host,
        key_path,
        port,
        field,
    };
    Ok(())
}

fn parse_port(port: &str) -> Result<u16, String> {
    let port = port
        .trim()
        .parse::<u16>()
        .map_err(|_| "port must be a number from 1 to 65535".to_string())?;
    validate_port(port)?;
    Ok(port)
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
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
            app.screen = Screen::Home
        }
        _ => {}
    }
    Ok(())
}

fn is_force_quit_key(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
}

fn execute_action(terminal: &mut Tui, action: &Action) -> Result<String> {
    match action {
        Action::InstallOpenSshServer => {
            let mut command = package_install_command(&["openssh-server", "openssh"]);
            run_interactive(terminal, &mut command)?;
            let _ =
                run_captured(Command::new("sudo").args(["systemctl", "enable", "--now", "ssh"]));
            let _ =
                run_captured(Command::new("sudo").args(["systemctl", "enable", "--now", "sshd"]));
            Ok("Installed the OpenSSH server package where supported and attempted to enable the ssh/sshd service.".to_string())
        }
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
                "Generated a local keypair for the person running sshup.\n\nPrivate key:\n{path}\n\nPublic key:\n{path}.pub\n\nTo let a server user log in with this key, append the public key to that user's ~/.ssh/authorized_keys on the server."
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
            install_sshd_config(terminal, HARDENED_SSHD_CONFIG)?;
            Ok(format!(
                "Installed hardened SSH settings at {SSHD_DROPIN_PATH} and reloaded sshd."
            ))
        }
        Action::ConfigurePort { port } => {
            validate_port(*port).map_err(|error| anyhow!(error))?;
            let config = HARDENED_SSHD_CONFIG.replace("Port 2222", &format!("Port {port}"));
            install_sshd_config(terminal, &config)?;
            Ok(format!(
                "Configured sshd to listen on port {port}, validated the config, and reloaded sshd."
            ))
        }
        Action::InstallFail2Ban => {
            let mut command = package_install_command(&["fail2ban", "fail2ban"]);
            run_interactive(terminal, &mut command)?;
            let _ = run_captured(Command::new("sudo").args([
                "systemctl",
                "enable",
                "--now",
                "fail2ban",
            ]));
            Ok(
                "Installed fail2ban where supported and attempted to enable the fail2ban service."
                    .to_string(),
            )
        }
        Action::ConfigureUfw { port } => {
            validate_port(*port).map_err(|error| anyhow!(error))?;
            run_captured(
                Command::new("sh")
                    .arg("-c")
                    .arg("command -v ufw >/dev/null 2>&1"),
            )
            .context(
                "ufw is not installed. Install ufw first or manage firewall rules outside sshup.",
            )?;
            run_interactive(
                terminal,
                Command::new("sudo").args(["ufw", "allow", &format!("{port}/tcp")]),
            )?;
            Ok(format!(
                "Added a ufw allow rule for TCP port {port}.\n\nsshup did not enable ufw automatically. If ufw is inactive, review existing services first, then run `sudo ufw enable` when ready."
            ))
        }
    }
}

fn install_sshd_config(terminal: &mut Tui, config: &str) -> Result<()> {
    let temp_path = std::env::temp_dir().join(format!("sshup-{}-sshd.conf", std::process::id()));
    fs::write(&temp_path, config)
        .with_context(|| format!("failed to write {}", temp_path.display()))?;

    run_interactive(terminal, Command::new("sudo").arg("-v"))?;
    run_captured(Command::new("sudo").args([
        "-n",
        "install",
        "-d",
        "-m",
        "0755",
        "/etc/ssh/sshd_config.d",
    ]))?;
    run_captured(
        Command::new("sudo")
            .arg("-n")
            .arg("install")
            .arg("-m")
            .arg("0644")
            .arg(&temp_path)
            .arg(SSHD_DROPIN_PATH),
    )?;
    run_captured(Command::new("sudo").args(["-n", "sshd", "-t"]))?;

    if let Err(systemctl_error) =
        run_captured(Command::new("sudo").args(["-n", "systemctl", "reload", "sshd"]))
    {
        run_captured(Command::new("sudo").args(["-n", "service", "ssh", "reload"]))
            .with_context(|| format!("systemctl reload also failed:\n{systemctl_error}"))?;
    }

    let _ = fs::remove_file(temp_path);
    Ok(())
}

fn package_install_command(packages: &[&str; 2]) -> Command {
    let script = format!(
        r#"set -eu
if command -v apt-get >/dev/null 2>&1; then
  sudo apt-get update && sudo apt-get install -y {}
elif command -v dnf >/dev/null 2>&1; then
  sudo dnf install -y {}
elif command -v yum >/dev/null 2>&1; then
  sudo yum install -y {}
elif command -v pacman >/dev/null 2>&1; then
  sudo pacman -Sy --noconfirm {}
elif command -v zypper >/dev/null 2>&1; then
  sudo zypper --non-interactive install {}
else
  echo "No supported package manager found." >&2
  exit 1
fi"#,
        packages[0], packages[1], packages[1], packages[0], packages[0]
    );
    let mut command = Command::new("sh");
    command.arg("-c").arg(script);
    command
}

fn run_interactive(terminal: &mut Tui, command: &mut Command) -> Result<()> {
    let command_display = command_display(command);
    suspend_terminal(terminal)?;
    let status = command.status();
    resume_terminal(terminal)?;

    let status = status.with_context(|| format!("failed to start command: {command_display}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "command failed: {command_display}\nstatus: {status}"
        ))
    }
}

fn run_captured(command: &mut Command) -> Result<()> {
    let command_display = command_display(command);
    let output = command
        .output()
        .with_context(|| format!("failed to start command: {command_display}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "{}",
            command_failure_message(&command_display, &output)
        ))
    }
}

fn command_display(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().to_string()];
    parts.extend(
        command
            .get_args()
            .map(|arg| shellish_quote(&arg.to_string_lossy())),
    );
    parts.join(" ")
}

fn shellish_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':' | '='))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn command_failure_message(command_display: &str, output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut message = format!(
        "command failed: {command_display}\nstatus: {}",
        output.status
    );

    if !stdout.trim().is_empty() {
        message.push_str("\n\nstdout:\n");
        message.push_str(stdout.trim());
    }

    if !stderr.trim().is_empty() {
        message.push_str("\n\nstderr:\n");
        message.push_str(stderr.trim());
    }

    message
}

#[derive(Debug)]
struct SshStatus {
    running: bool,
    ports: Option<String>,
    detail: Option<String>,
}

fn ssh_status() -> SshStatus {
    let running = service_is_active("sshd") || service_is_active("ssh");
    let ports = listening_ssh_ports();
    let detail = if running {
        Some("Use the port shown here in your connection command.".to_string())
    } else {
        Some("Install and start OpenSSH server to accept SSH connections.".to_string())
    };

    SshStatus {
        running,
        ports,
        detail,
    }
}

fn service_is_active(service: &str) -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", service])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn listening_ssh_ports() -> Option<String> {
    let output = Command::new("ss").args(["-ltnp"]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports = BTreeSet::new();
    for line in stdout.lines().skip(1).filter(|line| line.contains("sshd")) {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        let Some(local_address) = columns.get(3) else {
            continue;
        };
        if let Some(port) = local_address.rsplit(':').next()
            && port.chars().all(|c| c.is_ascii_digit())
        {
            ports.insert(port.to_string());
        }
    }

    if ports.is_empty() {
        None
    } else {
        Some(ports.into_iter().collect::<Vec<_>>().join(", "))
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
        Paragraph::new("sshup")
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
        } => {
            let values = [path.as_str(), comment.as_str()];
            draw_form(
                frame,
                chunks[1],
                "Generate SSH key",
                &[
                    "Creates a private/public keypair on this machine for the person running sshup.",
                    "To use it, copy the .pub file into the target server user's authorized_keys.",
                ],
                &["Private key path", "Key comment"],
                &values,
                *field,
            );
            set_form_cursor(frame, chunks[1], 2, *field, values[*field]);
        }
        Screen::CreateUser { username } => {
            let values = [username.as_str()];
            draw_form(
                frame,
                chunks[1],
                "Create non-root user",
                &[
                    "Creates a Linux account on this server.",
                    "This is usually the user named in ssh user@host and owns ~/.ssh/authorized_keys.",
                ],
                &["Username"],
                &values,
                0,
            );
            set_form_cursor(frame, chunks[1], 2, 0, values[0]);
        }
        Screen::Connection {
            username,
            host,
            key_path,
            port,
            field,
        } => {
            let values = [
                username.as_str(),
                host.as_str(),
                key_path.as_str(),
                port.as_str(),
            ];
            draw_form(
                frame,
                chunks[1],
                "Connection command",
                &[
                    "Builds the command to connect from this machine to the server.",
                    "Username is the server user; private key path is the matching local private key.",
                ],
                &["Username", "Host", "Private key path", "Port"],
                &values,
                *field,
            );
            set_form_cursor(frame, chunks[1], 2, *field, values[*field]);
        }
        Screen::ConfigurePort { port } => {
            let values = [port.as_str()];
            draw_form(
                frame,
                chunks[1],
                "Change sshd port",
                &[
                    "Writes the managed sshd drop-in with this port and the hardened defaults.",
                    "The default is 2222. Port 22 is intentionally rejected.",
                ],
                &["Port"],
                &values,
                0,
            );
            set_form_cursor(frame, chunks[1], 2, 0, values[0]);
        }
        Screen::ConfigureUfw { port } => {
            let values = [port.as_str()];
            draw_form(
                frame,
                chunks[1],
                "Allow SSH through ufw",
                &[
                    "Adds a ufw allow rule for the selected SSH TCP port.",
                    "sshup does not enable ufw automatically, so other services are not disrupted.",
                ],
                &["SSH port"],
                &values,
                0,
            );
            set_form_cursor(frame, chunks[1], 2, 0, values[0]);
        }
        Screen::Confirm { action } => draw_confirm(frame, chunks[1], action),
        Screen::Message { title, body } => draw_message(frame, chunks[1], title, body),
    }

    frame.render_widget(
        Paragraph::new(
            "q/Esc back  Ctrl+C quit  Enter select/submit  Tab switch fields  Ctrl+U clear",
        )
        .style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );
}

fn draw_home(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(8)])
        .split(area);

    let status = ssh_status();
    let status_style = if status.running {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };
    let mut status_lines = vec![
        Line::from(vec![
            Span::raw("Service: "),
            Span::styled(
                if status.running {
                    "running"
                } else {
                    "not running"
                },
                status_style,
            ),
        ]),
        Line::from(format!(
            "Listening ports: {}",
            status
                .ports
                .map(|ports| ports.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )),
    ];
    if let Some(detail) = status.detail {
        status_lines.push(Line::from(Span::styled(
            detail,
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(
        Paragraph::new(status_lines)
            .block(Block::default().title("SSH status").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

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
        chunks[1],
    );
}

fn draw_form(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    help: &[&str],
    labels: &[&str],
    values: &[&str],
    active: usize,
) {
    let mut lines = help
        .iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(Color::DarkGray))))
        .collect::<Vec<_>>();

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines.extend(
        labels
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
            .collect::<Vec<_>>(),
    );

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn set_form_cursor(
    frame: &mut Frame<'_>,
    area: Rect,
    help_lines: usize,
    active: usize,
    value: &str,
) {
    let content_x = area.x.saturating_add(1);
    let content_y = area.y.saturating_add(1);
    let max_x = area.right().saturating_sub(2);
    let x = content_x
        .saturating_add(2)
        .saturating_add(value.chars().count() as u16)
        .min(max_x);
    let help_offset = if help_lines == 0 { 0 } else { help_lines + 1 };
    let y = content_y.saturating_add(
        (help_offset as u16)
            .saturating_add((active as u16).saturating_mul(3))
            .saturating_add(1),
    );

    if area.contains(ratatui::layout::Position { x, y }) {
        frame.set_cursor_position(ratatui::layout::Position { x, y });
    }
}

fn draw_confirm(frame: &mut Frame<'_>, area: Rect, action: &Action) {
    let body = match action {
        Action::InstallOpenSshServer => {
            "Install OpenSSH server using the detected package manager, then attempt to enable and start the ssh/sshd service. Continue? y/n".to_string()
        }
        Action::GenerateKey { path, comment } => format!(
            "Generate an Ed25519 SSH key at:\n\n{path}\n\nComment:\n{comment}\n\nssh-keygen will ask for a passphrase. Continue? y/n"
        ),
        Action::CreateUser { username } => format!(
            "Create non-root Linux user `{username}` with a home directory and /bin/bash shell, then set its password. Continue? y/n"
        ),
        Action::HardenSshd => format!(
            "Install this sshd drop-in at {SSHD_DROPIN_PATH}:\n\n{HARDENED_SSHD_CONFIG}\nContinue? y/n"
        ),
        Action::ConfigurePort { port } => format!(
            "Configure sshd to listen on port {port} and keep the hardened sshd settings.\n\nThis writes {SSHD_DROPIN_PATH}, validates sshd config, then reloads sshd. Continue? y/n"
        ),
        Action::InstallFail2Ban => {
            "Install fail2ban using the detected package manager, then attempt to enable and start the fail2ban service. Continue? y/n".to_string()
        }
        Action::ConfigureUfw { port } => format!(
            "Add a ufw allow rule for TCP port {port}.\n\nsshup will not enable ufw automatically. This avoids blocking unrelated services by surprise. Continue? y/n"
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
    port: &'a mut String,
) -> &'a mut String {
    match field {
        0 => username,
        1 => host,
        2 => key_path,
        _ => port,
    }
}

fn start_terminal() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
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
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn q_and_esc_leave_generate_key_form() {
        for code in [KeyCode::Char('q'), KeyCode::Esc] {
            let mut app = App::default();

            handle_generate_key(
                &mut app,
                key(code),
                "/tmp/key".to_string(),
                "comment".to_string(),
                0,
            )
            .unwrap();

            assert!(matches!(app.screen, Screen::Home));
        }
    }

    #[test]
    fn tab_switches_generate_key_fields() {
        let mut app = App::default();

        handle_generate_key(
            &mut app,
            key(KeyCode::Tab),
            "/tmp/key".to_string(),
            "comment".to_string(),
            0,
        )
        .unwrap();

        assert!(matches!(app.screen, Screen::GenerateKey { field: 1, .. }));
    }

    #[test]
    fn create_user_tab_keeps_field_editable() {
        let mut app = App::default();

        handle_create_user(&mut app, key(KeyCode::Tab), "deploy".to_string()).unwrap();
        handle_create_user(&mut app, key(KeyCode::Backspace), "deploy".to_string()).unwrap();

        match app.screen {
            Screen::CreateUser { username } => assert_eq!(username, "deplo"),
            _ => panic!("expected create user form"),
        }
    }

    #[test]
    fn q_and_esc_leave_connection_form() {
        for code in [KeyCode::Char('q'), KeyCode::Esc] {
            let mut app = App::default();

            handle_connection(
                &mut app,
                key(code),
                "deploy".to_string(),
                "server.example.com".to_string(),
                "/tmp/key".to_string(),
                "2222".to_string(),
                1,
            )
            .unwrap();

            assert!(matches!(app.screen, Screen::Home));
        }
    }

    #[test]
    fn ctrl_c_is_force_quit_key() {
        assert!(is_force_quit_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
    }

    #[test]
    fn parse_port_rejects_default_ssh_port() {
        assert!(parse_port("22").is_err());
        assert_eq!(parse_port("2222").unwrap(), 2222);
    }

    #[test]
    fn shellish_quote_preserves_simple_command_parts() {
        assert_eq!(
            shellish_quote("/etc/ssh/sshd_config.d"),
            "/etc/ssh/sshd_config.d"
        );
        assert_eq!(shellish_quote("reload"), "reload");
    }

    #[test]
    fn shellish_quote_quotes_spaces() {
        assert_eq!(shellish_quote("needs space"), "'needs space'");
    }
}
