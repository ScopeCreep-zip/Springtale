use serde::{Deserialize, Serialize};

/// Ollama /api/chat request body.
#[derive(Debug, Serialize)]
pub struct OllamaChatRequest {
    pub model: String,
    pub messages: Vec<OllamaChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
pub struct OllamaChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<u32>,
}

/// Ollama /api/chat response (non-streaming or final streaming chunk).
#[derive(Debug, Deserialize)]
pub struct OllamaChatResponse {
    pub message: Option<OllamaResponseMessage>,
    pub done: bool,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u32>,
    #[serde(default)]
    pub eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct OllamaResponseMessage {
    pub role: String,
    pub content: String,
}

/// Ollama /api/tags response (list models).
#[derive(Debug, Deserialize)]
pub struct OllamaTagsResponse {
    pub models: Option<Vec<OllamaModel>>,
}

#[derive(Debug, Deserialize)]
pub struct OllamaModel {
    pub name: String,
}

/// Ollama adapter configuration.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OllamaConfig {
    /// Base URL for the Ollama API. Default: "http://127.0.0.1:11434".
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Model name. Default: "llama3.2".
    #[serde(default = "default_model")]
    pub model: String,
}

fn default_base_url() -> String {
    "http://127.0.0.1:11434".to_owned()
}

fn default_model() -> String {
    "llama3.2".to_owned()
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            model: default_model(),
        }
    }
}
