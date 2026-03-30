use serde::Deserialize;

/// Bot persona configuration. Loaded from `[bot]` section of `springtale.toml`.
/// Not user-configurable — admin sets it.
#[derive(Debug, Clone, Deserialize)]
pub struct BotPersona {
    /// Bot display name. Default: "Springtale".
    #[serde(default = "default_name")]
    pub name: String,
    /// Response tone hint. Default: "neutral".
    #[serde(default = "default_tone")]
    pub tone: String,
    /// Command prefix character. Default: '/'.
    #[serde(default = "default_prefix")]
    pub prefix: char,
}

fn default_name() -> String {
    "Springtale".to_owned()
}

fn default_tone() -> String {
    "neutral".to_owned()
}

fn default_prefix() -> char {
    '/'
}

impl Default for BotPersona {
    fn default() -> Self {
        Self {
            name: default_name(),
            tone: default_tone(),
            prefix: default_prefix(),
        }
    }
}
