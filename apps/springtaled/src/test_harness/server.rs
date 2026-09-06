//! A real springtaled listening on an ephemeral port.

use tokio::sync::mpsc;

use springtale_cooperation::command::FormationCommand;

use super::app::TestApp;

/// A running management API bound to `127.0.0.1:0`.
///
/// The server task is aborted when this value is dropped, so a test that
/// returns early does not leak a listener.
pub struct TestServer {
    /// `http://127.0.0.1:{port}` — what a client's `api.bind` must point at.
    pub base_url: String,
    /// Hex-encoded API token the daemon will accept as a bearer token.
    pub token_hex: String,
    /// Formation commands the API enqueued. Held open so `send` succeeds.
    pub formation_cmd_rx: mpsc::Receiver<FormationCommand>,
    task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    /// Bind an ephemeral port and serve the real router on it.
    ///
    /// Must be called from inside a Tokio runtime.
    pub async fn start() -> std::io::Result<Self> {
        let TestApp {
            router,
            token_hex,
            formation_cmd_rx,
        } = TestApp::build(true);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self {
            base_url: format!("http://{addr}"),
            token_hex,
            formation_cmd_rx,
            task,
        })
    }

    /// The `api.bind` value (`127.0.0.1:{port}`) for a client config file.
    pub fn bind_addr(&self) -> String {
        self.base_url.trim_start_matches("http://").to_owned()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
