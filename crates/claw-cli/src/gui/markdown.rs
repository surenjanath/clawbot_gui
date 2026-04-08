//! Minimal markdown rendering for assistant bubbles (pulldown-cmark → egui).

use eframe::egui;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Per-theme markdown colors (from `ClawTheme` in the app).
#[derive(Clone, Copy)]
pub struct MarkdownTheme {
    pub body: egui::Color32,
    pub code_bg: egui::Color32,
    pub code_stroke: egui::Color32,
    pub heading: egui::Color32,
}

pub fn show_markdown(ui: &mut egui::Ui, text: &str, base_size: f32, theme: &MarkdownTheme) {
    let MarkdownTheme {
        body,
        code_bg,
        code_stroke,
        heading: heading_col,
    } = *theme;
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(text, options);

    let mut in_code_block = false;
    let mut code_buf = String::new();
    let mut in_paragraph = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_paragraph(ui, &mut in_paragraph);
                let n = heading_level_num(level);
                let size = base_size + (4.0 - f32::from(n)) * 2.0;
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!("{} ", heading_marker(level)))
                        .size(size)
                        .color(heading_col),
                );
            }
            Event::End(TagEnd::Heading(_)) => {
                ui.add_space(4.0);
            }
            Event::Start(Tag::Paragraph) => {
                in_paragraph = true;
            }
            Event::End(TagEnd::Paragraph) => {
                flush_paragraph(ui, &mut in_paragraph);
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_paragraph(ui, &mut in_paragraph);
                in_code_block = true;
                code_buf.clear();
                if let CodeBlockKind::Fenced(lang) = kind {
                    if !lang.is_empty() {
                        code_buf.push_str(&format!("// {}\n", lang));
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let stroke = egui::Stroke::new(1.0, code_stroke);
                egui::Frame::default()
                    .fill(code_bg)
                    .corner_radius(10)
                    .stroke(stroke)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&code_buf)
                                    .font(egui::FontId::monospace(base_size - 1.0))
                                    .color(body),
                            )
                            .wrap(),
                        );
                    });
                code_buf.clear();
                ui.add_space(6.0);
            }
            Event::Start(Tag::List(_)) => {
                ui.add_space(4.0);
            }
            Event::End(TagEnd::List(_)) => {
                ui.add_space(4.0);
            }
            Event::End(TagEnd::Item) => {
                ui.add_space(2.0);
            }
            Event::Code(code) => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(code.as_ref())
                            .font(egui::FontId::monospace(base_size - 1.0))
                            .background_color(code_bg)
                            .color(body),
                    );
                });
            }
            Event::Text(t) => {
                if in_code_block {
                    code_buf.push_str(t.as_ref());
                } else {
                    ui.label(
                        egui::RichText::new(t.as_ref())
                            .size(base_size)
                            .color(body),
                    );
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_buf.push('\n');
                } else {
                    ui.add_space(4.0);
                }
            }
            Event::Rule => {
                ui.separator();
            }
            _ => {}
        }
    }
    flush_paragraph(ui, &mut in_paragraph);
}

fn heading_level_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn heading_marker(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "#",
        HeadingLevel::H2 => "##",
        HeadingLevel::H3 => "###",
        HeadingLevel::H4 => "####",
        HeadingLevel::H5 => "#####",
        HeadingLevel::H6 => "######",
    }
}

fn flush_paragraph(ui: &mut egui::Ui, in_paragraph: &mut bool) {
    if *in_paragraph {
        ui.add_space(2.0);
        *in_paragraph = false;
    }
}
