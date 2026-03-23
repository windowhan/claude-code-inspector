mod cursor_db;
mod db;
mod dashboard;
mod functions;
mod intercept;
mod mcp;
mod proxy;
mod routing;
mod session;
mod sse_tee;
mod supervisor;
mod llm;
mod supervisor_llm;
mod types;

use std::net::SocketAddr;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use rusqlite::Connection;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::info;

use session::new_session_cache;
use types::AppState;

#[derive(Parser, Debug)]
#[command(name = "claude-code-hook", about = "Claude Code LLM API Inspector")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Proxy listen address
    #[arg(long, default_value = "0.0.0.0:7878", global = true)]
    proxy_addr: String,

    /// Dashboard listen address
    #[arg(long, default_value = "0.0.0.0:7879", global = true)]
    dashboard_addr: String,

    /// Database path (defaults to platform data dir / claude-code-hook / logs.db)
    #[arg(long, global = true)]
    db_path: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the proxy + dashboard server (default)
    Serve,
    /// Start as an MCP server over stdio (for Claude Code integration)
    Mcp,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "claude_code_hook=info,warn".into()),
        )
        .init();

    let args = Args::parse();

    let db_path = resolve_db_path(args.db_path)?;
    let conn = Connection::open(&db_path)?;
    db::init_db(&conn)?;

    let (event_tx, _) = broadcast::channel(256);
    let state = AppState::new(conn, event_tx);

    tokio::spawn(cursor_db::watch(Arc::clone(&state)));

    match args.command.unwrap_or(Command::Serve) {
        Command::Serve => run_server(state, &args.proxy_addr, &args.dashboard_addr).await,
        Command::Mcp   => mcp::run_mcp_server(state).await,
    }
}

pub fn resolve_db_path(override_path: Option<String>) -> anyhow::Result<String> {
    if let Some(p) = override_path {
        return Ok(p);
    }
    let data_dir = dirs_next::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("claude-code-hook");
    std::fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("logs.db").to_string_lossy().to_string())
}

async fn run_server(state: Arc<AppState>, proxy_addr: &str, dashboard_addr: &str) -> anyhow::Result<()> {
    let proxy_addr: SocketAddr     = proxy_addr.parse()?;
    let dashboard_addr: SocketAddr = dashboard_addr.parse()?;

    let proxy_listener = match TcpListener::bind(proxy_addr).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            println!("claude-code-hook is already running on {proxy_addr}. Exiting.");
            std::process::exit(0);
        }
        Err(e) => return Err(e.into()),
    };
    let dashboard_listener = match TcpListener::bind(dashboard_addr).await {
        Ok(l) => l,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            println!("claude-code-hook is already running on {dashboard_addr}. Exiting.");
            std::process::exit(0);
        }
        Err(e) => return Err(e.into()),
    };

    println!();
    println!("  Claude Code LLM API Inspector");
    println!("  ─────────────────────────────────────────────────────");
    println!("  Proxy:     http://{proxy_addr}");
    println!("  Dashboard: http://{dashboard_addr}");
    println!();
    println!("  Set the environment variable:");
    println!("    export ANTHROPIC_BASE_URL=http://{proxy_addr}");
    println!();
    println!("  Claude Code MCP integration:");
    println!("    claude mcp add claude-inspector -- $(which claude-code-hook) mcp");
    println!();
    println!("  Cursor DB watcher: active (polling every 3s)");

    // Start supervisor LLM background task if configured
    {
        let config = {
            let db = state.db.lock().await;
            db::get_supervisor_config(&db).unwrap_or_default()
        };
        if config.enabled && !config.api_key.is_empty() {
            let state_sup = Arc::clone(&state);
            let handle = tokio::spawn(async move {
                supervisor_llm::run_supervisor_loop(state_sup).await;
            });
            let mut h = state.supervisor_handle.lock().await;
            *h = Some(handle);
            info!("Supervisor LLM task started");
        } else {
            info!("Supervisor LLM not configured (set api_key + enabled=true in settings)");
        }
    }

    let state_proxy     = Arc::clone(&state);
    let state_dashboard = Arc::clone(&state);
    let session_cache   = new_session_cache();

    let proxy_task = tokio::spawn(async move {
        info!("Proxy listening on {proxy_addr}");
        loop {
            match proxy_listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let state = Arc::clone(&state_proxy);
                    let sc    = session_cache.clone();
                    let io    = TokioIo::new(stream);
                    tokio::spawn(async move {
                        let svc = service_fn(move |req| {
                            let state = Arc::clone(&state);
                            let sc    = sc.clone();
                            async move { proxy::handle_request(req, state, peer_addr, sc).await }
                        });
                        if let Err(e) = http1::Builder::new().serve_connection(io, svc).with_upgrades().await {
                            tracing::debug!("Proxy connection: {e}");
                        }
                    });
                }
                Err(e) => tracing::error!("Proxy accept: {e}"),
            }
        }
    });

    let dashboard_task = tokio::spawn(async move {
        info!("Dashboard listening on {dashboard_addr}");
        loop {
            match dashboard_listener.accept().await {
                Ok((stream, _peer)) => {
                    let state = Arc::clone(&state_dashboard);
                    let io    = TokioIo::new(stream);
                    tokio::spawn(async move {
                        let svc = service_fn(move |req| {
                            let state = Arc::clone(&state);
                            async move { dashboard::handle_dashboard(req, state).await }
                        });
                        if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                            tracing::debug!("Dashboard connection: {e}");
                        }
                    });
                }
                Err(e) => tracing::error!("Dashboard accept: {e}"),
            }
        }
    });

    tokio::try_join!(proxy_task, dashboard_task)?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_db_path_uses_override() {
        let path = resolve_db_path(Some("/tmp/test.db".to_string())).unwrap();
        assert_eq!(path, "/tmp/test.db");
    }

    #[test]
    fn resolve_db_path_default_contains_claude_code_hook() {
        let path = resolve_db_path(None).unwrap();
        assert!(path.contains("claude-code-hook"));
        assert!(path.ends_with("logs.db"));
    }
}
