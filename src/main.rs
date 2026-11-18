//! PMOG binary: TUI by default, headless client with `--bot`.

use std::io;
use std::net::SocketAddr;
use std::time::Instant;

use pmog::app::App;
use pmog::constants::{SERVER_PORT_START, TICK, WEB_PORT_START};
use pmog::discovery::Discovery;
use pmog::input::handle_local_key;
use pmog::server::Ports;
use pmog::notify::SystemSink;
use pmog::web::Remote;
use pmog::worlds::WorldStore;
use pmog::{bot, machine_id, save, ui, update, utils};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, DisableFocusChange, EnableFocusChange, Event, KeyEventKind};
use ratatui::crossterm::execute;

struct Args {
    name: Option<String>,
    bot: bool,
    server: Option<SocketAddr>,
}

fn flag_value(args: &[String], names: &[&str]) -> Option<String> {
    args.iter().position(|a| names.contains(&a.as_str())).and_then(|i| args.get(i + 1).cloned())
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    Args {
        name: flag_value(&args, &["--name", "-n"]).map(|n| utils::clean_name(&n)).filter(|n| !n.is_empty()),
        bot: args.iter().any(|a| a == "--bot"),
        server: flag_value(&args, &["--server"]).and_then(|s| utils::parse_server_addr(&s, SERVER_PORT_START)),
    }
}

fn default_name() -> String {
    let name = utils::clean_name(&std::env::var("USER").unwrap_or_default());
    if name.is_empty() { "anon".into() } else { name }
}

fn main() -> io::Result<()> {
    if std::env::args().any(|a| a == "--version") {
        let release = update::release();
        println!("PMOG {}", if release > 0 { update::release_label(release) } else { "desenvolvimento".into() });
        return Ok(());
    }
    let args = parse_args();
    if let Ok(exe) = update::running_exe() {
        update::cleanup(exe);
    }
    let ports = Ports::default();
    if args.bot {
        let name = args.name.unwrap_or_else(|| format!("bot{}", rand::random::<u16>()));
        bot::run(&name, ports.discovery, args.server);
        return Ok(());
    }

    let machine_id = machine_id::resolve(true).map_err(|e| {
        eprintln!("Não consegui identificar este computador: {e}");
        io::Error::other(e.to_string())
    })?;
    let mut config = save::load();
    let name = args.name.or_else(|| Some(config.name.clone()).filter(|n| !n.is_empty())).unwrap_or_else(default_name);
    let discovery = Discovery::start(ports.discovery).ok();
    let store = WorldStore::in_data_dir();
    if let (Some(store), Some(base)) = (&store, utils::data_dir())
        && let Err(e) = store.import_legacy(&base, &format!("Servidor de {name}"))
    {
        eprintln!("Não consegui mover o save antigo para a pasta de mundos: {e}");
    }
    let mut app = App::new(name, machine_id, discovery, store, ports);

    app.notifier.enabled = config.notifications;
    app.notifier.set_sink(Box::new(SystemSink::new()));
    app.remote = Some(Remote::start(WEB_PORT_START, config.phone.clone()));
    app.check_web_update(false);
    app.check_firewall();
    if let Some(addr) = args.server {
        app.connect(addr);
    }

    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableFocusChange);
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), DisableFocusChange);
        panic_hook(info);
    }));
    let result = run(&mut terminal, &mut app, &mut config);
    let _ = execute!(io::stdout(), DisableFocusChange);
    ratatui::restore();
    app.shutdown();
    if app.restart {
        update::restart();
    }
    result
}

fn run(terminal: &mut DefaultTerminal, app: &mut App, config: &mut save::Config) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw_local(f, app))?;
        let wait = app.game.next_wake(Instant::now()).map_or(TICK, |w| w.min(TICK));
        if event::poll(wait)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.notifier.focus.key_pressed(Instant::now());
                    if !handle_local_key(app, key, Instant::now()) {
                        return Ok(());
                    }
                }
                Event::FocusGained => app.notifier.focus.gained(),
                Event::FocusLost => app.notifier.focus.lost(),
                _ => {}
            }
        }
        app.step(Instant::now());
        if app.restart {
            return Ok(());
        }
        let name_changed = std::mem::take(&mut app.name_changed);
        let phone_changed = app.remote.as_ref().is_some_and(|r| r.take_dirty());
        if std::mem::take(&mut app.notifications_changed) || name_changed || phone_changed {
            config.name = app.name.clone();
            config.notifications = app.notifier.enabled;
            if let Some(remote) = &app.remote {
                config.phone = remote.settings();
            }
            save::store(config);
        }
    }
}
