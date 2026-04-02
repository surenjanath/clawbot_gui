
#[derive(PartialEq)]
pub enum TuiMode {
    Chat,
    Settings,
    Help,
    Exit,
}

pub struct TuiState {
    pub mode: TuiMode,
    pub input: String,
    pub messages: Vec<String>,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            mode: TuiMode::Chat,
            input: String::new(),
            messages: Vec::new(),
        }
    }
}

pub fn render_menu() {
    println!("╔══════════════════════════════════════╗");
    println!("║        🦞 Claw Code Interface        ║");
    println!("╠══════════════════════════════════════╣");
    println!("║  [1] Chat           - Start a chat  ║");
    println!("║  [2] Settings       - Configure      ║");
    println!("║  [3] Help           - View help      ║");
    println!("║  [4] Exit           - Quit           ║");
    println!("╚══════════════════════════════════════╝");
    print!("\nSelect an option: ");
}

pub fn render_chat() {
    println!("╔══════════════════════════════════════╗");
    println!("║            🦞 Chat Mode              ║");
    println!("╠══════════════════════════════════════╣");
    println!("║ Type your message and press Enter   ║");
    println!("║ Type /back to return to menu         ║");
    println!("║ Type /clear to clear chat history    ║");
    println!("╚══════════════════════════════════════╝");
    println!();
}

pub fn render_settings() {
    println!("╔══════════════════════════════════════╗");
    println!("║           ⚙️ Settings                 ║");
    println!("╠══════════════════════════════════════╣");
    println!("║  [1] Model         - Set AI model   ║");
    println!("║  [2] API Key       - Configure API  ║");
    println!("║  [3] Permissions   - Set permissions║");
    println!("║  [4] Theme         - UI theme       ║");
    println!("║  [5] Back          - Return to menu ║");
    println!("╚══════════════════════════════════════╝");
    print!("\nSelect an option: ");
}

pub fn render_help() {
    println!("╔══════════════════════════════════════╗");
    println!("║             🦞 Help                   ║");
    println!("╠══════════════════════════════════════╣");
    println!("║ Welcome to Claw Code!               ║");
    println!("║                                      ║");
    println!("║ This is a CLI agent for code tasks.  ║");
    println!("║ Use the menu to navigate.            ║");
    println!("║                                      ║");
    println!("║ Commands:                            ║");
    println!("║   /back  - Return to menu           ║");
    println!("║   /clear - Clear screen             ║");
    println!("║   /help  - Show this help           ║");
    println!("║                                      ║");
    println!("║ Press Enter to return to menu...    ║");
    println!("╚══════════════════════════════════════╝");
}

pub fn handle_menu_input(input: &str) -> TuiMode {
    match input.trim() {
        "1" => TuiMode::Chat,
        "2" => TuiMode::Settings,
        "3" => TuiMode::Help,
        "4" => TuiMode::Exit,
        _ => TuiMode::Chat,
    }
}

pub fn handle_chat_input(input: &str) -> Option<TuiMode> {
    match input.trim() {
        "/back" => Some(TuiMode::Chat),
        "/exit" => Some(TuiMode::Exit),
        _ => None,
    }
}

pub fn handle_settings_input(input: &str) -> TuiMode {
    match input.trim() {
        "5" => TuiMode::Chat,
        _ => TuiMode::Settings,
    }
}