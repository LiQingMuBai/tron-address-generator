//! Native desktop shell: renders the embedded web UI inside a system webview.
//!
//! The same axum application used by `--web` is started on an ephemeral local
//! port, then a native window (WKWebView on macOS, WebView2 on Windows) simply
//! navigates to it. No layout is duplicated between the two targets.

use std::path::PathBuf;
use std::process::ExitCode;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use tron_vanity_address::notify::TelegramConfig;
use tron_vanity_address::web;
use wry::WebViewBuilder;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Start the local server and open the native window.
///
/// Never returns normally: the process exits when the window is closed.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = resolve_config();
    let port = web::spawn_local(&config_path, Some(TelegramConfig::default()))?;
    let url = format!("http://127.0.0.1:{port}");

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("TRON Vanity Generator")
        .with_inner_size(LogicalSize::new(1180.0, 820.0))
        .with_min_inner_size(LogicalSize::new(900.0, 640.0))
        .build(&event_loop)?;

    let _webview = WebViewBuilder::new().with_url(&url).build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// Locate `config.xml` next to the executable, falling back to the working directory.
///
/// A missing file is not fatal: the UI falls back to its built-in presets.
fn resolve_config() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("config.xml");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from("config.xml")
}
