use futures_util::StreamExt;
use irc::client::prelude::*;

/// Run the IRC gateway loop with reconnection.
///
/// Receives messages from the IRC server stream, routes them to
/// trigger-specific payloads, and dispatches to the callback.
/// Automatically reconnects on disconnection with 30s backoff.
///
/// Privacy: Does NOT respond to CTCP VERSION/TIME/PING requests
/// to minimize bot fingerprinting (§2.9 social graph protection).
pub async fn gateway_loop(
    config: irc::client::data::Config,
    command_prefix: String,
    sasl_enabled: bool,
    dispatcher: std::sync::Arc<dyn Fn(serde_json::Value) + Send + Sync>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        // Check shutdown before (re)connecting
        if *shutdown.borrow() {
            break;
        }

        tracing::info!(
            server = config.server.as_deref().unwrap_or("unknown"),
            "connecting to IRC server"
        );

        let mut client = match Client::from_config(config.clone()).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "failed to connect to IRC server");
                jittered_backoff().await;
                continue;
            }
        };

        if let Err(e) = client.identify() {
            tracing::error!(error = %e, "failed to identify to IRC server");
            jittered_backoff().await;
            continue;
        }

        // SASL PLAIN auth if enabled (required by some networks for Tor/VPS connections)
        if sasl_enabled {
            if let Err(e) = client.send_sasl_plain() {
                tracing::warn!(error = %e, "SASL PLAIN auth failed — falling back to NickServ");
            } else {
                tracing::info!("SASL PLAIN auth completed");
            }
        }

        let mut stream = match client.stream() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "failed to create IRC message stream");
                jittered_backoff().await;
                continue;
            }
        };

        tracing::info!("IRC connected and identified");

        // Message receive loop
        loop {
            tokio::select! {
                msg_result = stream.next() => {
                    match msg_result {
                        Some(Ok(message)) => {
                            if let Some(payload) = route_message(&message, &command_prefix) {
                                dispatcher(payload);
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!(error = %e, "IRC stream error");
                            break; // reconnect
                        }
                        None => {
                            tracing::warn!("IRC stream ended");
                            break; // reconnect
                        }
                    }
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::info!("IRC gateway received shutdown signal");
                        let _ = client.send_quit("Springtale shutting down");
                        return;
                    }
                }
            }
        }

        tracing::info!("IRC disconnected, reconnecting in 30s");
        jittered_backoff().await;
    }

    tracing::info!("IRC gateway stopped");
}

/// Jittered reconnect backoff to prevent timing correlation (§2.9).
/// Base 30s + random 0-30s = 30-60s total delay.
async fn jittered_backoff() {
    let base = std::time::Duration::from_secs(30);
    let jitter = std::time::Duration::from_millis(rand::random::<u64>() % 30000);
    tokio::time::sleep(base + jitter).await;
}

/// Route an IRC message to a trigger-specific JSON payload.
/// Returns None for messages we don't care about (PING, numeric replies, etc.).
fn route_message(message: &Message, command_prefix: &str) -> Option<serde_json::Value> {
    let nick = match &message.prefix {
        Some(Prefix::Nickname(nick, _, _)) => nick.clone(),
        _ => return None, // Ignore server messages
    };

    match &message.command {
        Command::PRIVMSG(target, text) => {
            // Check if it's a command (starts with prefix)
            if let Some(without_prefix) = text.strip_prefix(command_prefix) {
                let (command, args) = match without_prefix.split_once(' ') {
                    Some((cmd, rest)) => (cmd.to_owned(), rest.to_owned()),
                    None => (without_prefix.to_owned(), String::new()),
                };

                Some(serde_json::json!({
                    "trigger": "command_received",
                    "nick": nick,
                    "target": target,
                    "command": command,
                    "args": args,
                    "message": text,
                    "pubkey": nick,  // IRC uses nick as user identifier
                }))
            } else {
                Some(serde_json::json!({
                    "trigger": "message_received",
                    "nick": nick,
                    "target": target,
                    "message": text,
                    "pubkey": nick,
                }))
            }
        }

        Command::JOIN(channel, _, _) => Some(serde_json::json!({
            "trigger": "user_joined",
            "nick": nick,
            "channel": channel,
            "pubkey": nick,
        })),

        Command::PART(channel, reason) => Some(serde_json::json!({
            "trigger": "user_parted",
            "nick": nick,
            "channel": channel,
            "reason": reason.as_deref().unwrap_or(""),
            "pubkey": nick,
        })),

        Command::TOPIC(channel, topic) => Some(serde_json::json!({
            "trigger": "topic_changed",
            "nick": nick,
            "channel": channel,
            "topic": topic.as_deref().unwrap_or(""),
            "pubkey": nick,
        })),

        // Privacy: Do NOT respond to CTCP VERSION/TIME/PING
        // These fingerprint the bot and reveal implementation details
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_privmsg(nick: &str, target: &str, text: &str) -> Message {
        Message {
            tags: None,
            prefix: Some(Prefix::Nickname(nick.into(), "user".into(), "host".into())),
            command: Command::PRIVMSG(target.into(), text.into()),
        }
    }

    #[test]
    fn test_route_privmsg() {
        let msg = make_privmsg("alice", "#general", "hello world");
        let payload = route_message(&msg, "!").unwrap();
        assert_eq!(payload["trigger"], "message_received");
        assert_eq!(payload["nick"], "alice");
        assert_eq!(payload["message"], "hello world");
    }

    #[test]
    fn test_route_command() {
        let msg = make_privmsg("bob", "#bots", "!search tokyo weather");
        let payload = route_message(&msg, "!").unwrap();
        assert_eq!(payload["trigger"], "command_received");
        assert_eq!(payload["command"], "search");
        assert_eq!(payload["args"], "tokyo weather");
    }

    #[test]
    fn test_route_command_no_args() {
        let msg = make_privmsg("bob", "#bots", "!help");
        let payload = route_message(&msg, "!").unwrap();
        assert_eq!(payload["trigger"], "command_received");
        assert_eq!(payload["command"], "help");
        assert_eq!(payload["args"], "");
    }

    #[test]
    fn test_route_join() {
        let msg = Message {
            tags: None,
            prefix: Some(Prefix::Nickname("carol".into(), "u".into(), "h".into())),
            command: Command::JOIN("#new".into(), None, None),
        };
        let payload = route_message(&msg, "!").unwrap();
        assert_eq!(payload["trigger"], "user_joined");
        assert_eq!(payload["nick"], "carol");
    }

    #[test]
    fn test_route_server_message_ignored() {
        let msg = Message {
            tags: None,
            prefix: Some(Prefix::ServerName("irc.server.com".into())),
            command: Command::PRIVMSG("#chan".into(), "server msg".into()),
        };
        assert!(route_message(&msg, "!").is_none());
    }
}
