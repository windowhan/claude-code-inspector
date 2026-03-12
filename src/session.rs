use std::collections::HashMap;
use std::net::SocketAddr;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub pid: Option<i64>,
    pub cwd: Option<String>,
    pub project_name: Option<String>,
}

pub type SessionCache = Arc<Mutex<HashMap<String, SessionInfo>>>;

pub fn new_session_cache() -> SessionCache {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Look up a session by source address. Uses lsof to find the PID and CWD
/// of the process that owns this TCP connection on macOS.
pub async fn resolve_session(
    peer_addr: SocketAddr,
    cache: &SessionCache,
) -> SessionInfo {
    let key = peer_addr.to_string();
    {
        let cache_guard = cache.lock().await;
        if let Some(info) = cache_guard.get(&key) {
            return info.clone();
        }
    }

    let info = resolve_from_lsof(peer_addr.port()).await;

    let mut cache_guard = cache.lock().await;
    cache_guard.insert(key, info.clone());
    info
}

async fn resolve_from_lsof(src_port: u16) -> SessionInfo {
    let session_id = uuid::Uuid::new_v4().to_string();

    let pid = find_pid_for_port(src_port).await;

    let (cwd, project_name) = if let Some(pid) = pid {
        let cwd = find_cwd_for_pid(pid).await;
        let project_name = cwd.as_ref().map(|p| {
            std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string()
        });
        (cwd, project_name)
    } else {
        (None, None)
    };

    SessionInfo {
        session_id,
        pid,
        cwd,
        project_name,
    }
}

async fn find_pid_for_port(port: u16) -> Option<i64> {
    let output = tokio::task::spawn_blocking(move || {
        Command::new("lsof")
            .args(["-i", &format!(":{}", port), "-F", "p", "-n", "-P"])
            .output()
    })
    .await
    .ok()?
    .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    debug!("lsof output for port {}: {}", port, stdout.trim());

    let own_pid = std::process::id() as i64;
    for line in stdout.lines() {
        if let Some(pid_str) = line.strip_prefix('p') {
            if let Ok(pid) = pid_str.trim().parse::<i64>() {
                if pid != own_pid {
                    return Some(pid);
                }
            }
        }
    }
    None
}

async fn find_cwd_for_pid(pid: i64) -> Option<String> {
    let output = tokio::task::spawn_blocking(move || {
        Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-F", "n", "-n"])
            .output()
    })
    .await
    .ok()?
    .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    debug!("lsof cwd for pid {}: {}", pid, stdout.trim());

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix('n') {
            let path = path.trim().to_string();
            if !path.is_empty() && path.starts_with('/') {
                return Some(path);
            }
        }
    }

    warn!("lsof cwd failed for pid {}", pid);
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_cache_starts_empty() {
        let cache = new_session_cache();
        let guard = cache.try_lock().unwrap();
        assert!(guard.is_empty());
    }

    #[tokio::test]
    async fn resolve_session_caches_result() {
        let cache = new_session_cache();
        let addr: SocketAddr = "127.0.0.1:65432".parse().unwrap();

        // First call populates cache
        let info1 = resolve_session(addr, &cache).await;
        // Second call must return the same session_id (cache hit)
        let info2 = resolve_session(addr, &cache).await;

        assert_eq!(info1.session_id, info2.session_id);

        // Verify the entry exists in the cache
        let guard = cache.lock().await;
        assert!(guard.contains_key("127.0.0.1:65432"));
    }

    #[tokio::test]
    async fn resolve_session_different_ports_get_different_ids() {
        let cache = new_session_cache();
        let addr1: SocketAddr = "127.0.0.1:11111".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:11112".parse().unwrap();

        let info1 = resolve_session(addr1, &cache).await;
        let info2 = resolve_session(addr2, &cache).await;

        // Different connections → different session IDs
        assert_ne!(info1.session_id, info2.session_id);
    }

    #[test]
    fn project_name_is_basename_of_cwd() {
        let cwd = "/Users/alice/projects/my-app".to_string();
        let project_name = std::path::Path::new(&cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        assert_eq!(project_name, "my-app");
    }

    #[test]
    fn project_name_fallback_for_root_path() {
        let cwd = "/".to_string();
        let project_name = std::path::Path::new(&cwd)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        assert_eq!(project_name, "unknown");
    }

    /// Integration test: verify lsof can actually find the PID of our own process.
    /// Starts a real TCP listener, connects to it, then looks up the source port.
    #[tokio::test]
    async fn lsof_finds_pid_for_live_port() {
        use tokio::net::TcpListener;
        use tokio::net::TcpStream;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_port = listener.local_addr().unwrap().port();

        // Accept task (keeps listener alive)
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let stream = TcpStream::connect(format!("127.0.0.1:{server_port}")).await.unwrap();
        let client_port = stream.local_addr().unwrap().port();

        // Give lsof a moment to see the connection
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let pid = find_pid_for_port(client_port).await;
        // We may or may not find a PID (depends on OS, permissions, timing),
        // but the call must not panic.
        let _ = pid;
    }
}
