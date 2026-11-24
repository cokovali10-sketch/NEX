//! NEX Studio (egui) — a PyCharm-style desktop IDE for the NEX language.
//!
//! Rust + egui (GPU) + egui_code_editor (syntax-highlighted editable code).
//! Compiles NEX in process (nex_driver) and runs the result.

#![allow(deprecated)]

use eframe::egui;
use egui::Color32;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use std::time::Instant;

const EXAMPLE: &str = r#"struct Item:
    name: string
    price: i64

fn main():
    var cart: [Item] = []
    push(cart, Item("apple", 10))
    push(cart, Item("bread", 25))
    push(cart, Item("milk", 18))
    var total = 0
    for it in cart:
        println(it.name + ": " + to_string(it.price))
        total = total + it.price
    println(f"items: {len(cart)}, total: {total}")
"#;

const C_TEXT: Color32 = Color32::from_rgb(169, 183, 198);
const C_MUTED: Color32 = Color32::from_rgb(120, 128, 138);
const C_BG: Color32 = Color32::from_rgb(43, 43, 43);
const C_PANEL: Color32 = Color32::from_rgb(49, 51, 53);
const C_TOOL: Color32 = Color32::from_rgb(60, 63, 65);
const C_GREEN: Color32 = Color32::from_rgb(106, 154, 76);
const C_BLUE: Color32 = Color32::from_rgb(0, 122, 204);
const C_ACCENT: Color32 = Color32::from_rgb(54, 88, 128);

fn nex_syntax() -> Syntax {
    Syntax::new("NEX")
        .with_comment("#")
        .with_keywords([
            "fn", "let", "var", "if", "elif", "else", "while", "for", "in", "match", "return",
            "struct", "impl", "pub", "use", "from", "true", "false", "and", "or", "not", "mut",
            "unsafe", "defer", "break", "continue", "pass", "loop", "import", "as", "extern",
        ])
        .with_types([
            "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "bool", "char",
            "string", "void",
        ])
}

#[derive(Clone, Copy, PartialEq)]
enum Activity {
    Files,
    Search,
}

struct Studio {
    code: String,
    output: String,
    status: String,
    files: Vec<(String, String)>,
    active: usize,
    activity: Activity,
    find: String,
    font_size: f32,
    compile_time: Option<f64>,
    cursor_line: usize,
    cursor_col: usize,
    show_console: bool,
}

impl Studio {
    fn new() -> Self {
        let files = vec![
            ("main.nex".to_string(), EXAMPLE.to_string()),
            (
                "libs/mathx.nex".to_string(),
                include_str!("../../../libs/mathx.nex").to_string(),
            ),
            (
                "libs/strx.nex".to_string(),
                include_str!("../../../libs/strx.nex").to_string(),
            ),
            (
                "libs/seq.nex".to_string(),
                include_str!("../../../libs/seq.nex").to_string(),
            ),
            (
                "libs/examples/functions20.nex".to_string(),
                include_str!("../../../libs/examples/functions20.nex").to_string(),
            ),
        ];
        Self {
            code: files[0].1.clone(),
            output: "Ready. Press Run (Ctrl+Enter) to compile and execute.\n".to_string(),
            status: "ready".to_string(),
            files,
            active: 0,
            activity: Activity::Files,
            find: String::new(),
            font_size: 14.0,
            compile_time: None,
            cursor_line: 1,
            cursor_col: 1,
            show_console: true,
        }
    }

    fn run(&mut self) {
        self.show_console = true;
        let started = Instant::now();
        self.status = "compiling…".to_string();
        self.output.clear();

        let dir = std::env::temp_dir().join("nex_studio_egui");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        if let Err(error) = std::fs::write(dir.join("main.nex"), &self.code) {
            self.output = format!("failed to write source: {error}\n");
            self.status = "error".to_string();
            return;
        }
        let options = nex_driver::CompileOptions {
            build_dir: Some(dir.clone()),
            ..Default::default()
        };
        let result = match nex_driver::compile_source("main.nex", self.code.clone(), options) {
            Ok(result) => result,
            Err(error) => {
                self.output = format!("compile error: {error}\n");
                self.status = "error".to_string();
                self.compile_time = Some(started.elapsed().as_secs_f64());
                return;
            }
        };

        let mut out = String::new();
        for diagnostic in &result.diagnostics {
            let start = diagnostic.span.map(|span| span.start).unwrap_or(0);
            let line = self.code.as_bytes()[..start.min(self.code.len())]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
                + 1;
            let label = if diagnostic.is_error() {
                "error"
            } else {
                "warning"
            };
            out.push_str(&format!(
                "{label} [{}] line {line}: {}\n",
                diagnostic.code, diagnostic.message
            ));
        }
        if result.has_errors() {
            self.output = out;
            self.status = "error".to_string();
            self.compile_time = Some(started.elapsed().as_secs_f64());
            return;
        }
        let Some(artifacts) = result.artifacts else {
            self.output = "no artifacts produced\n".to_string();
            self.status = "error".to_string();
            return;
        };
        let Some(exe) = artifacts.executable else {
            self.output = "no executable produced\n".to_string();
            self.status = "error".to_string();
            return;
        };
        match std::process::Command::new(&exe).current_dir(&dir).output() {
            Ok(run) => {
                out.push_str(&String::from_utf8_lossy(&run.stdout));
                let err = String::from_utf8_lossy(&run.stderr);
                if !err.is_empty() {
                    out.push_str(&err);
                }
                let code = run.status.code().unwrap_or(0);
                self.status = if code == 0 {
                    "ok".to_string()
                } else {
                    format!("exit {code}")
                };
            }
            Err(error) => {
                out.push_str(&format!("\nfailed to run executable: {error}\n"));
                self.status = "run error".to_string();
            }
        }
        self.compile_time = Some(started.elapsed().as_secs_f64());
        self.output = out;
    }

    fn switch_to(&mut self, index: usize) {
        persist_current(&mut self.files, self.active, &self.code);
        self.active = index;
        self.code = self.files[index].1.clone();
        self.cursor_line = 1;
        self.cursor_col = 1;
    }
}

fn persist_current(files: &mut [(String, String)], active: usize, code: &str) {
    if let Some(slot) = files.get_mut(active) {
        slot.1 = code.to_string();
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut s = (*ctx.global_style()).clone();
    s.visuals = egui::Visuals::dark();

    let border = Color32::from_rgb(72, 72, 72);
    s.visuals.panel_fill = C_BG;
    s.visuals.window_fill = C_BG;
    s.visuals.faint_bg_color = C_PANEL;
    s.visuals.extreme_bg_color = Color32::from_rgb(30, 30, 30);
    s.visuals.widgets.noninteractive.bg_fill = C_BG;
    s.visuals.widgets.inactive.bg_fill = C_TOOL;
    s.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0f32, border);
    s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(75, 78, 80);
    s.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0f32, Color32::from_rgb(96, 99, 102));
    s.visuals.widgets.active.bg_fill = C_ACCENT;
    s.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0f32, C_ACCENT);
    s.visuals.selection.bg_fill = C_ACCENT;
    s.visuals.selection.stroke = egui::Stroke::new(1.0f32, Color32::from_rgb(88, 166, 255));
    s.visuals.window_stroke = egui::Stroke::new(1.0f32, border);
    s.visuals.override_text_color = Some(C_TEXT);

    s.spacing.item_spacing = egui::vec2(9.0, 7.0);
    s.spacing.button_padding = egui::vec2(12.0, 5.0);
    s.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
    );
    s.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(11.5, egui::FontFamily::Proportional),
    );
    ctx.set_global_style(s);
}

fn draw_activity_icon(ui: &egui::Ui, rect: egui::Rect, kind: Activity, color: Color32) {
    let p = ui.painter();
    let c = rect.center();
    let stroke = egui::Stroke::new(1.6f32, color);
    match kind {
        Activity::Files => {
            let r1 = egui::Rect::from_min_size(
                egui::pos2(c.x - 9.0, c.y - 10.0),
                egui::vec2(14.0, 18.0),
            );
            p.rect_stroke(
                r1,
                egui::CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            for i in 0..3 {
                let y = r1.top() + 5.0 + i as f32 * 4.0;
                p.line_segment(
                    [
                        egui::pos2(r1.left() + 3.0, y),
                        egui::pos2(r1.right() - 3.0, y),
                    ],
                    stroke,
                );
            }
        }
        Activity::Search => {
            p.circle_stroke(egui::pos2(c.x - 2.0, c.y - 2.0), 7.0, stroke);
            p.line_segment(
                [
                    egui::pos2(c.x + 3.0, c.y + 3.0),
                    egui::pos2(c.x + 9.0, c.y + 9.0),
                ],
                stroke,
            );
        }
    }
}

impl eframe::App for Studio {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {}

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Keyboard shortcuts.
        if ctx
            .input(|i| i.key_pressed(egui::Key::Enter) && (i.modifiers.ctrl || i.modifiers.command))
        {
            self.run();
        }
        if ctx.input(|i| {
            (i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus))
                && (i.modifiers.ctrl || i.modifiers.command)
        }) {
            self.font_size = (self.font_size + 1.0).min(28.0);
        }
        if ctx
            .input(|i| i.key_pressed(egui::Key::Minus) && (i.modifiers.ctrl || i.modifiers.command))
        {
            self.font_size = (self.font_size - 1.0).max(9.0);
        }

        // Menu bar.
        egui::Panel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New file").clicked() {
                        self.files.push(("untitled.nex".to_string(), String::new()));
                        self.switch_to(self.files.len() - 1);
                        ui.close_menu();
                    }
                    if ui.button("Load example").clicked() {
                        self.code = EXAMPLE.to_string();
                        persist_current(&mut self.files, self.active, &self.code);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Run", |ui| {
                    if ui.button("Run  (Ctrl+Enter)").clicked() {
                        self.run();
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Toggle console").clicked() {
                        self.show_console = !self.show_console;
                        ui.close_menu();
                    }
                    if ui.button("Clear console").clicked() {
                        self.output.clear();
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(C_MUTED, &self.status);
                });
            });
        });

        // Status bar.
        egui::Panel::bottom("status_bar").show(ctx, |ui| {
            egui::Frame::new()
                .fill(C_BLUE)
                .inner_margin(egui::Margin::symmetric(10, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::WHITE, "main");
                        ui.separator();
                        ui.colored_label(
                            Color32::WHITE,
                            format!("Ln {}, Col {}", self.cursor_line, self.cursor_col),
                        );
                        ui.separator();
                        ui.colored_label(Color32::WHITE, "UTF-8");
                        ui.separator();
                        ui.colored_label(Color32::WHITE, "Spaces: 4");
                        ui.separator();
                        ui.colored_label(Color32::WHITE, "NEX");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let msg = match self.compile_time {
                                Some(t) => format!("{}  ·  {:.2}s", self.status, t),
                                None => self.status.clone(),
                            };
                            ui.colored_label(Color32::WHITE, msg);
                        });
                    });
                });
        });

        // Activity bar.
        egui::Panel::left("activity")
            .exact_size(50.0)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(30, 30, 30))
                    .inner_margin(egui::Margin::symmetric(4, 6))
                    .show(ui, |ui| {
                        for kind in [Activity::Files, Activity::Search] {
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(40.0, 44.0), egui::Sense::click());
                            let active = self.activity == kind;
                            if resp.clicked() {
                                self.activity = kind;
                            }
                            let color = if active { Color32::WHITE } else { C_MUTED };
                            if active {
                                ui.painter().rect_filled(
                                    rect,
                                    egui::CornerRadius::same(4),
                                    C_ACCENT,
                                );
                            }
                            draw_activity_icon(ui, rect, kind, color);
                        }
                    });
            });

        // Sidebar.
        egui::Panel::left("files")
            .resizable(true)
            .default_size(220.0)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(C_PANEL)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        if self.activity == Activity::Search {
                            ui.colored_label(C_MUTED, "SEARCH");
                            ui.add_space(4.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.find)
                                    .hint_text("Search in file…")
                                    .desired_width(f32::INFINITY),
                            );
                            ui.add_space(6.0);
                            if !self.find.is_empty() {
                                let mut count = 0;
                                for (i, line) in self.code.lines().enumerate() {
                                    if line.contains(&self.find) {
                                        count += 1;
                                        ui.colored_label(
                                            C_TEXT,
                                            format!("{:>4}: {}", i + 1, line.trim()),
                                        );
                                    }
                                }
                                if count == 0 {
                                    ui.colored_label(C_MUTED, "No matches");
                                }
                            }
                        } else {
                            ui.colored_label(C_MUTED, "PROJECT");
                            ui.add_space(4.0);
                            ui.colored_label(C_TEXT, "▾  NEX");
                            let file_names: Vec<String> =
                                self.files.iter().map(|(n, _)| n.clone()).collect();
                            for (index, name) in file_names.iter().enumerate() {
                                let selected = self.active == index;
                                let indent = if name.contains('/') { 16.0 } else { 4.0 };
                                ui.horizontal(|ui| {
                                    ui.add_space(indent);
                                    let icon = if name.ends_with(".nex") { "◆" } else { "•" };
                                    let text = egui::RichText::new(format!("{icon} {name}"))
                                        .monospace()
                                        .size(13.0)
                                        .color(if selected { Color32::WHITE } else { C_TEXT });
                                    if ui.selectable_label(selected, text).clicked() {
                                        self.switch_to(index);
                                    }
                                });
                            }
                        }
                    });
            });

        // Central.
        egui::CentralPanel::default().show(ctx, |ui| {
            // Toolbar.
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new("▶ Run")
                            .fill(C_GREEN)
                            .min_size(egui::vec2(84.0, 26.0)),
                    )
                    .clicked()
                {
                    self.run();
                }
                ui.separator();
                // Font size controls.
                if ui.small_button("A-").clicked() {
                    self.font_size = (self.font_size - 1.0).max(9.0);
                }
                if ui.small_button("A+").clicked() {
                    self.font_size = (self.font_size + 1.0).min(28.0);
                }
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Example").clicked() {
                        self.code = EXAMPLE.to_string();
                        persist_current(&mut self.files, self.active, &self.code);
                    }
                });
            });
            ui.add_space(2.0);

            // Tab bar.
            egui::Frame::new()
                .fill(C_TOOL)
                .inner_margin(egui::Margin::symmetric(4, 3))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let file_names: Vec<String> =
                            self.files.iter().map(|(n, _)| n.clone()).collect();
                        for (index, name) in file_names.iter().enumerate() {
                            let active = self.active == index;
                            egui::Frame::new()
                                .fill(if active { C_BG } else { C_TOOL })
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let text = egui::RichText::new(format!("● {name}"))
                                            .monospace()
                                            .color(if active { Color32::WHITE } else { C_MUTED });
                                        if ui.selectable_label(active, text).clicked() {
                                            self.switch_to(index);
                                        }
                                    });
                                });
                        }
                    });
                });
            ui.add_space(2.0);

            // Editor.
            let console_reserve = if self.show_console { 170.0 } else { 20.0 };
            let avail = ui.available_height();
            let rows = (((avail - console_reserve - 14.0) / (self.font_size * 1.42)).floor()
                as usize)
                .max(5);
            let mut editor = CodeEditor::default()
                .id_source("main")
                .with_theme(ColorTheme::SONOKAI)
                .with_fontsize(self.font_size)
                .with_rows(rows)
                .with_numlines(true)
                .with_syntax(nex_syntax());
            let response = editor.show(ui, &mut self.code);
            if response.response.changed() {
                persist_current(&mut self.files, self.active, &self.code);
            }
            // Cursor line/col.
            let char_off = response.cursor_range.map(|r| r.primary.index).unwrap_or(0);
            let offset = self
                .code
                .char_indices()
                .nth(char_off)
                .map(|(b, _)| b)
                .unwrap_or(self.code.len());
            let bytes = &self.code.as_bytes()[..offset];
            let line = bytes.iter().filter(|b| **b == b'\n').count() + 1;
            let col = bytes.iter().rev().take_while(|b| **b != b'\n').count() + 1;
            self.cursor_line = line;
            self.cursor_col = col;

            if self.show_console {
                ui.add_space(5.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.colored_label(C_MUTED, "CONSOLE");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            self.output.clear();
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(160.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.output)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(6)
                                .interactive(false)
                                .text_color(C_TEXT),
                        );
                    });
            }
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([820.0, 540.0])
            .with_title("NEX Studio"),
        ..Default::default()
    };
    eframe::run_native(
        "NEX Studio",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(Studio::new()))
        }),
    )
}
