mod scanner;

use clap::Parser;
use std::sync::mpsc;
use std::thread;

// ======================
// CLI
// ======================
#[derive(Parser)]
struct Args {
    #[arg(value_name = "PATH")]
    path: Option<String>,
}

// ======================
// APP STATE
// ======================
struct App {
    nodes: Option<Vec<scanner::Node>>,
    current: usize,
    selected: usize,
    progress: Option<scanner::Progress>,
    scan_path: String, // <-- FIX: store scan path for UI title
}

use std::io::{self, Write};

fn ask_path() -> String {
    print!("Mention Scan directory:~ ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    input.trim().to_string()
}

// ======================
// MAIN
// ======================
fn main() {
    let args = Args::parse();

    let path = match args.path {
        Some(p) => p,
        None => ask_path(),
    };

    let (tx_nodes, rx_nodes) = mpsc::channel();
    let (tx_prog, rx_prog) = mpsc::channel();

    let path_clone = path.clone();

    // background scan
    thread::spawn(move || {
        let nodes = scanner::build_tree_with_progress(&path_clone, tx_prog);
        let _ = tx_nodes.send(nodes);
    });

    let app = App {
        nodes: None,
        current: 0,
        selected: 0,
        progress: None,
        scan_path: path.clone(), // <-- FIX
    };

    run_ui(app, rx_nodes, rx_prog).unwrap();
}

// ======================
// IMPORTS
// ======================
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};

use ratatui::{
    backend::CrosstermBackend,
    Terminal,
    widgets::{Block, Borders, List, ListItem, ListState},
    style::{Modifier, Style},
};

use humansize::{format_size, DECIMAL};

// ======================
// UI LOOP
// ======================
fn run_ui(
    mut app: App,
    rx_nodes: mpsc::Receiver<Vec<scanner::Node>>,
    rx_prog: mpsc::Receiver<scanner::Progress>,
) -> io::Result<()> {
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last = Instant::now();

    loop {
        // =========================
        // progress updates
        // =========================
        while let Ok(p) = rx_prog.try_recv() {
            app.progress = Some(p);
        }

        // =========================
        // final nodes
        // =========================
        if let Ok(nodes) = rx_nodes.try_recv() {
            app.nodes = Some(nodes);
            app.current = 0;
        }

        terminal.draw(|f| {
            let items: Vec<ListItem> = if let Some(nodes) = &app.nodes {
                let root = &nodes[app.current];

                root.children
                    .iter()
                    .map(|i| {
                        let n = &nodes[*i];

                        let name = n
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();

                        let icon = if n.is_dir { "📁" } else { "📄" };

                        ListItem::new(format!(
                            "{:>10}  {} {}",
                            format_size(n.size, DECIMAL),
                            icon,
                            name
                        ))
                    })
                    .collect()
            } else {
                let progress_text = if let Some(p) = &app.progress {
                    let percent = if p.total == 0 {
                        0
                    } else {
                        (p.done * 100) / p.total
                    };

                    format!("Scanning disk... please wait ({}%)", percent)
                } else {
                    "Scanning disk... please wait (0%)".to_string()
                };

                vec![ListItem::new(progress_text)]
            };

            // ======================
            // TITLE (FIXED)
            // ======================
            let title = if let Some(nodes) = &app.nodes {
                let path = &nodes[app.current].path;

                format!(
                    "Disk Analyzer [{}] | ↑↓ Enter Backspace | 'q' to exit",
                    path.display()
                )
            } else {
                let shortened = if app.scan_path.len() > 80 {
                    format!(
                        "...{}",
                        &app.scan_path[app.scan_path.len() - 77..]
                    )
                } else {
                    app.scan_path.clone()
                };

                format!(
                    "Disk Analyzer | scanning dir \"{}\"...",
                    shortened
                )
            };

            let mut state = ListState::default();
            state.select(Some(app.selected));

            let list = List::new(items)
                .block(Block::default().title(title).borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            f.render_stateful_widget(list, f.area(), &mut state);
        })?;

        // =========================
        // INPUT
        // =========================
        if let Ok(true) = event::poll(Duration::from_millis(50)) {
            if let Event::Key(key) = event::read()? {
                if last.elapsed() < Duration::from_millis(120) {
                    continue;
                }
                last = Instant::now();

                if app.nodes.is_none() {
                    continue;
                }

                let nodes = app.nodes.as_ref().unwrap();

                match key.code {
                    KeyCode::Char('q') => break,

                    KeyCode::Down => {
                        let children = &nodes[app.current].children;
                        if !children.is_empty() {
                            app.selected = (app.selected + 1).min(children.len() - 1);
                        }
                    }

                    KeyCode::Up => {
                        app.selected = app.selected.saturating_sub(1);
                    }

                    KeyCode::Enter => {
                        let children = &nodes[app.current].children;
                        if let Some(&next) = children.get(app.selected) {
                            app.current = next;
                            app.selected = 0;
                        }
                    }

                    KeyCode::Backspace => {
                        let current_path = &nodes[app.current].path;

                        if let Some(parent) = current_path.parent() {
                            for (i, n) in nodes.iter().enumerate() {
                                if n.path == parent {
                                    app.current = i;
                                    app.selected = 0;
                                    break;
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}