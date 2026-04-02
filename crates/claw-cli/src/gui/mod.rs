//! Claw egui front-end (Ollama chat; optional Claw API later — see `backend`).

mod app;
mod backend;
mod markdown;
mod ollama;
mod persist;
mod types;

pub use app::run_gui;
#[allow(unused_imports)] // Public surface for embedding / future use
pub use types::{AppSettings, ChatMessage, OllamaSettings};
