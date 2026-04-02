//! Future: optional Claw `ProviderClient` / Anthropic path from the same window.
//! Phase 2 — not wired into the UI yet.

/// Which backend the GUI targets (Ollama is fully implemented).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum GuiBackend {
    #[default]
    Ollama,
    /// Reserved: stream via `api::ProviderClient` + OAuth / API keys (see plan Phase 2).
    ClawApi,
}

impl GuiBackend {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama (local)",
            Self::ClawApi => "Claw API (coming soon)",
        }
    }
}
