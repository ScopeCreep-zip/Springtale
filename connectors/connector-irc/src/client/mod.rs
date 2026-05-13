use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::IrcError;

/// An IRC addressable target the bot has been in this session.
///
/// `kind` is `"channel"` (then `id` starts with `#`/`&`) or `"user"`
/// (then `id` is a bare nick).
#[derive(Debug, Clone)]
pub struct DiscoveredIrcTarget {
    pub network: String,
    pub id: String,
    pub kind: String,
}

/// Trait defining the IRC API surface used by actions.
/// Actions depend on this trait — enables mock testing.
#[async_trait]
pub trait IrcApi: Send + Sync {
    /// Send a message to a channel or user.
    async fn send_message(&self, target: &str, message: &str) -> Result<(), IrcError>;

    /// Join a channel.
    async fn join_channel(&self, channel: &str) -> Result<(), IrcError>;

    /// Leave a channel.
    async fn part_channel(&self, channel: &str) -> Result<(), IrcError>;

    /// Set channel topic.
    async fn set_topic(&self, channel: &str, topic: &str) -> Result<(), IrcError>;

    /// Send a /me action.
    async fn send_action(&self, target: &str, action: &str) -> Result<(), IrcError>;

    /// Enumerate every target the bot has interacted with this session
    /// — channels it has joined plus nicks it has DM'd. No network-wide
    /// `LIST` (privacy + bandwidth).
    async fn list_destinations(&self) -> Result<Vec<DiscoveredIrcTarget>, IrcError>;
}

/// Concrete IRC client wrapping the `irc` crate's Sender.
///
/// Applies publish-side jitter before every send to obscure
/// activity timing from network observers (§2.9 protection).
pub struct IrcClient {
    sender: irc::client::Sender,
    jitter_secs: u64,
    /// Network identifier (the IRC server hostname). Embedded in
    /// every emitted workspace key.
    network: String,
    /// In-memory snapshot of channels the bot has joined this session.
    /// Initialized from the config's `channels` list at startup; the
    /// gateway calls `record_join` / `record_part` as JOIN/PART events
    /// fire so the snapshot reflects reality.
    joined_channels: Arc<RwLock<HashSet<String>>>,
    /// Nicks the bot has DM'd or received DMs from this session.
    /// Populated by the gateway via `record_dm_target`.
    dm_targets: Arc<RwLock<HashSet<String>>>,
}

impl IrcClient {
    pub fn new(
        sender: irc::client::Sender,
        jitter_secs: u64,
        network: String,
        initial_channels: Vec<String>,
    ) -> Self {
        let channels: HashSet<String> = initial_channels.into_iter().collect();
        Self {
            sender,
            jitter_secs,
            network,
            joined_channels: Arc::new(RwLock::new(channels)),
            dm_targets: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Record that the bot has joined `channel` — called by the gateway
    /// when a JOIN event for our nick fires.
    pub async fn record_join(&self, channel: String) {
        self.joined_channels.write().await.insert(channel);
    }

    /// Record that the bot has parted `channel` — called by the gateway
    /// when a PART/KICK event for our nick fires.
    pub async fn record_part(&self, channel: &str) {
        self.joined_channels.write().await.remove(channel);
    }

    /// Record that `nick` has been DM'd or has DM'd us — called by the
    /// gateway when a PRIVMSG with a non-channel target involving our
    /// nick fires.
    pub async fn record_dm_target(&self, nick: String) {
        self.dm_targets.write().await.insert(nick);
    }

    /// Apply publish-side jitter before sending.
    async fn apply_jitter(&self) {
        if self.jitter_secs > 0 {
            let jitter = rand::random::<u64>() % self.jitter_secs;
            tokio::time::sleep(std::time::Duration::from_secs(jitter)).await;
        }
    }
}

#[async_trait]
impl IrcApi for IrcClient {
    async fn send_message(&self, target: &str, message: &str) -> Result<(), IrcError> {
        self.apply_jitter().await;
        self.sender
            .send_privmsg(target, message)
            .map_err(|e| IrcError::SendFailed(format!("PRIVMSG failed: {e}")))
    }

    async fn join_channel(&self, channel: &str) -> Result<(), IrcError> {
        self.sender
            .send_join(channel)
            .map_err(|e| IrcError::SendFailed(format!("JOIN failed: {e}")))
    }

    async fn part_channel(&self, channel: &str) -> Result<(), IrcError> {
        self.sender
            .send_part(channel)
            .map_err(|e| IrcError::SendFailed(format!("PART failed: {e}")))
    }

    async fn set_topic(&self, channel: &str, topic: &str) -> Result<(), IrcError> {
        self.sender
            .send_topic(channel, topic)
            .map_err(|e| IrcError::SendFailed(format!("TOPIC failed: {e}")))
    }

    async fn send_action(&self, target: &str, action: &str) -> Result<(), IrcError> {
        self.apply_jitter().await;
        // Privacy: Implement ACTION via raw PRIVMSG with CTCP wrapping.
        // The `ctcp` feature is DISABLED to prevent auto-responding to
        // VERSION/TIME/PING/FINGER queries (which leak timezone, client
        // identity, and enable bot fingerprinting — §2.9 violation).
        // CTCP ACTION is just: PRIVMSG target :\x01ACTION text\x01
        let ctcp_msg = format!("\x01ACTION {action}\x01");
        self.sender
            .send_privmsg(target, &ctcp_msg)
            .map_err(|e| IrcError::SendFailed(format!("ACTION failed: {e}")))
    }

    async fn list_destinations(&self) -> Result<Vec<DiscoveredIrcTarget>, IrcError> {
        let mut out = Vec::new();
        let channels = self.joined_channels.read().await.clone();
        for ch in channels {
            out.push(DiscoveredIrcTarget {
                network: self.network.clone(),
                id: ch,
                kind: "channel".to_owned(),
            });
        }
        let dms = self.dm_targets.read().await.clone();
        for nick in dms {
            out.push(DiscoveredIrcTarget {
                network: self.network.clone(),
                id: nick,
                kind: "user".to_owned(),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;

    pub struct MockIrcApi;

    #[async_trait]
    impl IrcApi for MockIrcApi {
        async fn send_message(&self, _: &str, _: &str) -> Result<(), IrcError> {
            Ok(())
        }
        async fn join_channel(&self, _: &str) -> Result<(), IrcError> {
            Ok(())
        }
        async fn part_channel(&self, _: &str) -> Result<(), IrcError> {
            Ok(())
        }
        async fn set_topic(&self, _: &str, _: &str) -> Result<(), IrcError> {
            Ok(())
        }
        async fn send_action(&self, _: &str, _: &str) -> Result<(), IrcError> {
            Ok(())
        }
        async fn list_destinations(&self) -> Result<Vec<DiscoveredIrcTarget>, IrcError> {
            Ok(vec![
                DiscoveredIrcTarget {
                    network: "irc.libera.chat".to_owned(),
                    id: "#springtale".to_owned(),
                    kind: "channel".to_owned(),
                },
                DiscoveredIrcTarget {
                    network: "irc.libera.chat".to_owned(),
                    id: "#bots".to_owned(),
                    kind: "channel".to_owned(),
                },
                DiscoveredIrcTarget {
                    network: "irc.libera.chat".to_owned(),
                    id: "alice".to_owned(),
                    kind: "user".to_owned(),
                },
            ])
        }
    }
}
