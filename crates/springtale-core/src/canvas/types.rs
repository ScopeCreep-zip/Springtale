//! Canvas data types — structured content the bot pushes to the UI.
//!
//! Per ARCHITECTURE.md: "The agent writes structured data to a Canvas
//! state object via IPC events; the SolidJS frontend renders it reactively."
//!
//! Security: Canvas receives **typed data**, never raw HTML. No `Html`
//! variant exists by design. The frontend renders these via SolidJS
//! component constructors — never innerHTML or dangerouslySetInnerHTML.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single content block on the Canvas.
///
/// Each variant maps to a specific SolidJS component in the frontend.
/// All rendering is auto-escaped — no XSS risk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CanvasBlock {
    /// Plain text paragraph.
    Text { id: String, content: String },

    /// Data table with headers and rows.
    Table {
        id: String,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },

    /// Key-value pairs (rendered as `<dl>` in frontend).
    KeyValue {
        id: String,
        pairs: Vec<(String, String)>,
    },

    /// Status card with label, state indicator, and optional message.
    Status {
        id: String,
        label: String,
        state: StatusState,
        message: Option<String>,
    },
}

/// Status indicator for status cards.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatusState {
    Info,
    Success,
    Warning,
    Error,
    Loading,
}

/// Full canvas state — snapshot of all blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasState {
    pub blocks: Vec<CanvasBlock>,
    pub title: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            blocks: Vec::new(),
            title: None,
            updated_at: Utc::now(),
        }
    }
}

/// Delta update to the canvas — avoids resending all blocks on every change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum CanvasUpdate {
    /// Replace all blocks.
    SetBlocks { blocks: Vec<CanvasBlock> },

    /// Update a single block by ID.
    UpdateBlock { id: String, block: CanvasBlock },

    /// Remove a block by ID.
    RemoveBlock { id: String },

    /// Clear all blocks.
    Clear,
}

impl CanvasState {
    /// Apply a delta update to the canvas state.
    pub fn apply(&mut self, update: &CanvasUpdate) {
        match update {
            CanvasUpdate::SetBlocks { blocks } => {
                self.blocks = blocks.clone();
            }
            CanvasUpdate::UpdateBlock { id, block } => {
                if let Some(existing) = self.blocks.iter_mut().find(|b| block_id(b) == id) {
                    *existing = block.clone();
                } else {
                    self.blocks.push(block.clone());
                }
            }
            CanvasUpdate::RemoveBlock { id } => {
                self.blocks.retain(|b| block_id(b) != id);
            }
            CanvasUpdate::Clear => {
                self.blocks.clear();
            }
        }
        self.updated_at = Utc::now();
    }
}

/// Extract the ID from any canvas block variant.
fn block_id(block: &CanvasBlock) -> &str {
    match block {
        CanvasBlock::Text { id, .. }
        | CanvasBlock::Table { id, .. }
        | CanvasBlock::KeyValue { id, .. }
        | CanvasBlock::Status { id, .. } => id,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_apply_set_blocks() {
        let mut state = CanvasState::default();
        let update = CanvasUpdate::SetBlocks {
            blocks: vec![CanvasBlock::Text {
                id: "1".to_owned(),
                content: "hello".to_owned(),
            }],
        };
        state.apply(&update);
        assert_eq!(state.blocks.len(), 1);
    }

    #[test]
    fn test_canvas_apply_update_block() {
        let mut state = CanvasState {
            blocks: vec![CanvasBlock::Text {
                id: "1".to_owned(),
                content: "old".to_owned(),
            }],
            ..Default::default()
        };
        let update = CanvasUpdate::UpdateBlock {
            id: "1".to_owned(),
            block: CanvasBlock::Text {
                id: "1".to_owned(),
                content: "new".to_owned(),
            },
        };
        state.apply(&update);
        assert_eq!(state.blocks.len(), 1);
        if let CanvasBlock::Text { content, .. } = &state.blocks[0] {
            assert_eq!(content, "new");
        }
    }

    #[test]
    fn test_canvas_apply_remove_block() {
        let mut state = CanvasState {
            blocks: vec![
                CanvasBlock::Text {
                    id: "1".to_owned(),
                    content: "a".to_owned(),
                },
                CanvasBlock::Text {
                    id: "2".to_owned(),
                    content: "b".to_owned(),
                },
            ],
            ..Default::default()
        };
        state.apply(&CanvasUpdate::RemoveBlock { id: "1".to_owned() });
        assert_eq!(state.blocks.len(), 1);
        assert_eq!(block_id(&state.blocks[0]), "2");
    }

    #[test]
    fn test_canvas_apply_clear() {
        let mut state = CanvasState {
            blocks: vec![CanvasBlock::Text {
                id: "1".to_owned(),
                content: "a".to_owned(),
            }],
            ..Default::default()
        };
        state.apply(&CanvasUpdate::Clear);
        assert!(state.blocks.is_empty());
    }

    #[test]
    fn test_canvas_serialization_roundtrip() {
        let state = CanvasState {
            blocks: vec![
                CanvasBlock::Text {
                    id: "t1".to_owned(),
                    content: "Hello".to_owned(),
                },
                CanvasBlock::Table {
                    id: "t2".to_owned(),
                    headers: vec!["Name".to_owned(), "Value".to_owned()],
                    rows: vec![vec!["key".to_owned(), "val".to_owned()]],
                },
                CanvasBlock::KeyValue {
                    id: "kv1".to_owned(),
                    pairs: vec![("status".to_owned(), "ok".to_owned())],
                },
                CanvasBlock::Status {
                    id: "s1".to_owned(),
                    label: "Health".to_owned(),
                    state: StatusState::Success,
                    message: Some("All good".to_owned()),
                },
            ],
            title: Some("Test Canvas".to_owned()),
            ..Default::default()
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: CanvasState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.blocks.len(), 4);
        assert_eq!(deserialized.title, Some("Test Canvas".to_owned()));
    }
}
