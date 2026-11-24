//! NEX Studio (egui) вЂ” a PyCharm-style desktop IDE for the NEX language.
//!
//! Rust + egui (GPU) + egui_code_editor (syntax-highlighted editable code).
//! Compiles NEX in process (nex_driver) and runs the result.

use eframe::egui;
use egui::Color32;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

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

struct Studio {
    code: String,
    output: String,
    status: String,
    files: Vec<(String, String)>,
    active: usize,
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
                "libs/examples/demo.nex".to_string(),
                include_str!("../../../libs/examples/demo.nex").to_string(),
            ),
        ];
        Self {
            code: files[0].1.clone(),
            output: String::new(),
            status: "ready".to_string(),
            files,
            active: 0,
        }
    }

    fn run(&mut self) {
        self.status = "compilingвЂ¦".to_string();
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
                self.status = format!("ok (exit {})", run.status.code().unwrap_or(0));
            }
            Err(error) => {
                out.push_str(&format!("\nfailed to run executable: {error}\n"));
                self.status = "run error".to_string();
            }
        }
        self.output = out;
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut s = (*ctx.global_style()).clone();
    s.visuals = egui::Visuals::dark();

    let bg = Color32::from_rgb(43, 43, 43);
    let panel = Color32::from_rgb(49, 51, 53);
    let toolbar = Color32::from_rgb(60, 63, 65);
    let border = Color32::from_rgb(72, 72, 72);
    let text = Color32::from_rgb(169, 183, 198);
    let accent = Color32::from_rgb(54, 88, 128);

    s.visuals.panel_fill = bg;
    s.visuals.window_fill = bg;
    s.visuals.faint_bg_color = panel;
    s.visuals.extreme_bg_color = Color32::from_rgb(30, 30, 30);
    s.visuals.widgets.noninteractive.bg_fill = bg;
    s.visuals.widgets.inactive.bg_fill = toolbar;
    s.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0f32, border);
    s.visuals.widgets.hovered.bg_fill = Color32::from_rgb(75, 78, 80);
    s.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0f32, Color32::from_rgb(96, 99, 102));
    s.visuals.widgets.active.bg_fill = accent;
    s.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0f32, accent);
    s.visuals.selection.bg_fill = accent;
    s.visuals.selection.stroke = egui::Stroke::new(1.0f32, Color32::from_rgb(88, 166, 255));
    s.visuals.window_stroke = egui::Stroke::new(1.0f32, border);

    s.visuals.override_text_color = Some(text);
    s.spacing.item_spacing = egui::vec2(9.0, 7.0);
    s.spacing.button_padding = egui::vec2(12.0, 5.0);
    s.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
    );
    ctx.set_global_style(s);
}

impl eframe::App for Studio {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {}

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Menu bar.
        egui::Panel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New").clicked() {
                        self.code.clear();
                        self.output.clear();
                        ui.close_menu();
                    }
                    if ui.button("Load example").clicked() {
                        self.code = EXAMPLE.to_string();
                        self.files[self.active].1 = self.code.clone();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Run", |ui| {
                    if ui.button("Run").clicked() {
                        self.run();
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Clear console").clicked() {
                        self.output.clear();
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(Color32::from_rgb(133, 133, 133), &self.status);
                });
            });
        });

        // Status bar.
        egui::Panel::bottom("status_bar").show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(0, 122, 204))
                .inner_margin(egui::Margin::symmetric(10, 3))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::WHITE, "main");
                        ui.separator();
                        ui.colored_label(Color32::WHITE, "UTF-8");
                        ui.separator();
                        ui.colored_label(Color32::WHITE, "NEX");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.colored_label(Color32::WHITE, &self.status);
                        });
                    });
                });
        });

        // Sidebar.
        egui::Panel::left("files")
            .resizable(true)
            .default_size(200.0)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(49, 51, 53))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.colored_label(Color32::from_rgb(133, 133, 133), "PROJECT");
                        ui.add_space(3.0);
                        for (index, (name, _)) in self.files.iter().enumerate() {
                            let selected = self.active == index;
                            if ui.selectable_label(selected, format!("  {name}")).clicked() {
                                self.active = index;
                                self.code = self.files[index].1.clone();
                            }
                        }
                    });
            });

        // Central: tab + editor (fills) + console.
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(60, 63, 65))
                .inner_margin(egui::Margin::symmetric(14, 5))
                .show(ui, |ui| {
                    let name = self.files[self.active].0.clone();
                    ui.colored_label(
                        Color32::from_rgb(169, 183, 198),
                        egui::RichText::new(format!("в—Џ {name}")).monospace(),
                    );
                });
            ui.add_space(4.0);

            let console_h = 170.0;
            let avail = ui.available_height();
            let rows = (((avail - console_h - 16.0) / 20.0).floor() as usize).max(6);
            let mut editor = CodeEditor::default()
                .id_source("main")
                .with_theme(ColorTheme::SONOKAI)
                .with_fontsize(14.0)
                .with_rows(rows)
                .with_numlines(true)
                .with_syntax(nex_syntax());
            let response = editor.show(ui, &mut self.code);
            if response.response.changed() {
                self.files[self.active].1 = self.code.clone();
            }

            ui.add_space(5.0);
            ui.separator();
            ui.colored_label(Color32::from_rgb(133, 133, 133), "CONSOLE");
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(console_h)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.output)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(7)
                            .interactive(false)
                            .text_color(Color32::from_rgb(169, 183, 198)),
                    );
                });
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([760.0, 480.0])
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

