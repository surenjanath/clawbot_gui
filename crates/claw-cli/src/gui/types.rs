use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct OllamaSettings {
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    #[serde(default)]
    pub enable_tools: bool,
}

impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "llama3.1:latest".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
            enable_tools: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub ollama: OllamaSettings,
    pub system_prompt: String,
    pub dark_mode: bool,
    pub font_size: f32,
    /// Stream tokens from Ollama when tools are off.
    #[serde(default = "default_true")]
    pub stream_responses: bool,
    /// Max user+assistant pairs in context (0 = unlimited).
    #[serde(default)]
    pub context_max_messages: usize,
    /// Max total chars of user+assistant content (0 = unlimited).
    #[serde(default)]
    pub context_max_chars: usize,
    /// WebFetch / WebSearch via the `tools` crate (network).
    #[serde(default = "default_true")]
    pub enable_research_tools: bool,
    /// Read-only `gui_read_file` / `gui_glob_search` under `workspace_root`.
    #[serde(default)]
    pub enable_workspace_tools: bool,
    /// Include MCP tools after a successful Refresh MCP (stdio servers from Claw config).
    #[serde(default)]
    pub enable_mcp_tools: bool,
    /// Workspace root for file tools; empty = process current directory when the GUI starts.
    #[serde(default)]
    pub workspace_root: String,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ollama: OllamaSettings::default(),
            system_prompt: String::new(),
            dark_mode: false,
            font_size: 14.0,
            stream_responses: true,
            context_max_messages: 0,
            context_max_chars: 0,
            enable_research_tools: true,
            enable_workspace_tools: false,
            enable_mcp_tools: false,
            workspace_root: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PromptPreset {
    pub name: String,
    pub text: String,
}

impl Default for PromptPreset {
    fn default() -> Self {
        Self {
            name: String::new(),
            text: String::new(),
        }
    }
}

/// Persisted GUI configuration (extends runtime `AppSettings` with presets).
#[derive(Clone, Serialize, Deserialize)]
pub struct GuiConfigFile {
    #[serde(flatten)]
    pub settings: AppSettings,
    #[serde(default)]
    pub prompt_presets: Vec<PromptPreset>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub id: usize,
    #[allow(dead_code)]
    pub timestamp: String,
}

impl ChatMessage {
    pub fn user(content: String, id: usize) -> Self {
        Self {
            role: "user".to_string(),
            content,
            id,
            timestamp: Self::now_time(),
        }
    }

    pub fn assistant(content: String, id: usize) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            id,
            timestamp: Self::now_time(),
        }
    }

    pub fn now_time() -> String {
        let now = std::time::SystemTime::now();
        let d = now.duration_since(std::time::UNIX_EPOCH).unwrap();
        let secs = d.as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SessionFile {
    pub messages: Vec<ChatMessage>,
    /// Per-chat system prompt override (empty = use global from settings).
    #[serde(default)]
    pub session_system_prompt: String,
}
