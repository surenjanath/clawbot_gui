use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::types::{GuiConfigFile, SessionFile};

/// Config + session root: `$CLAW_CONFIG_HOME/gui` or `~/.claw/gui` (via dirs).
pub fn gui_data_dir() -> PathBuf {
    if let Ok(h) = std::env::var("CLAW_CONFIG_HOME") {
        return PathBuf::from(h).join("gui");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claw")
        .join("gui")
}

pub fn settings_path() -> PathBuf {
    gui_data_dir().join("settings.json")
}

pub fn sessions_dir() -> PathBuf {
    gui_data_dir().join("sessions")
}

pub fn ensure_gui_dirs() -> io::Result<()> {
    fs::create_dir_all(gui_data_dir())?;
    fs::create_dir_all(sessions_dir())?;
    Ok(())
}

pub fn load_config() -> io::Result<GuiConfigFile> {
    let path = settings_path();
    if !path.exists() {
        return Ok(GuiConfigFile::default());
    }
    let s = fs::read_to_string(&path)?;
    serde_json::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn save_config(cfg: &GuiConfigFile) -> io::Result<()> {
    ensure_gui_dirs()?;
    let path = settings_path();
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(cfg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, format!("{data}\n"))?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn load_session(path: &Path) -> io::Result<SessionFile> {
    let s = fs::read_to_string(path)?;
    serde_json::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn save_session(path: &Path, session: &SessionFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let data =
        serde_json::to_string_pretty(session).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, format!("{data}\n"))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn list_session_files() -> io::Result<Vec<PathBuf>> {
    let dir = sessions_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut v: Vec<PathBuf> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    v.sort();
    Ok(v)
}

impl Default for GuiConfigFile {
    fn default() -> Self {
        Self {
            settings: super::types::AppSettings::default(),
            prompt_presets: default_presets(),
        }
    }
}

fn default_presets() -> Vec<super::types::PromptPreset> {
    vec![
        super::types::PromptPreset {
            name: "Coding assistant".to_string(),
            text: "You are an expert software engineer. Be concise, use markdown for code blocks, and explain trade-offs when relevant. Current date: {{date}}.".to_string(),
        },
        super::types::PromptPreset {
            name: "Code review".to_string(),
            text: "You are a senior reviewer. Focus on correctness, security, performance, and clarity. Use bullet points. Date: {{date}}.".to_string(),
        },
        super::types::PromptPreset {
            name: "Creative writing".to_string(),
            text: "You are a creative writing assistant. Be vivid and engaging unless asked otherwise.".to_string(),
        },
    ]
}
