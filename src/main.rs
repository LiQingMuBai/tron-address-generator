use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use indicatif::{ProgressBar, ProgressStyle};

use tron_vanity_address::config::{Config, ExportFormat};
use tron_vanity_address::error::Result;
use tron_vanity_address::export;
use tron_vanity_address::generator::{self, FoundAddress, ProgressSnapshot, SearchObserver};
use tron_vanity_address::matcher::Matcher;
use tron_vanity_address::notify::{
    TelegramConfig, TelegramNotifier, DEFAULT_TELEGRAM_CHAT, DEFAULT_TELEGRAM_TOKEN,
};
use tron_vanity_address::web;

/// CLI surface for the TRON vanity address generator.
#[derive(Debug, Parser)]
#[command(
    name = "tron-vanity",
    version,
    about = "TRON vanity address generator driven by an XML configuration file"
)]
struct Cli {
    /// Path to the XML configuration file
    #[arg(short, long, default_value = "config.xml")]
    config: PathBuf,

    /// Launch the embedded web interface instead of the terminal workflow
    #[arg(long)]
    web: bool,

    /// Interface to bind when running the web interface
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind when running the web interface
    #[arg(long, default_value_t = 8686)]
    port: u16,

    /// Override TaskConfig/TargetCount
    #[arg(short = 'n', long)]
    count: Option<usize>,

    /// Override TaskConfig/ThreadCount (0 = auto-detect CPU cores)
    #[arg(short, long)]
    threads: Option<usize>,

    /// Override TaskConfig/ExportFormat (csv | txt | json)
    #[arg(short, long)]
    format: Option<String>,

    /// Directory receiving the exported result file
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Disable the live progress display
    #[arg(long)]
    no_progress: bool,

    /// Override the Telegram bot token used for notifications
    #[arg(long)]
    telegram_token: Option<String>,

    /// Override the Telegram chat id receiving the notifications
    #[arg(long)]
    telegram_chat: Option<String>,

    /// Disable the Telegram notifications sent for every found address
    #[arg(long)]
    no_telegram: bool,
}

/// Resolve the effective Telegram settings, or `None` when notifications are off.
fn telegram_config(cli: &Cli) -> Option<TelegramConfig> {
    if cli.no_telegram {
        return None;
    }
    Some(TelegramConfig {
        token: cli
            .telegram_token
            .clone()
            .unwrap_or_else(|| DEFAULT_TELEGRAM_TOKEN.to_string()),
        chat_id: cli
            .telegram_chat
            .clone()
            .unwrap_or_else(|| DEFAULT_TELEGRAM_CHAT.to_string()),
    })
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Dispatch to either the web interface or the terminal workflow.
fn run() -> Result<()> {
    let cli = Cli::parse();
    let telegram = telegram_config(&cli);

    if cli.web {
        return web::serve(&cli.config, &cli.host, cli.port, telegram);
    }

    let mut config = Config::load(&cli.config)?;

    if let Some(count) = cli.count {
        config.task.target_count = count;
    }
    if let Some(threads) = cli.threads {
        config.task.thread_count = threads;
    }
    if let Some(format) = &cli.format {
        config.task.export_format = ExportFormat::parse(format)?;
    }
    config.validate()?;

    let requested_threads = config.task.thread_count;
    let threads = if requested_threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        requested_threads
    };

    let matcher = Arc::new(Matcher::new(&config.conditions)?);

    print_banner(
        &config,
        threads,
        requested_threads == 0,
        &matcher.describe(),
    );

    if config.conditions.conditions.is_empty() {
        println!("warning: no conditions configured, every generated address will match\n");
    }

    let bar = if cli.no_progress {
        None
    } else {
        Some(build_progress_bar())
    };
    let base: Arc<dyn SearchObserver> = match &bar {
        Some(bar) => Arc::new(TerminalObserver { bar: bar.clone() }),
        None => Arc::new(SilentObserver),
    };
    let observer: Arc<dyn SearchObserver> = match telegram {
        Some(config) => Arc::new(TelegramNotifier::new(config, base)),
        None => base,
    };

    let outcome = generator::search(
        Arc::clone(&matcher),
        config.task.target_count,
        threads,
        Arc::new(AtomicBool::new(false)),
        observer,
    );

    if let Some(bar) = bar {
        bar.finish_and_clear();
    }

    if outcome.records.is_empty() {
        println!("no matching address was generated");
        return Ok(());
    }

    print_results(&outcome.records);

    let export_dir = if cli.output.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        cli.output.clone()
    };
    let path = export::export(&outcome.records, config.task.export_format, &export_dir)?;

    println!(
        "\nfinished: {} address(es) in {:.2}s ({:.0} keys/s, {} attempts total)",
        outcome.records.len(),
        outcome.elapsed.as_secs_f64(),
        outcome.keys_per_second(),
        outcome.total_attempts
    );
    println!(
        "exported {} -> {}",
        config.task.export_format.as_str(),
        path.display()
    );

    Ok(())
}

/// Renders live search counters into the terminal spinner.
struct TerminalObserver {
    bar: ProgressBar,
}

impl SearchObserver for TerminalObserver {
    fn on_progress(&self, snapshot: ProgressSnapshot) {
        self.bar.set_message(format!(
            "attempts: {:<12} {:>10.0} keys/s  found: {}/{}  elapsed: {:.1}s",
            snapshot.attempts, snapshot.rate, snapshot.found, snapshot.target, snapshot.elapsed
        ));
    }

    fn on_found(&self, _record: &FoundAddress) {}
}

/// Discards all search events, used by `--no-progress`.
struct SilentObserver;

impl SearchObserver for SilentObserver {
    fn on_progress(&self, _snapshot: ProgressSnapshot) {}
    fn on_found(&self, _record: &FoundAddress) {}
}

/// Create the spinner used to render live progress.
fn build_progress_bar() -> ProgressBar {
    let bar = ProgressBar::new_spinner();
    let style =
        ProgressStyle::with_template("{spinner:.cyan} {msg}").expect("valid progress template");
    bar.set_style(style);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar
}

/// Unicode banner summarising the effective run parameters.
fn print_banner(config: &Config, threads: usize, auto_threads: bool, conditions: &[String]) {
    let thread_label = if auto_threads {
        format!("{} (auto)", threads)
    } else {
        threads.to_string()
    };

    let rows = vec![
        ("Chain", config.chain.chain_type.clone()),
        ("Address prefix", config.chain.address_prefix.clone()),
        ("Target count", config.task.target_count.to_string()),
        ("Threads", thread_label),
        ("Export format", config.task.export_format.as_str().to_string()),
        ("Logic operator", config.conditions.logic.as_str().to_string()),
        (
            "Conditions",
            format!("{}", config.conditions.conditions.len()),
        ),
    ];

    let mut width = rows
        .iter()
        .map(|(key, value)| key.len() + value.len() + 4)
        .max()
        .unwrap_or(30)
        .max(38);
    for condition in conditions {
        width = width.max(condition.len() + 12);
    }

    println!("╔{}╗", "═".repeat(width));
    println!(
        "║{:<width$}║",
        "  TRON Vanity Address Generator",
        width = width
    );
    println!("╠{}╣", "═".repeat(width));
    for (key, value) in rows {
        let line = format!("  {:<16} {}", key, value);
        println!("║{:<width$}║", line, width = width);
    }
    if !conditions.is_empty() {
        println!("╟{}╢", "─".repeat(width));
        for condition in conditions {
            let line = format!("  • {}", condition);
            println!("║{:<width$}║", line, width = width);
        }
    }
    println!("╚{}╝", "═".repeat(width));
    println!();
}

/// Render the matched addresses as a terminal table.
fn print_results(records: &[FoundAddress]) {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["#", "Address", "Private Key (Hex)", "Attempts"]);

    for record in records {
        table.add_row(vec![
            Cell::new(record.index),
            Cell::new(&record.address),
            Cell::new(&record.private_key),
            Cell::new(record.found_at_attempts),
        ]);
    }

    println!("{table}");
}
