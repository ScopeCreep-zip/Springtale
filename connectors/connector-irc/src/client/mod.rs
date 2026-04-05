use async_trait::async_trait;

use crate::error::IrcError;

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
}

/// Concrete IRC client wrapping the `irc` crate's Sender.
///
/// Applies publish-side jitter before every send to obscure
/// activity timing from network observers (§2.9 protection).
pub struct IrcClient {
    sender: irc::client::Sender,
    jitter_secs: u64,
}

impl IrcClient {
    pub fn new(sender: irc::client::Sender, jitter_secs: u64) -> Self {
        Self {
            sender,
            jitter_secs,
        }
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
    }
}
