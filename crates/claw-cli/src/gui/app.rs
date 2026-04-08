use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::panic;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use eframe::epaint::{CornerRadius, Margin, Mesh, Shadow, Shape, Vec2};
use reqwest::blocking::Client;
use super::backend::GuiBackend;
use super::markdown;
use super::agent_tools::{discover_mcp_tools_blocking, GuiMcpToolEntry, GuiToolDispatcher};
use super::ollama::{
    build_api_history, expand_system_prompt, probe_ollama_tool_support,
    run_chat_with_optional_tools, run_ollama_stream_blocking,
};
use runtime::ScopedMcpServerConfig;
use super::persist::{self as persist_mod, load_config, save_config, save_session};
use super::types::{AppSettings, ChatMessage, GuiConfigFile, PromptPreset, SessionFile};

#[derive(Clone)]
struct LogEntry {
    level: String,
    message: String,
    timestamp: String,
}

pub(crate) enum ConnectionStatus {
    Connected,
    Disconnected,
    Checking,
}

enum GuiMessage {
    Response(String),
    Error(String),
    Models(Vec<String>),
    Status(ConnectionStatus),
    Log(String, String),
    ToolProbe {
        supports: bool,
        summary: String,
        raw: String,
    },
    StreamDelta(String),
    StreamDone {
        cancelled: bool,
        error: Option<String>,
    },
    McpRefreshDone(
        Result<
            (
                Vec<GuiMcpToolEntry>,
                Arc<BTreeMap<String, ScopedMcpServerConfig>>,
                String,
            ),
            String,
        >,
    ),
}

#[derive(Clone, Copy)]
struct ClawTheme {
    chat_bg_top: egui::Color32,
    chat_bg_bottom: egui::Color32,
    panel: egui::Color32,
    elevated: egui::Color32,
    input: egui::Color32,
    accent: egui::Color32,
    accent_soft: egui::Color32,
    user_bubble: egui::Color32,
    assistant_bubble: egui::Color32,
    user_message_text: egui::Color32,
    border: egui::Color32,
    success: egui::Color32,
    error: egui::Color32,
    warn: egui::Color32,
    log_bg: egui::Color32,
    text: egui::Color32,
    text_muted: egui::Color32,
    text_dim: egui::Color32,
    md_code_bg: egui::Color32,
    md_code_stroke: egui::Color32,
    md_heading: egui::Color32,
    md_body: egui::Color32,
    bubble_shadow: Shadow,
    logo_shadow: Shadow,
}

fn claw_theme(dark: bool) -> ClawTheme {
    if dark {
        // OLED-style black / zinc — no blue cast; accent is neutral light for contrast.
        ClawTheme {
            chat_bg_top: egui::Color32::from_rgb(14, 14, 14),
            chat_bg_bottom: egui::Color32::from_rgb(4, 4, 4),
            panel: egui::Color32::from_rgb(10, 10, 10),
            elevated: egui::Color32::from_rgb(26, 26, 28),
            input: egui::Color32::from_rgb(6, 6, 8),
            accent: egui::Color32::from_rgb(235, 235, 240),
            accent_soft: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 28),
            user_bubble: egui::Color32::from_rgb(28, 28, 32),
            assistant_bubble: egui::Color32::from_rgb(16, 16, 18),
            user_message_text: egui::Color32::from_rgb(248, 248, 250),
            border: egui::Color32::from_rgb(56, 56, 62),
            success: egui::Color32::from_rgb(130, 220, 150),
            error: egui::Color32::from_rgb(255, 110, 110),
            warn: egui::Color32::from_rgb(240, 190, 90),
            log_bg: egui::Color32::from_rgb(6, 6, 6),
            text: egui::Color32::from_rgb(240, 240, 245),
            text_muted: egui::Color32::from_rgb(170, 170, 178),
            text_dim: egui::Color32::from_rgb(115, 115, 125),
            md_code_bg: egui::Color32::from_rgb(8, 8, 10),
            md_code_stroke: egui::Color32::from_rgb(48, 48, 54),
            md_heading: egui::Color32::from_rgb(250, 250, 252),
            md_body: egui::Color32::from_rgb(225, 225, 230),
            bubble_shadow: Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: egui::Color32::from_black_alpha(200),
            },
            logo_shadow: Shadow {
                offset: [0, 6],
                blur: 22,
                spread: 0,
                color: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 22),
            },
        }
    } else {
        ClawTheme {
            chat_bg_top: egui::Color32::from_rgb(255, 254, 252),
            chat_bg_bottom: egui::Color32::from_rgb(241, 246, 250),
            panel: egui::Color32::from_rgb(255, 255, 255),
            elevated: egui::Color32::from_rgb(236, 242, 247),
            input: egui::Color32::from_rgb(255, 255, 255),
            accent: egui::Color32::from_rgb(0, 122, 112),
            accent_soft: egui::Color32::from_rgb(220, 244, 240),
            user_bubble: egui::Color32::from_rgb(0, 106, 98),
            assistant_bubble: egui::Color32::from_rgb(232, 238, 244),
            user_message_text: egui::Color32::from_rgb(255, 255, 255),
            border: egui::Color32::from_rgb(198, 208, 220),
            success: egui::Color32::from_rgb(22, 140, 72),
            error: egui::Color32::from_rgb(200, 48, 48),
            warn: egui::Color32::from_rgb(180, 100, 20),
            log_bg: egui::Color32::from_rgb(250, 252, 254),
            text: egui::Color32::from_rgb(20, 32, 48),
            text_muted: egui::Color32::from_rgb(70, 88, 110),
            text_dim: egui::Color32::from_rgb(120, 138, 158),
            md_code_bg: egui::Color32::from_rgb(246, 248, 252),
            md_code_stroke: egui::Color32::from_rgb(210, 218, 230),
            md_heading: egui::Color32::from_rgb(0, 110, 100),
            md_body: egui::Color32::from_rgb(30, 42, 58),
            bubble_shadow: Shadow {
                offset: [0, 4],
                blur: 18,
                spread: 0,
                color: egui::Color32::from_rgba_unmultiplied(40, 70, 90, 22),
            },
            logo_shadow: Shadow {
                offset: [0, 6],
                blur: 20,
                spread: 0,
                color: egui::Color32::from_rgba_unmultiplied(0, 122, 112, 30),
            },
        }
    }
}

/// Vertical gradient mesh behind the chat scroll area.
fn paint_chat_canvas_gradient(painter: &egui::Painter, rect: egui::Rect, top: egui::Color32, bottom: egui::Color32) {
    if rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    let mut mesh = Mesh::default();
    let i = mesh.vertices.len() as u32;
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
    painter.add(Shape::mesh(mesh));
}

fn settings_section(
    ui: &mut egui::Ui,
    title: &str,
    text: egui::Color32,
    band: egui::Color32,
    stripe: egui::Color32,
) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        egui::Frame::new()
            .fill(stripe)
            .corner_radius(3)
            .inner_margin(Margin::ZERO)
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(4.0, 28.0));
            });
        ui.add_space(10.0);
        egui::Frame::new()
            .fill(band)
            .corner_radius(10)
            .inner_margin(Margin::symmetric(12, 8))
            .stroke(egui::Stroke::new(
                1.0,
                stripe.linear_multiply(0.4),
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(title)
                        .font(egui::FontId::proportional(14.0))
                        .strong()
                        .color(text),
                );
            });
    });
    ui.add_space(8.0);
}

/// First-launch assistant content — rendered as a card instead of markdown.
const GUI_WELCOME_MARKER: &str = "__claw_gui_welcome__";

fn settings_field_caption(ui: &mut egui::Ui, t: &ClawTheme, text: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(text)
            .size(13.0)
            .strong()
            .color(t.text),
    );
}

fn render_welcome_card(ui: &mut egui::Ui, t: &ClawTheme, base: f32) {
    let tile = |ui: &mut egui::Ui, icon: &str, title: &str, body: &str| {
        egui::Frame::new()
            .fill(t.input)
            .corner_radius(12)
            .stroke(egui::Stroke::new(1.0, t.border.linear_multiply(0.65)))
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("{icon}  {title}"))
                        .size((base - 1.0).max(12.0))
                        .strong()
                        .color(t.accent),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(egui::RichText::new(body).size(12.0).color(t.text_muted)).wrap(),
                );
            });
        ui.add_space(10.0);
    };

    egui::Frame::new()
        .fill(t.assistant_bubble)
        .corner_radius(22)
        .stroke(egui::Stroke::new(1.0, t.accent.linear_multiply(0.45)))
        .inner_margin(Margin::symmetric(28, 24))
        .shadow(t.bubble_shadow)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Welcome")
                    .font(egui::FontId::proportional(base + 7.0))
                    .strong()
                    .color(t.text),
            );
            ui.add_space(8.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Local Ollama chat with streaming, built-in tools, optional MCP, research, and workspace file access.",
                    )
                    .size(13.5)
                    .color(t.text_muted),
                )
                .wrap(),
            );
            ui.add_space(18.0);
            ui.columns(2, |cols| {
                tile(
                    &mut cols[0],
                    "💬",
                    "Chat",
                    "Responses stream live when “Stream responses” is on and tools are off in Settings.",
                );
                tile(
                    &mut cols[1],
                    "🔧",
                    "Tools",
                    "Enable in Settings; Refresh MCP loads servers from your Claw config.",
                );
                tile(
                    &mut cols[0],
                    "🌐",
                    "Research",
                    "Optional WebFetch/WebSearch when enabled (uses the network).",
                );
                tile(
                    &mut cols[1],
                    "📁",
                    "Workspace",
                    "Read-only file + glob search under the folder you choose.",
                );
            });
            egui::Frame::new()
                .fill(t.elevated)
                .corner_radius(10)
                .inner_margin(Margin::symmetric(14, 11))
                .stroke(egui::Stroke::new(1.0, t.border.linear_multiply(0.75)))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(
                                "Toolbar: Refresh loads models · Settings configures endpoint, model, streaming, tools, and MCP.",
                            )
                            .size(12.5)
                            .color(t.text_muted),
                        )
                        .wrap(),
                    );
                });
        });
}

fn toolbar_standard_btn(ui: &mut egui::Ui, label: &str, t: &ClawTheme, w: f32, h: f32) -> egui::Response {
    ui.add_sized(
        Vec2::new(w, h),
        egui::Button::new(egui::RichText::new(label).size(12.5).color(t.text))
            .fill(t.input)
            .stroke(egui::Stroke::new(1.0, t.border.linear_multiply(0.88)))
            .corner_radius(8.0),
    )
}

fn toolbar_settings_btn(ui: &mut egui::Ui, panel_open: bool, t: &ClawTheme, w: f32, h: f32) -> egui::Response {
    let (label, fill, stroke) = if panel_open {
        (
            "Hide panel",
            t.accent_soft,
            egui::Stroke::new(1.5, t.accent),
        )
    } else {
        (
            "Settings",
            t.input,
            egui::Stroke::new(1.0, t.border.linear_multiply(0.88)),
        )
    };
    ui.add_sized(
        Vec2::new(w, h),
        egui::Button::new(
            egui::RichText::new(label)
                .size(12.5)
                .color(t.text)
                .strong(),
        )
        .fill(fill)
        .stroke(stroke)
        .corner_radius(8.0),
    )
}

fn toolbar_logs_btn(ui: &mut egui::Ui, active: bool, t: &ClawTheme, w: f32, h: f32) -> egui::Response {
    let (fill, stroke) = if active {
        (t.accent_soft, egui::Stroke::new(1.5, t.accent))
    } else {
        (t.input, egui::Stroke::new(1.0, t.border.linear_multiply(0.88)))
    };
    ui.add_sized(
        Vec2::new(w, h),
        egui::Button::new(egui::RichText::new("Logs").size(12.5).color(t.text))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(8.0),
    )
}

pub struct ClawGui {
    messages: Vec<ChatMessage>,
    input: String,
    settings: AppSettings,
    prompt_presets: Vec<PromptPreset>,
    /// When true, `session_system_prompt` overrides global for this chat only.
    session_only_prompt: bool,
    session_system_prompt: String,
    show_settings: bool,
    show_logs: bool,
    is_loading: bool,
    is_probing_tools: bool,
    available_models: Vec<String>,
    selected_model_index: usize,
    connection_status: ConnectionStatus,
    total_tokens: usize,
    total_requests: usize,
    logs: VecDeque<LogEntry>,
    request_duration_ms: u64,
    session_start: Instant,
    error_message: String,
    show_error: bool,
    /// Brief "Copied" feedback: message id + end time.
    copy_flash: Option<(usize, Instant)>,
    tool_probe_summary: String,
    tool_probe_supports: Option<bool>,
    tool_probe_raw: String,
    mcp_servers: Option<Arc<BTreeMap<String, ScopedMcpServerConfig>>>,
    mcp_tool_entries: Vec<GuiMcpToolEntry>,
    mcp_status_line: String,
    is_refreshing_mcp: bool,
    tx: Option<mpsc::Sender<GuiMessage>>,
    rx: Option<mpsc::Receiver<GuiMessage>>,
    stream_cancel: Option<Arc<AtomicBool>>,
    new_preset_name: String,
    import_prompt_path: String,
    session_open_path: String,
    current_session_path: Option<PathBuf>,
    /// Phase 2 stub — shown in settings.
    backend: GuiBackend,
    /// Skip redundant `ctx.style_mut` when unchanged (reduces work each frame).
    style_dark_applied: Option<bool>,
    style_font_applied: Option<f32>,
}

impl ClawGui {
    fn new() -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let cfg = load_config().unwrap_or_default();

        let mut gui = Self {
            messages: vec![ChatMessage::assistant(GUI_WELCOME_MARKER.to_string(), 0)],
            input: String::new(),
            settings: cfg.settings,
            prompt_presets: cfg.prompt_presets,
            session_only_prompt: false,
            session_system_prompt: String::new(),
            show_settings: false,
            show_logs: false,
            is_loading: false,
            is_probing_tools: false,
            available_models: Vec::new(),
            selected_model_index: 0,
            connection_status: ConnectionStatus::Disconnected,
            total_tokens: 0,
            total_requests: 0,
            logs: VecDeque::new(),
            request_duration_ms: 0,
            session_start: Instant::now(),
            error_message: String::new(),
            show_error: false,
            copy_flash: None,
            tool_probe_summary: "Not tested yet — click “Probe tool calling”.".to_string(),
            tool_probe_supports: None,
            tool_probe_raw: String::new(),
            mcp_servers: None,
            mcp_tool_entries: Vec::new(),
            mcp_status_line: "MCP: click Refresh MCP after enabling (uses Claw config).".to_string(),
            is_refreshing_mcp: false,
            tx: Some(tx),
            rx: Some(rx),
            stream_cancel: None,
            new_preset_name: String::new(),
            import_prompt_path: String::new(),
            session_open_path: String::new(),
            current_session_path: None,
            backend: GuiBackend::Ollama,
            style_dark_applied: None,
            style_font_applied: None,
        };

        if gui.settings.font_size < 8.0 {
            gui.settings.font_size = 14.0;
        }

        gui.add_log("INFO", "Application started");
        Ok(gui)
    }

    fn persist_settings(&self) {
        let cfg = GuiConfigFile {
            settings: self.settings.clone(),
            prompt_presets: self.prompt_presets.clone(),
        };
        if let Err(e) = save_config(&cfg) {
            eprintln!("claw-gui: failed to save settings: {e}");
        }
    }

    fn next_msg_id(&self) -> usize {
        self.messages.iter().map(|m| m.id).max().map_or(0, |m| m + 1)
    }

    fn compose_system_prompt(&self) -> String {
        let raw = if self.session_only_prompt && !self.session_system_prompt.trim().is_empty() {
            self.session_system_prompt.clone()
        } else if !self.settings.system_prompt.trim().is_empty() {
            self.settings.system_prompt.clone()
        } else {
            let mut s = "You are a helpful AI assistant. Be concise and clear.".to_string();
            if self.settings.ollama.enable_tools {
                s.push_str(
                    " You may call tools when useful: get_current_time, word_count {\"text\":...}, math_add {\"a\":n,\"b\":n}.",
                );
            }
            s
        };
        expand_system_prompt(&raw)
    }

    fn push_log_line(&mut self, level: String, message: String) {
        let entry = LogEntry {
            level,
            message,
            timestamp: ChatMessage::now_time(),
        };
        self.logs.push_front(entry);
        if self.logs.len() > 100 {
            self.logs.pop_back();
        }
    }

    fn add_log(&mut self, level: &str, message: &str) {
        self.push_log_line(level.to_string(), message.to_string());
    }

    fn apply_style(&mut self, ctx: &egui::Context) {
        let dark = self.settings.dark_mode;
        let sz = self.settings.font_size.clamp(10.0, 28.0);
        let t = claw_theme(dark);

        let mut visuals = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.hyperlink_color = t.accent;
        visuals.selection.bg_fill = egui::Color32::from_rgba_unmultiplied(
            t.accent.r(),
            t.accent.g(),
            t.accent.b(),
            55,
        );
        visuals.selection.stroke = egui::Stroke::new(1.0, t.accent);
        visuals.window_corner_radius = CornerRadius::same(12);
        visuals.menu_corner_radius = CornerRadius::same(10);
        visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);
        visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
        visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
        visuals.widgets.active.corner_radius = CornerRadius::same(8);
        visuals.widgets.open.corner_radius = CornerRadius::same(8);
        visuals.popup_shadow = Shadow {
            offset: [0, 4],
            blur: 18,
            spread: 0,
            color: egui::Color32::from_black_alpha(if dark { 38 } else { 14 }),
        };
        visuals.window_shadow = Shadow {
            offset: [0, 10],
            blur: 28,
            spread: 0,
            color: egui::Color32::from_black_alpha(if dark { 48 } else { 20 }),
        };
        if dark {
            visuals.panel_fill = t.panel;
            visuals.extreme_bg_color = t.input;
            visuals.faint_bg_color = t.elevated;
        }
        ctx.set_visuals(visuals);

        let need_font_tweak = self.style_dark_applied != Some(dark) || self.style_font_applied != Some(sz);
        if !need_font_tweak {
            return;
        }
        self.style_dark_applied = Some(dark);
        self.style_font_applied = Some(sz);
        ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(10.0, 8.0);
            style.spacing.button_padding = Vec2::new(12.0, 7.0);
            style.spacing.interact_size = Vec2::new(44.0, 22.0);
            for ts in [
                egui::TextStyle::Small,
                egui::TextStyle::Body,
                egui::TextStyle::Button,
                egui::TextStyle::Heading,
                egui::TextStyle::Monospace,
            ] {
                if let Some(fid) = style.text_styles.get_mut(&ts) {
                    fid.size = match ts {
                        egui::TextStyle::Small => (sz - 2.0).max(9.0),
                        egui::TextStyle::Heading => sz + 4.0,
                        _ => sz,
                    };
                }
            }
        });
    }

    fn stop_generation(&mut self) {
        if let Some(c) = self.stream_cancel.take() {
            c.store(true, Ordering::SeqCst);
        }
    }

    fn new_chat(&mut self) {
        self.messages.clear();
        self.messages.push(ChatMessage::assistant(
            "New chat — send a message to begin.".to_string(),
            0,
        ));
        self.session_system_prompt.clear();
        self.session_only_prompt = false;
        self.current_session_path = None;
        self.add_log("INFO", "New chat");
        self.persist_settings();
    }

    fn save_current_session(&mut self) {
        let path = self
            .current_session_path
            .clone()
            .unwrap_or_else(|| {
                let name = format!(
                    "chat-{}.json",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                );
                persist_mod::sessions_dir().join(name)
            });
        let session = SessionFile {
            messages: self.messages.clone(),
            session_system_prompt: self.session_system_prompt.clone(),
        };
        match save_session(&path, &session) {
            Ok(()) => {
                self.current_session_path = Some(path.clone());
                self.add_log("INFO", &format!("Saved session {}", path.display()));
            }
            Err(e) => {
                self.error_message = format!("Save failed: {e}");
                self.show_error = true;
            }
        }
    }

    fn open_session_from_path(&mut self, path: &str) {
        let p = PathBuf::from(path.trim());
        match super::persist::load_session(&p) {
            Ok(s) => {
                self.messages = s.messages;
                self.session_system_prompt = s.session_system_prompt;
                self.session_only_prompt = !self.session_system_prompt.trim().is_empty();
                self.current_session_path = Some(p);
                self.add_log("INFO", "Session loaded");
            }
            Err(e) => {
                self.error_message = format!("Open failed: {e}");
                self.show_error = true;
            }
        }
    }

    fn import_system_prompt_file(&mut self) {
        let p = self.import_prompt_path.trim();
        if p.is_empty() {
            return;
        }
        match std::fs::read_to_string(p) {
            Ok(s) => {
                self.settings.system_prompt = s;
                self.persist_settings();
                self.add_log("INFO", "Imported system prompt from file");
            }
            Err(e) => {
                self.error_message = format!("Import failed: {e}");
                self.show_error = true;
            }
        }
    }

    fn export_system_prompt_file(&mut self, path: &str) {
        let p = path.trim();
        if p.is_empty() {
            return;
        }
        if let Err(e) = std::fs::write(p, &self.settings.system_prompt) {
            self.error_message = format!("Export failed: {e}");
            self.show_error = true;
        } else {
            self.add_log("INFO", "Exported system prompt");
        }
    }

    fn test_connection(&mut self) {
        self.connection_status = ConnectionStatus::Checking;
        self.add_log("INFO", "Testing Ollama connection...");

        let base_url = self.settings.ollama.base_url.clone();
        let tx = self.tx.clone().unwrap();

        std::thread::spawn(move || {
            let client = Client::builder().timeout(Duration::from_secs(10)).build();

            match client {
                Ok(c) => {
                    let url = format!("{}/api/tags", base_url);
                    match c.get(&url).send() {
                        Ok(resp) if resp.status().is_success() => {
                            match resp.json::<serde_json::Value>() {
                                Ok(json) => {
                                    let models: Vec<String> = json
                                        .get("models")
                                        .and_then(|m| m.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|m| {
                                                    m.get("name")
                                                        .and_then(|n| n.as_str())
                                                        .map(String::from)
                                                })
                                                .collect()
                                        })
                                        .unwrap_or_default();
                                    let status = if models.is_empty() {
                                        ConnectionStatus::Disconnected
                                    } else {
                                        ConnectionStatus::Connected
                                    };
                                    let _ = tx.send(GuiMessage::Status(status));
                                    let _ = tx.send(GuiMessage::Log(
                                        if models.is_empty() {
                                            "WARN"
                                        } else {
                                            "INFO"
                                        }
                                        .to_string(),
                                        if models.is_empty() {
                                            "No models reported (check Ollama /api/tags response)"
                                                .to_string()
                                        } else {
                                            format!("Connected ({} models)", models.len())
                                        },
                                    ));
                                    let _ = tx.send(GuiMessage::Models(models));
                                }
                                Err(e) => {
                                    let _ = tx.send(GuiMessage::Status(ConnectionStatus::Disconnected));
                                    let _ = tx.send(GuiMessage::Log(
                                        "ERROR".to_string(),
                                        format!("Parse failed - {e}"),
                                    ));
                                }
                            }
                        }
                        Ok(resp) => {
                            let _ = tx.send(GuiMessage::Status(ConnectionStatus::Disconnected));
                            let _ = tx.send(GuiMessage::Log(
                                "ERROR".to_string(),
                                format!("HTTP {}", resp.status()),
                            ));
                        }
                        Err(e) => {
                            let _ = tx.send(GuiMessage::Status(ConnectionStatus::Disconnected));
                            let _ = tx.send(GuiMessage::Log(
                                "ERROR".to_string(),
                                format!("Connection failed - {e}"),
                            ));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(GuiMessage::Status(ConnectionStatus::Disconnected));
                    let _ = tx.send(GuiMessage::Log(
                        "ERROR".to_string(),
                        format!("Client failed - {e}"),
                    ));
                }
            }
        });
    }

    fn probe_tool_calling(&mut self) {
        if self.available_models.is_empty() {
            self.error_message = "Load models first (Test connection), then probe.".to_string();
            self.show_error = true;
            self.add_log("WARN", "Probe skipped: no models list");
            return;
        }
        self.is_probing_tools = true;
        self.add_log("INFO", "Probing tool calling for current model...");
        let base_url = self.settings.ollama.base_url.clone();
        let model = self.settings.ollama.model.clone();
        let temperature = self.settings.ollama.temperature;
        let max_tokens = self.settings.ollama.max_tokens;
        let tx = self.tx.clone().unwrap();

        std::thread::spawn(move || {
            let client = match Client::builder().timeout(Duration::from_secs(120)).build() {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(GuiMessage::ToolProbe {
                        supports: false,
                        summary: format!("Client error: {e}"),
                        raw: String::new(),
                    });
                    return;
                }
            };
            let (supports, summary, raw) = probe_ollama_tool_support(
                &client,
                &base_url,
                &model,
                temperature,
                max_tokens,
            );
            let _ = tx.send(GuiMessage::ToolProbe {
                supports,
                summary,
                raw,
            });
        });
    }

    fn workspace_path_for_tools(&self) -> PathBuf {
        let r = self.settings.workspace_root.trim();
        if r.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            PathBuf::from(r)
        }
    }

    fn build_tool_dispatcher(&self) -> GuiToolDispatcher {
        GuiToolDispatcher {
            enable_research_tools: self.settings.enable_research_tools,
            enable_workspace_tools: self.settings.enable_workspace_tools,
            enable_mcp_tools: self.settings.enable_mcp_tools,
            workspace_root: self.workspace_path_for_tools(),
            mcp_servers: self.mcp_servers.clone(),
            mcp_tools: Arc::new(self.mcp_tool_entries.clone()),
        }
    }

    fn refresh_mcp_catalog(&mut self) {
        if self.is_refreshing_mcp {
            return;
        }
        self.is_refreshing_mcp = true;
        self.add_log("INFO", "Discovering MCP tools from Claw config…");
        let tx = self.tx.clone().unwrap();
        std::thread::spawn(move || {
            let r = discover_mcp_tools_blocking();
            let _ = tx.send(GuiMessage::McpRefreshDone(r));
        });
    }

    fn send_message(&mut self) {
        if self.input.trim().is_empty() {
            return;
        }
        if self.available_models.is_empty() {
            self.error_message = "No models available. Click 'Test Connection' first.".to_string();
            self.show_error = true;
            self.add_log("ERROR", "No models available");
            return;
        }

        let user_input = std::mem::take(&mut self.input);
        let msg_id = self.next_msg_id();
        self.messages
            .push(ChatMessage::user(user_input.clone(), msg_id));
        self.messages
            .push(ChatMessage::assistant(String::new(), msg_id + 1));
        self.is_loading = true;
        self.stream_cancel = None;

        self.add_log(
            "SEND",
            &format!("Message sent ({} chars)", user_input.len()),
        );

        let base_url = self.settings.ollama.base_url.clone();
        let model = self.settings.ollama.model.clone();
        let system = self.compose_system_prompt();
        let temperature = self.settings.ollama.temperature;
        let max_tokens = self.settings.ollama.max_tokens;
        let enable_tools = self.settings.ollama.enable_tools;
        let stream_on = self.settings.stream_responses && !enable_tools;
        let tool_dispatcher = self.build_tool_dispatcher();

        let api_history = build_api_history(
            &self.messages,
            true,
            self.settings.context_max_messages,
            self.settings.context_max_chars,
        );

        let tx = self.tx.clone().unwrap();

        if stream_on {
            let cancel = Arc::new(AtomicBool::new(false));
            self.stream_cancel = Some(cancel.clone());
            std::thread::spawn(move || {
                let start = Instant::now();
                let tx2 = tx.clone();
                let r = run_ollama_stream_blocking(
                    &base_url.trim_end_matches('/'),
                    &model,
                    &system,
                    &api_history,
                    temperature,
                    max_tokens,
                    cancel.clone(),
                    move |d| {
                        let _ = tx2.send(GuiMessage::StreamDelta(d));
                    },
                );
                let cancelled = cancel.load(Ordering::SeqCst);
                match r {
                    Ok(()) => {
                        let _ = tx.send(GuiMessage::StreamDone {
                            cancelled,
                            error: None,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(GuiMessage::StreamDone {
                            cancelled,
                            error: Some(e),
                        });
                    }
                }
                let elapsed = start.elapsed().as_millis();
                let _ = tx.send(GuiMessage::Log(
                    "INFO".to_string(),
                    format!("Stream finished in {elapsed}ms"),
                ));
            });
        } else {
            std::thread::spawn(move || {
                let start = Instant::now();
                let client = match Client::builder().timeout(Duration::from_secs(180)).build() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(GuiMessage::Error(format!("Client error: {e}")));
                        return;
                    }
                };

                let base = base_url.trim_end_matches('/');
                match run_chat_with_optional_tools(
                    &client,
                    base,
                    &model,
                    &system,
                    &api_history,
                    temperature,
                    max_tokens,
                    enable_tools,
                    &tool_dispatcher,
                ) {
                    Ok(text) => {
                        let n = text.len();
                        let _ = tx.send(GuiMessage::Response(text));
                        let _ = tx.send(GuiMessage::Log(
                            "INFO".to_string(),
                            format!("Response received ({n} chars)"),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.send(GuiMessage::Error(e));
                    }
                }

                let elapsed = start.elapsed().as_millis();
                let _ = tx.send(GuiMessage::Log(
                    "INFO".to_string(),
                    format!("Request completed in {elapsed}ms"),
                ));
            });
        }
    }

    fn check_response(&mut self, ctx: &egui::Context) {
        while let Some(rx) = &self.rx {
            if let Ok(msg) = rx.try_recv() {
                match msg {
                    GuiMessage::Response(content) => {
                        if let Some(last) = self.messages.last_mut() {
                            last.content = content.clone();
                        }
                        self.is_loading = false;
                        self.stream_cancel = None;
                        self.connection_status = ConnectionStatus::Connected;
                        self.request_duration_ms =
                            Instant::now().duration_since(self.session_start).as_millis() as u64;
                        self.total_tokens += content.split_whitespace().count() / 4;
                        self.total_requests += 1;
                        self.add_log("RECV", &format!("Response ({} chars)", content.len()));
                    }
                    GuiMessage::StreamDelta(s) => {
                        if let Some(last) = self.messages.last_mut() {
                            if last.role == "assistant" {
                                last.content.push_str(&s);
                            }
                        }
                        ctx.request_repaint();
                    }
                    GuiMessage::StreamDone { cancelled, error } => {
                        self.is_loading = false;
                        self.stream_cancel = None;
                        let had_error = error.is_some();
                        if let Some(e) = error {
                            if let Some(last) = self.messages.last_mut() {
                                if last.role == "assistant" && last.content.is_empty() {
                                    last.content = format!("❌ Error: {e}");
                                } else {
                                    last.content.push_str(&format!("\n\n❌ Error: {e}"));
                                }
                            }
                            self.add_log("ERROR", &e);
                            self.error_message = e;
                            self.show_error = true;
                        } else if cancelled {
                            if let Some(last) = self.messages.last_mut() {
                                if last.role == "assistant" {
                                    last.content.push_str("\n\n_[Stopped]_");
                                }
                            }
                            self.add_log("INFO", "Generation stopped by user");
                        }
                        if !had_error {
                            self.connection_status = ConnectionStatus::Connected;
                            if let Some(last) = self.messages.last() {
                                if last.role == "assistant" {
                                    self.total_tokens +=
                                        last.content.split_whitespace().count() / 4;
                                    self.total_requests += 1;
                                }
                            }
                        }
                    }
                    GuiMessage::Error(error) => {
                        if let Some(last) = self.messages.last_mut() {
                            last.content = format!("❌ Error: {error}");
                        }
                        self.is_loading = false;
                        self.stream_cancel = None;
                        self.connection_status = ConnectionStatus::Disconnected;
                        self.total_requests += 1;
                        self.add_log("ERROR", &error);
                        self.error_message = error;
                        self.show_error = true;
                    }
                    GuiMessage::Models(models) => {
                        self.available_models = models;
                        if let Some(idx) = self
                            .available_models
                            .iter()
                            .position(|m| m == &self.settings.ollama.model)
                        {
                            self.selected_model_index = idx;
                        }
                    }
                    GuiMessage::Status(status) => {
                        self.connection_status = status;
                    }
                    GuiMessage::Log(level, message) => {
                        self.push_log_line(level, message);
                    }
                    GuiMessage::ToolProbe {
                        supports,
                        summary,
                        raw,
                    } => {
                        self.is_probing_tools = false;
                        self.tool_probe_supports = Some(supports);
                        self.tool_probe_summary = summary.clone();
                        self.tool_probe_raw = raw;
                        self.add_log(if supports { "INFO" } else { "WARN" }, &summary);
                    }
                    GuiMessage::McpRefreshDone(result) => {
                        self.is_refreshing_mcp = false;
                        match result {
                            Ok((entries, servers, note)) => {
                                self.mcp_tool_entries = entries;
                                self.mcp_servers = Some(servers);
                                self.mcp_status_line = note.clone();
                                self.add_log("INFO", &note);
                            }
                            Err(e) => {
                                self.mcp_tool_entries.clear();
                                self.mcp_servers = None;
                                self.mcp_status_line = format!("MCP refresh failed: {e}");
                                self.add_log("ERROR", &e);
                            }
                        }
                    }
                }
            } else {
                break;
            }
        }
    }

    fn clear_chat(&mut self) {
        self.messages.clear();
        self.messages.push(ChatMessage::assistant(
            "Chat cleared. Start a new conversation!".to_string(),
            0,
        ));
        self.add_log("INFO", "Chat cleared");
    }

    fn clear_logs(&mut self) {
        self.logs.clear();
        self.add_log("INFO", "Logs cleared");
    }

    fn copy_message(&mut self, ctx: &egui::Context, id: usize) {
        if let Some(msg) = self.messages.iter().find(|m| m.id == id) {
            self.copy_flash = Some((id, Instant::now() + Duration::from_millis(1400)));
            ctx.copy_text(msg.content.clone());
            ctx.request_repaint_after(Duration::from_millis(1500));
            self.add_log("INFO", "Copied to clipboard");
        }
    }

    pub(crate) fn draw_ui(&mut self, ctx: &egui::Context) {
        self.check_response(ctx);
        if self
            .copy_flash
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() < *until)
        {
            ctx.request_repaint();
        }
        if self.is_loading || self.is_probing_tools || self.is_refreshing_mcp {
            ctx.request_repaint();
        }
        // Streaming: repaint every frame so tokens + caret blink stay smooth.
        if self.is_loading && self.stream_cancel.is_some() {
            ctx.request_repaint();
        }

        self.apply_style(ctx);

        let dark = self.settings.dark_mode;
        let t = claw_theme(dark);

        let mut settings_changed = false;

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            // One shared height so toggles and buttons align on the same baseline.
            let toolbar_h = 30.0_f32;

            egui::Frame::default()
                .fill(t.panel)
                .stroke(egui::Stroke::new(1.0, t.border))
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;

                        // Row 1: brand (fixed) — status chips wrap on narrow windows.
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            egui::Frame::new()
                                .fill(t.accent_soft)
                                .corner_radius(18)
                                .shadow(t.logo_shadow)
                                .stroke(egui::Stroke::new(
                                    1.5,
                                    egui::Color32::from_rgba_unmultiplied(
                                        t.accent.r(),
                                        t.accent.g(),
                                        t.accent.b(),
                                        130,
                                    ),
                                ))
                                .inner_margin(Margin::same(9))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("🦞")
                                            .font(egui::FontId::proportional(26.0)),
                                    );
                                });
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 5.0;
                                    ui.label(
                                        egui::RichText::new("Claw")
                                            .font(egui::FontId::proportional(19.0))
                                            .strong()
                                            .color(t.text),
                                    );
                                    ui.label(
                                        egui::RichText::new("·")
                                            .font(egui::FontId::proportional(22.0))
                                            .color(t.accent),
                                    );
                                    ui.label(
                                        egui::RichText::new("Code")
                                            .font(egui::FontId::proportional(19.0))
                                            .strong()
                                            .color(t.text),
                                    );
                                });
                                ui.label(
                                    egui::RichText::new("LOCAL FIRST  ·  OLLAMA")
                                        .font(egui::FontId::monospace(10.5))
                                        .color(t.text_dim),
                                );
                            });
                        });

                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                            let (pill_bg, pill_fg, pill_txt) = match self.connection_status {
                                ConnectionStatus::Connected => (
                                    egui::Color32::from_rgba_unmultiplied(
                                        t.success.r(),
                                        t.success.g(),
                                        t.success.b(),
                                        45,
                                    ),
                                    t.success,
                                    "Connected",
                                ),
                                ConnectionStatus::Disconnected => (
                                    egui::Color32::from_rgba_unmultiplied(
                                        t.error.r(),
                                        t.error.g(),
                                        t.error.b(),
                                        45,
                                    ),
                                    t.error,
                                    "Offline",
                                ),
                                ConnectionStatus::Checking => (
                                    egui::Color32::from_rgba_unmultiplied(
                                        t.warn.r(),
                                        t.warn.g(),
                                        t.warn.b(),
                                        45,
                                    ),
                                    t.warn,
                                    "Checking…",
                                ),
                            };
                            ui.label(
                                egui::RichText::new(format!(" {pill_txt} "))
                                    .background_color(pill_bg)
                                    .color(pill_fg)
                                    .size(12.5),
                            );

                            let stream_note = if self.settings.ollama.enable_tools {
                                "Batch · tools"
                            } else if self.settings.stream_responses {
                                "Stream"
                            } else {
                                "Batch"
                            };
                            ui.label(
                                egui::RichText::new(format!(" {stream_note} "))
                                    .color(t.text_muted)
                                    .size(10.5)
                                    .background_color(t.elevated),
                            );

                            let (tc, tt) = if self.settings.ollama.enable_tools {
                                (t.accent, "Tools on")
                            } else {
                                (t.text_dim, "Tools off")
                            };
                            ui.label(
                                egui::RichText::new(format!(" {tt} ")).color(tc).size(11.5),
                            );
                            if let Some(s) = self.tool_probe_supports {
                                let (c, l) = if s {
                                    (t.success, "Tools OK")
                                } else {
                                    (t.warn, "Tools ?")
                                };
                                ui.label(
                                    egui::RichText::new(format!(" {l} "))
                                        .color(c)
                                        .size(11.0)
                                        .background_color(t.elevated),
                                );
                            }
                            if self.settings.enable_mcp_tools && !self.mcp_tool_entries.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        " MCP {} ",
                                        self.mcp_tool_entries.len()
                                    ))
                                    .color(t.accent)
                                    .size(11.0)
                                    .background_color(t.elevated),
                                )
                                .on_hover_text(&self.mcp_status_line);
                            }

                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} msgs · {} tok",
                                    self.total_requests, self.total_tokens
                                ))
                                .color(t.text_dim)
                                .size(11.0),
                            );

                            let session_dur = self.session_start.elapsed().as_secs();
                            let dur = if session_dur > 3600 {
                                format!(
                                    "{}h {}m",
                                    session_dur / 3600,
                                    (session_dur % 3600) / 60
                                )
                            } else if session_dur >= 60 {
                                format!("{}m", session_dur / 60)
                            } else {
                                format!("{session_dur}s")
                            };
                            ui.label(
                                egui::RichText::new(dur)
                                    .color(t.text_dim.gamma_multiply(0.85))
                                    .size(11.0),
                            );
                        });

                        // Row 2: toolbar — consistent outlined buttons; Settings / Logs use accent when active.
                        let row2_h = toolbar_h + 8.0;
                        let bh = 32.0_f32;
                        ui.allocate_ui_with_layout(
                            Vec2::new(ui.available_width(), row2_h),
                            egui::Layout::right_to_left(egui::Align::Center)
                                .with_cross_align(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                if toolbar_settings_btn(ui, self.show_settings, &t, 102.0, bh)
                                    .on_hover_text("Toggle settings sidebar")
                                    .clicked()
                                {
                                    self.show_settings = !self.show_settings;
                                }
                                if toolbar_standard_btn(ui, "Refresh", &t, 88.0, bh)
                                    .on_hover_text("Test Ollama connection & load models")
                                    .clicked()
                                {
                                    self.test_connection();
                                }
                                let mcp_lbl = if self.is_refreshing_mcp {
                                    "MCP…"
                                } else {
                                    "Refresh MCP"
                                };
                                let mcp_ir = ui.add_enabled_ui(!self.is_refreshing_mcp, |ui| {
                                    toolbar_standard_btn(ui, mcp_lbl, &t, 112.0, bh).on_hover_text(
                                        "Discover stdio MCP tools from your Claw config (same as CLI)",
                                    )
                                });
                                if mcp_ir.inner.clicked() {
                                    self.refresh_mcp_catalog();
                                }
                                if self.is_refreshing_mcp {
                                    ui.spinner();
                                }
                                if toolbar_standard_btn(ui, "Clear chat", &t, 96.0, bh)
                                    .on_hover_text("Clear messages")
                                    .clicked()
                                {
                                    self.clear_chat();
                                }
                                if toolbar_standard_btn(ui, "New chat", &t, 92.0, bh)
                                    .on_hover_text("Start a fresh thread")
                                    .clicked()
                                {
                                    self.new_chat();
                                }
                                if toolbar_logs_btn(ui, self.show_logs, &t, 68.0, bh)
                                    .on_hover_text("Toggle live log panel")
                                    .clicked()
                                {
                                    self.show_logs = !self.show_logs;
                                }
                            },
                        );
                    });
                });
        });

        if self.show_logs {
            egui::SidePanel::left("logs")
                .min_width(260.0)
                .show(ctx, |ui| {
                    egui::Frame::default()
                        .fill(t.log_bg)
                        .stroke(egui::Stroke::new(1.0, t.border))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(
                                    egui::RichText::new("Live logs")
                                        .font(egui::FontId::proportional(14.0))
                                        .strong()
                                        .color(t.text),
                                ));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .button(
                                                egui::RichText::new("✕")
                                                    .color(t.text_dim)
                                                    .small(),
                                            )
                                            .clicked()
                                        {
                                            self.clear_logs();
                                        }
                                    },
                                );
                            });
                            ui.separator();
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                for log in self.logs.iter() {
                                    let col = match log.level.as_str() {
                                        "ERROR" => t.error,
                                        "WARN" => t.warn,
                                        "SEND" | "RECV" => t.accent,
                                        _ => egui::Color32::from_gray(140),
                                    };
                                    ui.add_space(3.0);
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Label::new(
                                            egui::RichText::new(&log.timestamp)
                                                .color(egui::Color32::from_gray(65))
                                                .size(10.0),
                                        ));
                                        ui.add(egui::Label::new(
                                            egui::RichText::new(&log.level)
                                                .color(col)
                                                .size(10.0),
                                        ));
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&log.message)
                                                .color(egui::Color32::from_gray(170))
                                                .size(11.0),
                                        )
                                        .wrap(),
                                    );
                                }
                                if self.logs.is_empty() {
                                    ui.add(egui::Label::new(
                                        egui::RichText::new("No activity yet...")
                                            .color(egui::Color32::from_gray(80)),
                                    ));
                                }
                            });
                        });
                });
        }

        if self.show_error && !self.error_message.is_empty() {
            egui::TopBottomPanel::bottom("error").show(ctx, |ui| {
                let err_fill = egui::Color32::from_rgba_unmultiplied(
                    t.error.r(),
                    t.error.g(),
                    t.error.b(),
                    if dark { 35 } else { 28 },
                );
                egui::Frame::default()
                    .fill(err_fill)
                    .stroke(egui::Stroke::new(1.0, t.error))
                    .inner_margin(Margin::symmetric(12, 10))
                    .corner_radius(8)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(
                                egui::RichText::new("⚠").color(t.error).size(18.0),
                            ));
                            ui.add(egui::Label::new(
                                egui::RichText::new(&self.error_message)
                                    .color(t.text)
                                    .size(13.0),
                            ));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button(
                                            egui::RichText::new("✕")
                                                .color(egui::Color32::from_gray(150)),
                                        )
                                        .clicked()
                                    {
                                        self.show_error = false;
                                        self.error_message.clear();
                                    }
                                },
                            );
                        });
                    });
            });
        }

        if self.show_settings {
            egui::SidePanel::right("settings")
                .min_width(368.0)
                .show(ctx, |ui| {
                    egui::Frame::default()
                        .fill(t.panel)
                        .stroke(egui::Stroke::new(1.0, t.border))
                        .inner_margin(20)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Settings")
                                            .font(egui::FontId::proportional(22.0))
                                            .strong()
                                            .color(t.text),
                                    );
                                    ui.label(
                                        egui::RichText::new("  ◆")
                                            .size(11.0)
                                            .color(t.accent),
                                    );
                                });
                                ui.label(
                                    egui::RichText::new(format!("Backend · {}", self.backend.label()))
                                        .color(t.text_muted)
                                        .size(13.0),
                                );
                                ui.add_space(14.0);
                                ui.separator();
                                ui.add_space(10.0);

                                settings_section(ui, "Appearance", t.text, t.elevated, t.accent);
                                egui::Frame::new()
                                    .fill(t.elevated)
                                    .corner_radius(12)
                                    .stroke(egui::Stroke::new(1.0, t.border.linear_multiply(0.55)))
                                    .inner_margin(Margin::symmetric(14, 12))
                                    .show(ui, |ui| {
                                        if ui
                                            .checkbox(&mut self.settings.dark_mode, "Dark mode")
                                            .changed()
                                        {
                                            settings_changed = true;
                                        }
                                        if ui
                                            .add(egui::Slider::new(
                                                &mut self.settings.font_size,
                                                10.0..=22.0,
                                            )
                                            .text("Font size"))
                                            .changed()
                                        {
                                            settings_changed = true;
                                        }
                                    });

                                ui.add_space(18.0);
                                ui.separator();
                                ui.add_space(10.0);

                                settings_section(ui, "Ollama", t.text, t.elevated, t.accent);
                                egui::Frame::new()
                                    .fill(t.elevated)
                                    .corner_radius(12)
                                    .stroke(egui::Stroke::new(1.0, t.border.linear_multiply(0.55)))
                                    .inner_margin(Margin::symmetric(14, 12))
                                    .show(ui, |ui| {
                                settings_field_caption(ui, &t, "Endpoint URL");
                                if ui
                                    .text_edit_singleline(&mut self.settings.ollama.base_url)
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                ui.add_space(8.0);
                                settings_field_caption(ui, &t, "Model");
                                egui::Frame::default()
                                    .fill(t.input)
                                    .corner_radius(8)
                                    .stroke(egui::Stroke::new(1.0, t.border.gamma_multiply(0.65)))
                                    .show(ui, |ui| {
                                        egui::ComboBox::from_id_salt("model")
                                            .selected_text(
                                                egui::RichText::new(&self.settings.ollama.model)
                                                    .color(t.text),
                                            )
                                            .show_ui(ui, |ui| {
                                                for (i, m) in
                                                    self.available_models.iter().enumerate()
                                                {
                                                    if ui
                                                        .selectable_label(
                                                            i == self.selected_model_index,
                                                            m,
                                                        )
                                                        .clicked()
                                                    {
                                                        self.selected_model_index = i;
                                                        self.settings.ollama.model = m.clone();
                                                        settings_changed = true;
                                                    }
                                                }
                                            });
                                    });

                                ui.add_space(12.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "🌡️ Temperature: {:.1}",
                                        self.settings.ollama.temperature
                                    ))
                                    .color(t.text_muted)
                                    .size(13.0),
                                );
                                if ui
                                    .add(egui::Slider::new(
                                        &mut self.settings.ollama.temperature,
                                        0.0..=2.0,
                                    )
                                    .text(""))
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(format!(
                                        "📝 Max Tokens: {}",
                                        self.settings.ollama.max_tokens
                                    ))
                                    .color(t.text_muted)
                                    .size(13.0),
                                );
                                if ui
                                    .add(egui::Slider::new(
                                        &mut self.settings.ollama.max_tokens,
                                        256..=8192,
                                    )
                                    .text(""))
                                    .changed()
                                {
                                    settings_changed = true;
                                }

                                ui.add_space(10.0);
                                if ui
                                    .checkbox(
                                        &mut self.settings.stream_responses,
                                        "Stream responses (disabled when tools are on)",
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                if ui
                                    .add(
                                        egui::DragValue::new(
                                            &mut self.settings.context_max_messages,
                                        )
                                        .speed(1)
                                        .prefix("Context max pairs: "),
                                    )
                                    .on_hover_text("0 = unlimited user+assistant pairs")
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut self.settings.context_max_chars)
                                            .speed(256)
                                            .prefix("Context max chars: "),
                                    )
                                    .on_hover_text("0 = unlimited total chars in history")
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                    });

                                ui.add_space(18.0);
                                ui.separator();
                                ui.add_space(10.0);
                                settings_section(ui, "Tools (OpenAI-style)", t.text, t.elevated, t.accent);
                                ui.add_space(4.0);
                                if ui
                                    .checkbox(
                                        &mut self.settings.ollama.enable_tools,
                                        "Enable tools in chat",
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            "Built-ins plus optional research, workspace, and MCP (see below).",
                                        )
                                        .color(egui::Color32::from_gray(110))
                                        .size(11.0),
                                    )
                                    .wrap(),
                                );
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    let can_probe = !self.is_probing_tools
                                        && !self.is_loading
                                        && !self.available_models.is_empty();
                                    let probe_btn = egui::Button::new(
                                        egui::RichText::new(if self.is_probing_tools {
                                            "Probing…"
                                        } else {
                                            "Probe tool calling"
                                        })
                                        .color(t.accent),
                                    );
                                    if ui.add_enabled(can_probe, probe_btn).clicked() {
                                        self.probe_tool_calling();
                                    }
                                    if self.is_probing_tools {
                                        ui.spinner();
                                    }
                                });
                                ui.add_space(6.0);
                                let probe_col = match self.tool_probe_supports {
                                    Some(true) => t.success,
                                    Some(false) => t.warn,
                                    None => egui::Color32::from_gray(140),
                                };
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&self.tool_probe_summary)
                                            .color(probe_col)
                                            .size(11.0),
                                    )
                                    .wrap(),
                                );
                                if !self.tool_probe_raw.is_empty() {
                                    egui::CollapsingHeader::new("Raw tool_calls JSON")
                                        .default_open(false)
                                        .show(ui, |ui| {
                                            egui::Frame::default()
                                                .fill(t.input)
                                                .corner_radius(6)
                                                .inner_margin(8)
                                                .show(ui, |ui| {
                                                    let mut raw = self.tool_probe_raw.clone();
                                                    ui.add(
                                                        egui::TextEdit::multiline(&mut raw)
                                                            .desired_rows(5)
                                                            .code_editor()
                                                            .interactive(false),
                                                    );
                                                });
                                        });
                                }

                                ui.add_space(12.0);
                                settings_section(
                                    ui,
                                    "Research, workspace & MCP",
                                    t.text,
                                    t.elevated,
                                    t.accent,
                                );
                                ui.add_space(4.0);
                                if ui
                                    .checkbox(
                                        &mut self.settings.enable_research_tools,
                                        "Research tools (WebFetch / WebSearch — network)",
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                if ui
                                    .checkbox(
                                        &mut self.settings.enable_workspace_tools,
                                        "Workspace tools (read-only file + glob)",
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Workspace folder")
                                            .color(egui::Color32::from_gray(140))
                                            .size(11.5),
                                    );
                                    if ui
                                        .add(
                                            egui::TextEdit::singleline(
                                                &mut self.settings.workspace_root,
                                            )
                                            .hint_text("empty = app working directory")
                                            .desired_width(ui.available_width().max(120.0)),
                                        )
                                        .changed()
                                    {
                                        settings_changed = true;
                                    }
                                });
                                ui.add_space(4.0);
                                if ui
                                    .checkbox(
                                        &mut self.settings.enable_mcp_tools,
                                        "Expose MCP tools to the model (after Refresh MCP)",
                                    )
                                    .changed()
                                {
                                    settings_changed = true;
                                }
                                ui.horizontal(|ui| {
                                    let can = !self.is_refreshing_mcp;
                                    let btn = egui::Button::new(egui::RichText::new(
                                        if self.is_refreshing_mcp {
                                            "Refreshing MCP…"
                                        } else {
                                            "Refresh MCP catalog"
                                        },
                                    )
                                    .color(t.accent));
                                    if ui.add_enabled(can, btn).clicked() {
                                        self.refresh_mcp_catalog();
                                    }
                                    if self.is_refreshing_mcp {
                                        ui.spinner();
                                    }
                                });
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&self.mcp_status_line)
                                            .color(egui::Color32::from_gray(115))
                                            .size(10.5),
                                    )
                                    .wrap(),
                                );

                                ui.add_space(12.0);
                                settings_section(ui, "System prompt", t.text, t.elevated, t.accent);
                                egui::Frame::default()
                                    .fill(t.input)
                                    .corner_radius(8)
                                    .stroke(egui::Stroke::new(1.0, t.border.gamma_multiply(0.65)))
                                    .show(ui, |ui| {
                                        if ui
                                            .add(
                                                egui::TextEdit::multiline(
                                                    &mut self.settings.system_prompt,
                                                )
                                                .desired_rows(3)
                                                .frame(false),
                                            )
                                            .changed()
                                        {
                                            settings_changed = true;
                                        }
                                    });

                                let (ch, tok) = self.system_prompt_stats();
                                ui.label(
                                    egui::RichText::new(format!(
                                        "~{tok} tokens est. · {ch} chars (after {{date}}/{{time}}/{{os}})"
                                    ))
                                    .color(egui::Color32::from_gray(100))
                                    .size(11.0),
                                );

                                egui::CollapsingHeader::new("Advanced system prompt")
                                    .default_open(false)
                                    .show(ui, |ui| {
                                        ui.checkbox(
                                            &mut self.session_only_prompt,
                                            "Use session-only prompt when set below",
                                        );
                                        ui.add(
                                            egui::TextEdit::multiline(&mut self.session_system_prompt)
                                                .desired_rows(2)
                                                .hint_text("Overrides global for this chat when checkbox above is on"),
                                        );
                                        ui.label(
                                            egui::RichText::new("Placeholders: {{date}}, {{time}}, {{os}}")
                                                .size(10.0)
                                                .color(egui::Color32::from_gray(100)),
                                        );

                                        ui.add_space(6.0);
                                        egui::ComboBox::from_id_salt("preset_pick")
                                            .selected_text("Apply preset…")
                                            .show_ui(ui, |ui| {
                                                for p in self.prompt_presets.clone() {
                                                    if ui.button(&p.name).clicked() {
                                                        self.settings.system_prompt = p.text.clone();
                                                        settings_changed = true;
                                                    }
                                                }
                                            });

                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(&mut self.new_preset_name)
                                                    .hint_text("New preset name")
                                                    .desired_width(120.0),
                                            );
                                            if ui.button("Save preset from global prompt").clicked()
                                                && !self.new_preset_name.trim().is_empty()
                                            {
                                                self.prompt_presets.push(PromptPreset {
                                                    name: self.new_preset_name.trim().to_string(),
                                                    text: self.settings.system_prompt.clone(),
                                                });
                                                self.new_preset_name.clear();
                                                settings_changed = true;
                                            }
                                        });

                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(&mut self.import_prompt_path)
                                                    .hint_text("Path to .txt")
                                                    .desired_width(180.0),
                                            );
                                            if ui.button("Import").clicked() {
                                                self.import_system_prompt_file();
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            let mut export_p = self.import_prompt_path.clone();
                                            ui.add(
                                                egui::TextEdit::singleline(&mut export_p)
                                                    .hint_text("Export path .txt")
                                                    .desired_width(180.0),
                                            );
                                            if ui.button("Export global prompt").clicked() {
                                                self.export_system_prompt_file(&export_p);
                                            }
                                        });
                                    });

                                ui.add_space(12.0);
                                ui.separator();
                                ui.add_space(6.0);
                                settings_section(ui, "Session", t.text, t.elevated, t.accent);
                                ui.horizontal(|ui| {
                                    if ui.button("Save session").clicked() {
                                        let _ = persist_mod::ensure_gui_dirs();
                                        self.save_current_session();
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.session_open_path)
                                            .hint_text("Path to session .json")
                                            .desired_width(200.0),
                                    );
                                    if ui.button("Open").clicked() {
                                        self.open_session_from_path(&self.session_open_path.clone());
                                    }
                                });
                                if let Ok(files) = persist_mod::list_session_files() {
                                    egui::ComboBox::from_id_salt("recent_sess")
                                        .selected_text("Recent sessions…")
                                        .show_ui(ui, |ui| {
                                            for p in files.iter().take(12) {
                                                let label = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                                                if ui.button(&label).clicked() {
                                                    let path = p.display().to_string();
                                                    self.session_open_path = path.clone();
                                                    self.open_session_from_path(&path);
                                                }
                                            }
                                        });
                                }

                                ui.add_space(16.0);
                                ui.separator();
                                ui.add_space(6.0);

                                settings_section(ui, "Statistics", t.text, t.elevated, t.accent);
                                ui.add_space(4.0);
                                egui::Frame::default()
                                    .fill(t.input)
                                    .corner_radius(6)
                                    .inner_margin(10)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Label::new(
                                                egui::RichText::new("Requests:")
                                                    .color(egui::Color32::from_gray(130)),
                                            ));
                                            ui.add(egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{}",
                                                    self.total_requests
                                                ))
                                                .color(egui::Color32::from_gray(220)),
                                            ));
                                        });
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Label::new(
                                                egui::RichText::new("~Tokens:")
                                                    .color(egui::Color32::from_gray(130)),
                                            ));
                                            ui.add(egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{}",
                                                    self.total_tokens
                                                ))
                                                .color(egui::Color32::from_gray(220)),
                                            ));
                                        });
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Label::new(
                                                egui::RichText::new("Models:")
                                                    .color(egui::Color32::from_gray(130)),
                                            ));
                                            ui.add(egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{}",
                                                    self.available_models.len()
                                                ))
                                                .color(egui::Color32::from_gray(220)),
                                            ));
                                        });
                                    });

                                ui.add_space(16.0);
                                if ui
                                    .button(egui::RichText::new("Test connection").color(t.accent))
                                    .clicked()
                                {
                                    self.test_connection();
                                }
                            });
                        });
                });
        }

        if settings_changed {
            self.persist_settings();
        }

        egui::TopBottomPanel::bottom("input")
            .min_height(108.0)
            .show(ctx, |ui| {
            egui::Frame::default()
                .fill(t.panel)
                .stroke(egui::Stroke::new(1.0, t.border.linear_multiply(0.9)))
                .inner_margin(Margin::symmetric(18, 16))
                .show(ui, |ui| {
                    let dock = ui.max_rect();
                    ui.painter_at(dock).rect_filled(
                        egui::Rect::from_min_max(
                            dock.left_top(),
                            egui::pos2(dock.right(), dock.top() + 2.0),
                        ),
                        CornerRadius::ZERO,
                        t.border.linear_multiply(0.85),
                    );
                    let mut send_via_enter = false;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let full = ui.available_width();
                        let stop_w = 78.0_f32;
                        let send_w = 100.0_f32;
                        let gap = 12.0_f32;
                        let composer_w = (full - stop_w - send_w - gap * 2.0).max(200.0);
                        let row_h = 48.0_f32;

                        let stop_enabled = self.is_loading && self.stream_cancel.is_some();
                        let stop_btn = egui::Button::new(
                            egui::RichText::new("Stop")
                                .size(13.0)
                                .strong()
                                .color(if stop_enabled {
                                    t.error
                                } else {
                                    t.text_dim
                                }),
                        )
                        .fill(if stop_enabled {
                            t.elevated
                        } else {
                            t.input
                        })
                        .stroke(egui::Stroke::new(
                            1.5,
                            if stop_enabled {
                                t.error.linear_multiply(0.55)
                            } else {
                                t.border.linear_multiply(0.65)
                            },
                        ))
                        .corner_radius(12.0)
                        .min_size(Vec2::new(stop_w, row_h));
                        if ui
                            .add_enabled(stop_enabled, stop_btn)
                            .on_hover_text("Stop streaming (available while the reply is streaming)")
                            .clicked()
                        {
                            self.stop_generation();
                        }

                        egui::Frame::default()
                            .fill(t.input)
                            .corner_radius(14)
                            .stroke(egui::Stroke::new(1.5, t.border.linear_multiply(0.95)))
                            .inner_margin(Margin::symmetric(14, 12))
                            .show(ui, |ui| {
                                ui.set_width(composer_w);
                                ui.set_min_height(72.0);
                                let out = egui::TextEdit::multiline(&mut self.input)
                                    .id_salt("claw_composer")
                                    .return_key(egui::KeyboardShortcut::new(
                                        egui::Modifiers::SHIFT,
                                        egui::Key::Enter,
                                    ))
                                    .hint_text(
                                        egui::RichText::new(
                                            "Write a message…  ·  Enter to send  ·  Shift+Enter new line",
                                        )
                                        .size(13.5)
                                        .color(t.text_dim),
                                    )
                                    .desired_width(ui.available_width())
                                    .desired_rows(5)
                                    .frame(false)
                                    .show(ui);
                                let can_send = !self.is_loading
                                    && !self.is_probing_tools
                                    && !self.input.trim().is_empty();
                                if out.response.has_focus()
                                    && ui.ctx().input(|i| {
                                        i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
                                    })
                                    && can_send
                                {
                                    send_via_enter = true;
                                }
                            });

                        let enabled = !self.is_loading
                            && !self.is_probing_tools
                            && !self.input.trim().is_empty();
                        let send_fg = if enabled {
                            egui::Color32::from_rgb(12, 12, 14)
                        } else {
                            t.text_dim
                        };
                        let btn = egui::Button::new(
                            egui::RichText::new("Send")
                                .size(14.0)
                                .strong()
                                .color(send_fg),
                        )
                        .fill(if enabled {
                            t.accent
                        } else {
                            t.elevated
                        })
                        .stroke(if enabled {
                            egui::Stroke::new(1.5, t.border.linear_multiply(0.35))
                        } else {
                            egui::Stroke::new(1.0, t.border.linear_multiply(0.55))
                        })
                        .corner_radius(12.0)
                        .min_size(Vec2::new(send_w, row_h));

                        if ui.add_enabled(enabled, btn).clicked() {
                            self.send_message();
                        }
                    });
                    if send_via_enter {
                        self.send_message();
                    }
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let canvas = ui.max_rect();
            paint_chat_canvas_gradient(
                &ui.painter_at(canvas),
                canvas,
                t.chat_bg_top,
                t.chat_bg_bottom,
            );
            egui::Frame::default()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(Margin::symmetric(24, 20))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("claw_chat_scroll")
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let full_w = ui.available_width();
                            // Readable line length: cap width even on ultra-wide windows.
                            let bubble_max = 560.0_f32;
                            let max_w = bubble_max.min(full_w * 0.78);

                            let mut msgs_to_copy: Vec<usize> = Vec::new();

                            for (mi, msg) in self.messages.iter().enumerate() {
                                let is_user = msg.role == "user";
                                let content = &msg.content;
                                if is_user && content.trim().is_empty() {
                                    continue;
                                }

                                ui.add_space(8.0);
                                if is_user {
                                    ui.with_layout(
                                        egui::Layout::top_down(egui::Align::Max),
                                        |ui| {
                                            ui.set_width(ui.available_width());
                                            egui::Frame::default()
                                                .fill(t.user_bubble)
                                                .corner_radius(20)
                                                .inner_margin(Margin::symmetric(16, 13))
                                                .stroke(egui::Stroke::new(
                                                    1.5,
                                                    t.border.linear_multiply(0.9),
                                                ))
                                                .shadow(t.bubble_shadow)
                                                .show(ui, |ui| {
                                                    ui.set_max_width(max_w);
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(content)
                                                                .color(t.user_message_text)
                                                                .size(self.settings.font_size),
                                                        )
                                                        .wrap(),
                                                    );
                                                });
                                        },
                                    );
                                } else if content == GUI_WELCOME_MARKER {
                                    ui.add_space(10.0);
                                    let card_w = (ui.available_width() - 12.0).min(720.0).max(320.0);
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(card_w, 0.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            render_welcome_card(ui, &t, self.settings.font_size);
                                        },
                                    );
                                } else if !content.trim().is_empty() {
                                    let streaming_here = self.is_loading
                                        && self.stream_cancel.is_some()
                                        && mi + 1 == self.messages.len();
                                    let copy_id = msg.id;
                                    let copy_flashing = self
                                        .copy_flash
                                        .as_ref()
                                        .is_some_and(|(id, until)| {
                                            *id == copy_id && Instant::now() < *until
                                        });
                                    let copy_lbl = if copy_flashing {
                                        "Copied ✓"
                                    } else {
                                        "Copy"
                                    };
                                    egui::Frame::default()
                                        .fill(t.assistant_bubble)
                                        .corner_radius(18)
                                        .inner_margin(Margin::symmetric(15, 13))
                                        .stroke(egui::Stroke::new(
                                            1.0,
                                            t.border.linear_multiply(0.85),
                                        ))
                                        .shadow(t.bubble_shadow)
                                        .show(ui, |ui| {
                                            let bubble_w = max_w.min(ui.available_width());
                                            ui.set_width(bubble_w);
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("●")
                                                        .size(9.0)
                                                        .color(t.accent),
                                                );
                                                ui.add_space(4.0);
                                                ui.label(
                                                    egui::RichText::new("Assistant")
                                                        .size(11.0)
                                                        .color(t.text_dim),
                                                );
                                                if streaming_here {
                                                    ui.add_space(6.0);
                                                    ui.label(
                                                        egui::RichText::new(" STREAMING ")
                                                            .size(9.0)
                                                            .strong()
                                                            .color(t.panel)
                                                            .background_color(
                                                                t.accent.linear_multiply(0.42),
                                                            ),
                                                    );
                                                }
                                                ui.add_space(
                                                    (ui.available_width() - 88.0).max(0.0),
                                                );
                                                let copy_btn = egui::Button::new(
                                                    egui::RichText::new(copy_lbl)
                                                        .size(12.0)
                                                        .color(if copy_flashing {
                                                            t.success
                                                        } else {
                                                            t.accent
                                                        }),
                                                );
                                                if ui
                                                    .add(copy_btn)
                                                    .on_hover_text("Copy this message")
                                                    .clicked()
                                                {
                                                    msgs_to_copy.push(copy_id);
                                                }
                                            });
                                            ui.add_space(6.0);
                                            markdown::show_markdown(
                                                ui,
                                                content,
                                                self.settings.font_size,
                                                &markdown::MarkdownTheme {
                                                    body: t.md_body,
                                                    code_bg: t.md_code_bg,
                                                    code_stroke: t.md_code_stroke,
                                                    heading: t.md_heading,
                                                },
                                            );
                                            if streaming_here {
                                                ui.add_space(2.0);
                                                let caret_on = (ui.ctx().input(|i| i.time) * 2.4)
                                                    as i32
                                                    % 2
                                                    == 0;
                                                ui.horizontal(|ui| {
                                                    if caret_on {
                                                        ui.label(
                                                            egui::RichText::new("▍")
                                                                .font(egui::FontId::monospace(
                                                                    self.settings.font_size,
                                                                ))
                                                                .color(t.accent),
                                                        );
                                                    }
                                                });
                                            }
                                        });
                                }
                            }

                            for id in msgs_to_copy {
                                self.copy_message(ctx, id);
                            }

                            let show_thinking_spinner = self.is_loading
                                && !self.messages.iter().rev().any(|m| {
                                    m.role == "assistant" && !m.content.trim().is_empty()
                                });
                            if show_thinking_spinner {
                                ui.add_space(8.0);
                                egui::Frame::default()
                                    .fill(t.assistant_bubble)
                                    .corner_radius(18)
                                    .inner_margin(Margin::symmetric(15, 13))
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        t.border.linear_multiply(0.85),
                                    ))
                                    .shadow(t.bubble_shadow)
                                    .show(ui, |ui| {
                                        let bubble_w = max_w.min(ui.available_width());
                                        ui.set_width(bubble_w);
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new("●")
                                                    .size(9.0)
                                                    .color(t.accent),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new("Assistant")
                                                    .size(11.0)
                                                    .color(t.text_dim),
                                            );
                                        });
                                        ui.add_space(6.0);
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.add(egui::Label::new(
                                                egui::RichText::new("Waiting for first token…")
                                                    .color(t.text_muted)
                                                    .size(13.0),
                                            ));
                                        });
                                    });
                            }

                            ui.add_space(16.0);
                        });
                });
        });
    }

    fn system_prompt_stats(&self) -> (usize, usize) {
        let s = self.compose_system_prompt();
        let chars = s.len();
        let est_tokens = chars / 4;
        (chars, est_tokens)
    }
}

impl Default for ClawGui {
    fn default() -> Self {
        Self::new().expect("ClawGui::new")
    }
}

impl eframe::App for ClawGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_settings();
    }
}

pub fn run_gui() -> eframe::Result<()> {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        eprintln!("GUI panicked: {info}");
        default_hook(info);
    }));

    // Glow avoids deep wgpu/naga stacks vs default wgpu when both backends are enabled.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(1400.0, 950.0))
            .with_min_inner_size(Vec2::new(900.0, 650.0))
            .with_title("Claw Code · Tidepool")
            .with_resizable(true),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    let result = eframe::run_native(
        "Claw Code · Tidepool",
        options,
        Box::new(|_cc| match ClawGui::new() {
            Ok(gui) => Ok(Box::new(gui)),
            Err(e) => {
                eprintln!("ERROR: Failed to create GUI: {e}");
                Err(
                    Box::new(io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to create GUI: {e}"),
                    )) as Box<dyn std::error::Error + Send + Sync>,
                )
            }
        }),
    );

    if let Err(e) = &result {
        eprintln!("GUI error: {e}");
    }

    result
}
