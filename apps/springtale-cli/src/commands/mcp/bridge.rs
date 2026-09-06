//! The stdin → daemon → stdout loop.
//!
//! MCP's stdio transport is newline-delimited JSON-RPC: "messages are
//! delimited by newlines, and MUST NOT contain embedded newlines". This
//! loop reads one, hands it to a [`McpTransport`], and writes back at
//! most one line. Nothing here knows what a tool is.

use std::future::Future;

use anyhow::Result;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// JSON-RPC "Parse error" — the message was not valid JSON.
const PARSE_ERROR: i64 = -32700;
/// JSON-RPC "Internal error" — the daemon could not be reached, or
/// answered with something that is not a message.
const INTERNAL_ERROR: i64 = -32603;

/// Where a stdio message goes. One implementation talks HTTP to
/// springtaled ([`super::DaemonTransport`]); the test uses a stub.
pub trait McpTransport {
    /// Forward `message` verbatim and return the daemon's reply body,
    /// or `None` when the daemon accepted it without one (its answer to
    /// a notification).
    fn send(&self, message: String) -> impl Future<Output = Result<Option<String>>> + Send;
}

/// Pump `reader` into `transport` and its replies into `writer` until
/// the reader hits EOF (the client closed the pipe, which is how the
/// stdio transport says "shut down").
pub async fn bridge<R, W, T>(reader: R, mut writer: W, transport: &T) -> Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
    T: McpTransport,
{
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        let message = line.trim();
        if message.is_empty() {
            continue;
        }

        // The only inspection this bridge performs: a message with an
        // `id` is a request and is owed exactly one reply; one without
        // is a notification and gets none. That is stdio framing, not
        // protocol knowledge.
        let id = match serde_json::from_str::<Value>(message) {
            Ok(value) => value.get("id").cloned(),
            Err(err) => {
                // Unparseable input has no id to echo, so `null` — the
                // JSON-RPC 2.0 rule for a parse error.
                let body = error_response(&Value::Null, PARSE_ERROR, &err.to_string());
                write_line(&mut writer, &body).await?;
                continue;
            }
        };

        let reply = transport.send(message.to_owned()).await;
        let Some(id) = id else {
            // Notification: never write a response. A failure has
            // nowhere to go but stderr, which the client treats as logs.
            if let Err(err) = reply {
                eprintln!("springtale mcp: notification not delivered: {err:#}");
            }
            continue;
        };

        let body = match reply {
            Ok(Some(body)) => single_line(&body),
            Ok(None) => error_response(
                &id,
                INTERNAL_ERROR,
                "daemon accepted the request without a response",
            ),
            Err(err) => error_response(&id, INTERNAL_ERROR, &format!("{err:#}")),
        };
        write_line(&mut writer, &body).await?;
    }
    Ok(())
}

/// Collapse a response body onto one line.
///
/// Raw newlines in JSON are whitespace between tokens — a newline
/// *inside* a string must be escaped as `\n` — so dropping them cannot
/// change the message, and the stdio framing requires it.
fn single_line(body: &str) -> String {
    body.replace(['\n', '\r'], "")
}

/// Build a JSON-RPC error response carrying the original request id.
fn error_response(id: &Value, code: i64, message: &str) -> String {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    });
    // A `serde_json::Value` built from owned data cannot fail to
    // serialize, but the CLI still never unwraps.
    serde_json::to_string(&body).unwrap_or_else(|_| {
        format!(r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":{INTERNAL_ERROR},"message":"serialization failed"}}}}"#)
    })
}

/// Write one message and flush — the client is waiting on this byte.
async fn write_line<W: AsyncWrite + Unpin>(writer: &mut W, body: &str) -> Result<()> {
    writer.write_all(body.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stub daemon: records what it was handed, answers with `reply`.
    struct StubTransport {
        reply: &'static str,
        seen: std::sync::Mutex<Vec<String>>,
    }

    impl McpTransport for StubTransport {
        async fn send(&self, message: String) -> Result<Option<String>> {
            if let Ok(mut seen) = self.seen.lock() {
                seen.push(message);
            }
            Ok(Some(self.reply.to_owned()))
        }
    }

    #[tokio::test]
    async fn test_bridge_request_forwards_body_and_writes_daemon_response() {
        let transport = StubTransport {
            reply: r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#,
            seen: std::sync::Mutex::new(Vec::new()),
        };
        let request = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n";
        let mut out: Vec<u8> = Vec::new();

        bridge(request.as_bytes(), &mut out, &transport)
            .await
            .expect("bridge runs to EOF");

        let forwarded = transport.seen.lock().expect("stub lock");
        assert_eq!(forwarded.as_slice(), [request.trim().to_owned()]);
        assert_eq!(
            String::from_utf8(out).expect("utf-8 stdout"),
            format!("{}\n", transport.reply)
        );
    }
}
