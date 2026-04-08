//! Ollama OpenAI-style tools for the GUI: builtins, research (WebFetch/WebSearch), workspace read-only, MCP.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tools::execute_tool;

use runtime::{
    glob_search, read_file, ConfigLoader, ManagedMcpTool, McpServerManager, McpToolCallResult,
    ScopedMcpServerConfig,
};

const MAX_MCP_TOOLS: usize = 48;
const MAX_DESC_CHARS: usize = 900;

/// Cached MCP tool (from `tools/list`), used to build OpenAI `tools` JSON.
#[derive(Clone, Debug)]
pub struct GuiMcpToolEntry {
    pub qualified_name: String,
    pub description: String,
    pub parameters: Value,
}

impl GuiMcpToolEntry {
    fn from_managed(t: &ManagedMcpTool) -> Self {
        let description = t
            .tool
            .description
            .clone()
            .unwrap_or_else(|| format!("MCP tool `{}` on server `{}`", t.raw_name, t.server_name));
        let parameters = t
            .tool
            .input_schema
            .clone()
            .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
        Self {
            qualified_name: t.qualified_name.clone(),
            description: truncate_str(&description, MAX_DESC_CHARS),
            parameters: normalize_json_schema(parameters),
        }
    }

    fn to_openai_json(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.qualified_name,
                "description": &self.description,
                "parameters": &self.parameters
            }
        })
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn normalize_json_schema(mut schema: Value) -> Value {
    if !schema.is_object() {
        return json!({ "type": "object", "properties": {} });
    }
    let obj = schema.as_object_mut().expect("checked");
    if obj.get("type").is_none() {
        obj.insert("type".to_string(), json!("object"));
    }
    if obj.get("properties").is_none() {
        obj.insert("properties".to_string(), json!({}));
    }
    schema
}

fn resolve_under_workspace(root: &Path, user_path: &str) -> Result<PathBuf, String> {
    let trimmed = user_path.trim();
    if trimmed.is_empty() {
        return Err("path must not be empty".to_string());
    }
    if trimmed.contains("..") {
        return Err("path must not contain '..'".to_string());
    }
    let joined = root.join(trimmed.trim_start_matches(['/', '\\']));
    let abs = joined
        .canonicalize()
        .map_err(|e| format!("invalid path: {e}"))?;
    let root_canon = root
        .canonicalize()
        .map_err(|e| format!("workspace root: {e}"))?;
    if !abs.starts_with(&root_canon) {
        return Err("path must stay inside the workspace root".to_string());
    }
    Ok(abs)
}

/// Discover stdio MCP tools from Claw config (same sources as the REPL). Spawns servers briefly.
/// Returns tool entries, shared server map for later `tools/call`, and a status note.
pub fn discover_mcp_tools_blocking() -> Result<
    (
        Vec<GuiMcpToolEntry>,
        Arc<BTreeMap<String, ScopedMcpServerConfig>>,
        String,
    ),
    String,
> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let loader = ConfigLoader::default_for(&cwd);
    let config = loader.load().map_err(|e| e.to_string())?;
    let servers = config.mcp().servers().clone();
    let servers_arc = Arc::new(servers.clone());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    rt.block_on(async move {
        let mut manager = McpServerManager::from_servers(&servers);
        let discovered = manager.discover_tools().await.map_err(|e| e.to_string())?;
        let unsupported: Vec<String> = manager
            .unsupported_servers()
            .iter()
            .map(|u| format!("{} ({:?}): {}", u.server_name, u.transport, u.reason))
            .collect();
        let _ = manager.shutdown().await;

        let mut entries: Vec<GuiMcpToolEntry> =
            discovered.iter().map(GuiMcpToolEntry::from_managed).collect();
        entries.truncate(MAX_MCP_TOOLS);

        let note = if unsupported.is_empty() {
            format!("{} MCP tool(s) from stdio servers.", entries.len())
        } else {
            format!(
                "{} MCP tool(s). Skipped non-stdio servers:\n{}",
                entries.len(),
                unsupported.join("\n")
            )
        };
        Ok((entries, servers_arc, note))
    })
}

fn format_mcp_result(result: McpToolCallResult) -> String {
    if result.is_error == Some(true) {
        return format!("MCP error: {}", serde_json::to_string(&result).unwrap_or_default());
    }
    let mut parts = Vec::new();
    for block in &result.content {
        if block.kind == "text" {
            if let Some(t) = block.data.get("text").and_then(|v| v.as_str()) {
                parts.push(t.to_string());
            }
        }
    }
    if parts.is_empty() {
        if let Some(sc) = &result.structured_content {
            return serde_json::to_string_pretty(sc).unwrap_or_else(|_| sc.to_string());
        }
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| format!("{result:?}"))
    } else {
        parts.join("\n")
    }
}

fn call_mcp_tool_blocking(
    servers: &BTreeMap<String, ScopedMcpServerConfig>,
    qualified_name: &str,
    arguments: &str,
) -> Result<String, String> {
    let args: Option<Value> = if arguments.trim().is_empty() {
        None
    } else {
        Some(serde_json::from_str(arguments).map_err(|e| format!("invalid JSON arguments: {e}"))?)
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    rt.block_on(async move {
        let mut manager = McpServerManager::from_servers(servers);
        manager
            .discover_tools()
            .await
            .map_err(|e| format!("MCP discover: {e}"))?;
        let response = manager
            .call_tool(qualified_name, args)
            .await
            .map_err(|e| format!("MCP call: {e}"))?;
        let _ = manager.shutdown().await;

        if let Some(err) = response.error {
            return Err(format!("MCP JSON-RPC: {} ({})", err.message, err.code));
        }
        let result = response
            .result
            .ok_or_else(|| "MCP empty result".to_string())?;
        Ok(format_mcp_result(result))
    })
}

/// Built-in + optional research + workspace + MCP tool definitions for Ollama.
#[derive(Clone, Debug)]
pub struct GuiToolDispatcher {
    pub enable_research_tools: bool,
    pub enable_workspace_tools: bool,
    pub enable_mcp_tools: bool,
    pub workspace_root: PathBuf,
    pub mcp_servers: Option<Arc<BTreeMap<String, ScopedMcpServerConfig>>>,
    pub mcp_tools: Arc<Vec<GuiMcpToolEntry>>,
}

impl Default for GuiToolDispatcher {
    fn default() -> Self {
        Self {
            enable_research_tools: true,
            enable_workspace_tools: false,
            enable_mcp_tools: false,
            workspace_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            mcp_servers: None,
            mcp_tools: Arc::new(Vec::new()),
        }
    }
}

impl GuiToolDispatcher {
    pub fn definitions(&self) -> Vec<Value> {
        let mut tools = builtin_tool_definitions();
        if self.enable_research_tools {
            tools.extend(research_tool_definitions());
        }
        if self.enable_workspace_tools {
            tools.extend(workspace_tool_definitions());
        }
        if self.enable_mcp_tools {
            for entry in self.mcp_tools.iter() {
                tools.push(entry.to_openai_json());
            }
        }
        tools
    }

    pub fn invoke(&self, name: &str, arguments: &str) -> Result<String, String> {
        match name {
            "get_current_time" | "word_count" | "math_add" => run_builtin_only(name, arguments),
            "gui_web_fetch" => self.run_web_fetch(arguments),
            "gui_web_search" => self.run_web_search(arguments),
            "gui_read_file" => self.run_read_file(arguments),
            "gui_glob_search" => self.run_glob(arguments),
            n if n.starts_with("mcp__") => self.run_mcp(n, arguments),
            _ => Err(format!("unknown tool '{name}'")),
        }
    }

    fn run_web_fetch(&self, arguments: &str) -> Result<String, String> {
        if !self.enable_research_tools {
            return Err("research tools are disabled".to_string());
        }
        let v: Value = serde_json::from_str(arguments).map_err(|e| e.to_string())?;
        execute_tool("WebFetch", &v)
    }

    fn run_web_search(&self, arguments: &str) -> Result<String, String> {
        if !self.enable_research_tools {
            return Err("research tools are disabled".to_string());
        }
        let v: Value = serde_json::from_str(arguments).map_err(|e| e.to_string())?;
        execute_tool("WebSearch", &v)
    }

    fn run_read_file(&self, arguments: &str) -> Result<String, String> {
        if !self.enable_workspace_tools {
            return Err("workspace tools are disabled".to_string());
        }
        let v: Value = serde_json::from_str(arguments).map_err(|e| e.to_string())?;
        let path = v
            .get("path")
            .and_then(|p| p.as_str())
            .ok_or_else(|| "missing path".to_string())?;
        let abs = resolve_under_workspace(&self.workspace_root, path)?;
        let out = read_file(abs.to_str().ok_or("path utf-8")?, None, None).map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
    }

    fn run_glob(&self, arguments: &str) -> Result<String, String> {
        if !self.enable_workspace_tools {
            return Err("workspace tools are disabled".to_string());
        }
        let v: Value = serde_json::from_str(arguments).map_err(|e| e.to_string())?;
        let pattern = v
            .get("pattern")
            .and_then(|p| p.as_str())
            .ok_or_else(|| "missing pattern".to_string())?;
        let sub = v.get("path").and_then(|p| p.as_str()).unwrap_or(".");
        let base = resolve_under_workspace(&self.workspace_root, sub)?;
        let base_str = base.to_str().ok_or("path utf-8")?.to_string();
        let out = glob_search(pattern, Some(&base_str)).map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&out).map_err(|e| e.to_string())
    }

    fn run_mcp(&self, qualified_name: &str, arguments: &str) -> Result<String, String> {
        if !self.enable_mcp_tools {
            return Err("MCP tools are disabled".to_string());
        }
        let servers = self
            .mcp_servers
            .as_ref()
            .ok_or_else(|| "MCP servers not loaded — use Refresh MCP in the GUI".to_string())?;
        call_mcp_tool_blocking(servers.as_ref(), qualified_name, arguments)
    }
}

fn builtin_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "get_current_time",
                "description": "Returns the current wall-clock time (UTC-based, best effort).",
                "parameters": { "type": "object", "properties": {}, "additionalProperties": false }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "word_count",
                "description": "Counts whitespace-separated words in the given text.",
                "parameters": {
                    "type": "object",
                    "properties": { "text": { "type": "string", "description": "Text to count" } },
                    "required": ["text"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "math_add",
                "description": "Adds two integers.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "integer" },
                        "b": { "type": "integer" }
                    },
                    "required": ["a", "b"]
                }
            }
        }),
    ]
}

/// Only the three builtins — used for the tool-calling probe (fast, no network).
pub fn probe_tool_definitions() -> Vec<Value> {
    builtin_tool_definitions()
}

fn research_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "gui_web_fetch",
                "description": "Fetch a URL, convert to readable text, and answer a focused question (research, documentation, articles).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "https URL to fetch" },
                        "prompt": { "type": "string", "description": "What to extract or summarize from the page" }
                    },
                    "required": ["url", "prompt"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "gui_web_search",
                "description": "Search the web for current information; returns cited snippets (use for research, news, facts).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "allowed_domains": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional allowlist of domains"
                        }
                    },
                    "required": ["query"]
                }
            }
        }),
    ]
}

fn workspace_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "gui_read_file",
                "description": "Read a text file under the configured workspace root (read-only).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path from workspace root" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "gui_glob_search",
                "description": "Glob files under a directory within the workspace (e.g. **/*.rs).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string" },
                        "path": { "type": "string", "description": "Relative directory under workspace (default .)" }
                    },
                    "required": ["pattern"]
                }
            }
        }),
    ]
}

fn json_int(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_f64().map(|f| f as i64))
    })
}

fn format_time_rough_utc() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!(
            "UTC instant: {}s {}ns since Unix epoch",
            d.as_secs(),
            d.subsec_nanos()
        ),
        Err(_) => "time unavailable".to_string(),
    }
}

fn run_builtin_only(name: &str, arguments: &str) -> Result<String, String> {
    let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));
    match name {
        "get_current_time" => Ok(format_time_rough_utc()),
        "word_count" => {
            let text = args
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let n = text.split_whitespace().count();
            Ok(format!("Word count: {n}"))
        }
        "math_add" => {
            let a = json_int(&args, "a").ok_or_else(|| "missing or invalid integer field 'a'".to_string())?;
            let b = json_int(&args, "b").ok_or_else(|| "missing or invalid integer field 'b'".to_string())?;
            Ok(format!("{}", a + b))
        }
        _ => Err(format!("unknown builtin '{name}'")),
    }
}
