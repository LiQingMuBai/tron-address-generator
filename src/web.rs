use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::{CompositeConditions, Condition, Config, ExportFormat, LogicOperator, Position};
use crate::error::{AppError, Result};
use crate::export;
use crate::generator::{self, FoundAddress, ProgressSnapshot, SearchObserver};
use crate::matcher::Matcher;
use crate::notify::{TelegramConfig, TelegramNotifier};

/// Single-page front-end embedded into the binary (equivalent of go:embed).
const INDEX_HTML: &str = include_str!("web_ui.html");

/// Shared state backing the web UI.
type AppState = Arc<Mutex<WebState>>;

/// Everything the browser needs to render the current run.
#[derive(Default)]
struct WebState {
    running: bool,
    finished: bool,
    completed: bool,
    target: usize,
    attempts: u64,
    rate: f64,
    elapsed: f64,
    found: Vec<FoundAddress>,
    stop: Arc<AtomicBool>,
    telegram: Option<TelegramConfig>,
}

/// Start the embedded web server and block until it is shut down.
pub fn serve(
    config_path: &Path,
    host: &str,
    port: u16,
    telegram: Option<TelegramConfig>,
) -> Result<()> {
    let app = build_router(config_path.to_path_buf(), make_state(telegram));

    let address = format!("{host}:{port}");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| AppError::WebServer(e.to_string()))?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&address)
            .await
            .map_err(|e| AppError::WebServer(format!("cannot bind {address}: {e}")))?;

        let url = format!("http://{address}");
        print_banner(&url);
        open_browser(&url);

        axum::serve(listener, app)
            .await
            .map_err(|e| AppError::WebServer(e.to_string()))
    })
}

/// Serve the same application from a background thread on an ephemeral local port.
///
/// Returns the port the server listens on, so a native window can navigate to it.
pub fn spawn_local(config_path: &Path, telegram: Option<TelegramConfig>) -> Result<u16> {
    let app = build_router(config_path.to_path_buf(), make_state(telegram));

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| AppError::WebServer(format!("cannot bind local port: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| AppError::WebServer(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::WebServer(e.to_string()))?
        .port();

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("web server: {error}");
                return;
            }
        };
        runtime.block_on(async move {
            let listener = match tokio::net::TcpListener::from_std(listener) {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!("web server: {error}");
                    return;
                }
            };
            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("web server: {error}");
            }
        });
    });

    Ok(port)
}

/// Build the shared state for the web application.
fn make_state(telegram: Option<TelegramConfig>) -> AppState {
    Arc::new(Mutex::new(WebState {
        telegram,
        ..WebState::default()
    }))
}

/// Assemble the router serving both the page and its JSON API.
fn build_router(config_path: PathBuf, state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route(
            "/api/defaults",
            get(move || {
                let path = config_path.clone();
                async move { defaults(&path) }
            }),
        )
        .route("/api/state", get(state_handler))
        .route("/api/start", post(start))
        .route("/api/stop", post(stop_search))
        .route("/api/download", get(download))
        .with_state(state)
}

/// Serve the embedded page.
async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// Expose the values declared in the XML configuration as UI presets.
fn defaults(config_path: &Path) -> Response {
    match Config::load(config_path) {
        Ok(config) => Json(config_defaults(&config)).into_response(),
        Err(error) => Json(json!({ "error": error.to_string() })).into_response(),
    }
}

/// Snapshot of the running (or last) search.
async fn state_handler(State(state): State<AppState>) -> Json<Value> {
    let guard = state.lock().expect("web state mutex poisoned");
    Json(json!({
        "running": guard.running,
        "finished": guard.finished,
        "completed": guard.completed,
        "target": guard.target,
        "attempts": guard.attempts,
        "rate": guard.rate,
        "elapsed": guard.elapsed,
        "found": guard.found,
    }))
}

/// Validate the request and launch a search on a dedicated thread.
async fn start(State(state): State<AppState>, Json(request): Json<StartRequest>) -> Response {
    let (conditions, target, threads) = match request.into_parts() {
        Ok(parts) => parts,
        Err(error) => return bad_request(error),
    };

    let matcher = match Matcher::new(&conditions) {
        Ok(matcher) => Arc::new(matcher),
        Err(error) => return bad_request(error),
    };

    let (stop, telegram) = {
        let mut guard = state.lock().expect("web state mutex poisoned");
        if guard.running {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "A search is already running" })),
            )
                .into_response();
        }
        guard.running = true;
        guard.finished = false;
        guard.completed = false;
        guard.target = target;
        guard.attempts = 0;
        guard.rate = 0.0;
        guard.elapsed = 0.0;
        guard.found.clear();
        guard.stop = Arc::new(AtomicBool::new(false));
        (Arc::clone(&guard.stop), guard.telegram.clone())
    };

    std::thread::spawn({
        let state = Arc::clone(&state);
        let stop = Arc::clone(&stop);
        move || {
            let base: Arc<dyn SearchObserver> = Arc::new(WebObserver {
                state: Arc::clone(&state),
            });
            let observer: Arc<dyn SearchObserver> = match telegram {
                Some(config) => Arc::new(TelegramNotifier::new(config, base)),
                None => base,
            };
            let outcome = generator::search(matcher, target, threads, Arc::clone(&stop), observer);

            let mut guard = state.lock().expect("web state mutex poisoned");
            guard.running = false;
            guard.finished = true;
            guard.completed = outcome.records.len() >= target;
            guard.attempts = outcome.total_attempts;
            guard.elapsed = outcome.elapsed.as_secs_f64();
            guard.found = outcome.records;
        }
    });

    Json(json!({ "ok": true })).into_response()
}

/// Request cancellation of the running search.
async fn stop_search(State(state): State<AppState>) -> Json<Value> {
    let guard = state.lock().expect("web state mutex poisoned");
    guard.stop.store(true, Ordering::SeqCst);
    Json(json!({ "ok": true }))
}

/// Stream the collected results as a downloadable TXT file.
async fn download(State(state): State<AppState>) -> Response {
    let records = {
        let guard = state.lock().expect("web state mutex poisoned");
        guard.found.clone()
    };

    let format = ExportFormat::Txt;
    let body = match export::render(&records, format) {
        Ok(body) => body,
        Err(error) => return internal_error(error),
    };

    let file_name = export::default_file_name(format);
    let disposition = format!("attachment; filename=\"{file_name}\"");

    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

/// Bridges search events into the shared web state.
struct WebObserver {
    state: AppState,
}

impl SearchObserver for WebObserver {
    fn on_progress(&self, snapshot: ProgressSnapshot) {
        if let Ok(mut guard) = self.state.lock() {
            guard.attempts = snapshot.attempts;
            guard.rate = snapshot.rate;
            guard.elapsed = snapshot.elapsed;
            guard.target = snapshot.target;
        }
    }

    fn on_found(&self, record: &FoundAddress) {
        if let Ok(mut guard) = self.state.lock() {
            guard.found.push(record.clone());
        }
    }
}

/// Browser payload describing a search to run.
#[derive(Debug, Deserialize)]
struct StartRequest {
    suffix: String,
    target: usize,
    #[serde(default)]
    threads: usize,
}

impl StartRequest {
    /// Convert the browser payload into validated domain values.
    fn into_parts(self) -> Result<(CompositeConditions, usize, usize)> {
        if self.target == 0 {
            return Err(AppError::BadRequest(
                "Target count must be greater than 0".to_string(),
            ));
        }
        let suffix = self.suffix.trim();
        let threads = if self.threads == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        } else {
            self.threads
        };

        // An empty suffix means "no filter": every freshly generated address
        // qualifies, so the run simply yields random addresses.
        let conditions = if suffix.is_empty() {
            CompositeConditions {
                logic: LogicOperator::And,
                conditions: Vec::new(),
            }
        } else {
            CompositeConditions {
                logic: LogicOperator::And,
                conditions: vec![Condition::Suffix {
                    value: suffix.to_string(),
                }],
            }
        };

        Ok((conditions, self.target, threads))
    }
}

/// Serialize the XML configuration into the shape expected by the browser.
fn config_defaults(config: &Config) -> Value {
    let threads = if config.task.thread_count == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    } else {
        config.task.thread_count
    };
    json!({
        "suffix": derive_suffix(config),
        "target": config.task.target_count,
        "threads": threads,
    })
}

/// Best-effort translation of the XML conditions into a single trailing suffix.
///
/// Returns an empty string when the configuration declares no matching
/// condition, which the UI shows as "no suffix" (i.e. random addresses).
fn derive_suffix(config: &Config) -> String {
    for condition in &config.conditions.conditions {
        match condition {
            Condition::Suffix { value } => return value.clone(),
            Condition::Repeating {
                ch,
                min_count,
                position: Position::Last,
            } => return ch.to_string().repeat(*min_count),
            _ => {}
        }
    }
    String::new()
}

/// Render a validation failure as HTTP 400.
fn bad_request(error: AppError) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

/// Render an internal failure as HTTP 500.
fn internal_error(error: AppError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

/// Print the access banner for the running server.
fn print_banner(url: &str) {
    let lines = [
        "TRON Vanity Address Generator".to_string(),
        format!("Listen on : {url}"),
        "Press Ctrl+C to stop".to_string(),
    ];
    let width = lines.iter().map(|l| l.len()).max().unwrap_or(30).max(38);

    println!("╔{}╗", "═".repeat(width));
    println!("║{:<width$}║", format!("  {}", lines[0]), width = width);
    println!("╠{}╣", "═".repeat(width));
    for line in &lines[1..] {
        println!("║{:<width$}║", format!("  {line}"), width = width);
    }
    println!("╚{}╝", "═".repeat(width));
}

/// Best-effort attempt to open the default browser.
fn open_browser(url: &str) {
    let command: Option<(&str, Vec<&str>)> = if cfg!(target_os = "macos") {
        Some(("open", vec![url]))
    } else if cfg!(target_os = "windows") {
        Some(("cmd", vec!["/C", "start", "", url]))
    } else {
        Some(("xdg-open", vec![url]))
    };

    if let Some((program, args)) = command {
        let _ = std::process::Command::new(program).args(args).spawn();
    }
}
