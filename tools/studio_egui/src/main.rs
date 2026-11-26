//! NEX Studio (egui) — a PyCharm-style desktop IDE for the NEX language.
//!
//! Rust + egui (GPU) + egui_code_editor (syntax-highlighted editable code).
//! Compiles NEX in process (nex_driver) and runs the result.

#![allow(deprecated)]

use eframe::egui;
use egui::Color32;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

const C_TEXT: Color32 = Color32::from_rgb(225, 232, 239);
const C_MUTED: Color32 = Color32::from_rgb(137, 151, 167);
const C_BG: Color32 = Color32::from_rgb(24, 28, 35);
const C_PANEL: Color32 = Color32::from_rgb(31, 36, 45);
const C_TOOL: Color32 = Color32::from_rgb(39, 45, 56);
const C_GREEN: Color32 = Color32::from_rgb(45, 177, 132);
const C_BLUE: Color32 = Color32::from_rgb(38, 46, 57);
const C_ACCENT: Color32 = Color32::from_rgb(48, 105, 115);

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
    files: Vec<ProjectFile>,
    open_files: Vec<usize>,
    active: usize,
    project_root: Option<PathBuf>,
    project_path_input: String,
    create_path_input: String,
    show_project_dialog: bool,
    show_new_file_dialog: bool,
    run_sender: Sender<RunCompletion>,
    run_receiver: Receiver<RunCompletion>,
    is_running: bool,
    activity: Activity,
    find: String,
    font_size: f32,
    compile_time: Option<f64>,
    cursor_line: usize,
    cursor_col: usize,
    show_console: bool,
}

struct ProjectFile {
    path: String,
    code: String,
    saved: String,
}

struct RunCompletion {
    output: String,
    status: String,
    elapsed: f64,
}

impl Studio {
    fn new() -> Self {
        let (run_sender, run_receiver) = mpsc::channel();
        let current = std::env::current_dir().unwrap_or_default();
        let startup_root = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .or_else(|| is_nex_project(&current).then_some(current));
        let (project_root, files, code, status, project_path_input, active) =
            if let Some(root) = startup_root {
                match load_project(&root) {
                    Ok(files) => {
                        let root = root.canonicalize().unwrap_or(root);
                        let active = project_file_index(&root, &files).unwrap_or(0);
                        let code = files
                            .get(active)
                            .map(|file| file.code.clone())
                            .unwrap_or_default();
                        let input = root.display().to_string();
                        (
                            Some(root),
                            files,
                            code,
                            "Project opened".to_string(),
                            input,
                            active,
                        )
                    }
                    Err(error) => (
                        None,
                        Vec::new(),
                        String::new(),
                        error.to_string(),
                        String::new(),
                        0,
                    ),
                }
            } else {
                (
                    None,
                    Vec::new(),
                    String::new(),
                    "Open a NEX project to get started".to_string(),
                    String::new(),
                    0,
                )
            };
        let open_files = if status == "Project opened" && !files.is_empty() {
            vec![active]
        } else {
            Vec::new()
        };
        Self {
            code,
            output: "Welcome to NEX Studio. Open a project folder to begin.\n".to_string(),
            status,
            files,
            open_files,
            active,
            project_root,
            project_path_input,
            create_path_input: "src/new_file.nex".to_string(),
            show_project_dialog: false,
            show_new_file_dialog: false,
            run_sender,
            run_receiver,
            is_running: false,
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
        if self.is_running {
            return;
        }
        self.show_console = true;
        self.persist_current();
        let Some(root) = self.project_root.clone() else {
            self.output = "Open a NEX project before building or running it.\n".to_string();
            self.status = "no project".to_string();
            return;
        };
        if let Err(error) = self.save_all() {
            self.output = format!("Could not save project before running: {error}\n");
            self.status = "save error".to_string();
            return;
        }
        let Some(entry) = project_entry(&root) else {
            self.output = "Could not find the project entry file. Set [build] entry in nex.toml.\n"
                .to_string();
            self.status = "no entry file".to_string();
            return;
        };
        self.status = "Building and running…".to_string();
        self.output = format!(
            "Project: {}\nEntry: {}\n\nBuilding…\n",
            root.display(),
            entry.display()
        );
        self.is_running = true;
        let sender = self.run_sender.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let result = run_project(&root, &entry);
            let elapsed = started.elapsed().as_secs_f64();
            let completion = match result {
                Ok((output, status)) => RunCompletion {
                    output,
                    status,
                    elapsed,
                },
                Err(error) => RunCompletion {
                    output: format!("{error}\n"),
                    status: "build error".to_string(),
                    elapsed,
                },
            };
            let _ = sender.send(completion);
        });
    }

    fn switch_to(&mut self, index: usize) {
        self.persist_current();
        if !self.open_files.contains(&index) {
            self.open_files.push(index);
        }
        self.active = index;
        self.code = self.files[index].code.clone();
        self.cursor_line = 1;
        self.cursor_col = 1;
    }

    fn persist_current(&mut self) {
        if let Some(file) = self.files.get_mut(self.active) {
            file.code.clone_from(&self.code);
        }
    }

    fn save_all(&mut self) -> std::io::Result<()> {
        self.persist_current();
        let Some(root) = self.project_root.as_ref() else {
            return Err(std::io::Error::other("no project folder is open"));
        };
        for file in &mut self.files {
            if file.code != file.saved {
                let destination = root.join(&file.path);
                if let Some(parent) = destination.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&destination, &file.code)?;
                file.saved.clone_from(&file.code);
            }
        }
        Ok(())
    }

    fn open_project(&mut self) {
        let root = PathBuf::from(self.project_path_input.trim());
        if let Err(error) = self.save_all() {
            if self.project_root.is_some() {
                self.status = format!("Save current project first: {error}");
                return;
            }
        }
        match load_project(&root) {
            Ok(files) => {
                let root = root.canonicalize().unwrap_or(root);
                self.active = project_file_index(&root, &files).unwrap_or(0);
                self.code = files
                    .get(self.active)
                    .map(|file| file.code.clone())
                    .unwrap_or_default();
                self.files = files;
                self.open_files = vec![self.active];
                self.project_root = Some(root.clone());
                self.project_path_input = root.display().to_string();
                self.output = format!(
                    "Opened {} source files from {}\n",
                    self.files.len(),
                    root.display()
                );
                self.status = "Project opened".to_string();
                self.show_project_dialog = false;
            }
            Err(error) => {
                self.output = format!("Could not open project: {error}\n");
                self.status = "open failed".to_string();
            }
        }
    }

    fn create_file(&mut self) {
        let Some(root) = self.project_root.as_ref() else {
            self.status = "Open a project first".to_string();
            return;
        };
        let relative = Path::new(self.create_path_input.trim());
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
            || relative.extension().is_none_or(|ext| ext != "nex")
        {
            self.status = "Choose a relative .nex file path".to_string();
            return;
        }
        let destination = root.join(relative);
        if destination.exists() {
            self.status = "That file already exists".to_string();
            return;
        }
        if let Some(parent) = destination.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                self.status = format!("Could not create folder: {error}");
                return;
            }
        }
        if let Err(error) = std::fs::write(&destination, "") {
            self.status = format!("Could not create file: {error}");
            return;
        }
        let relative = relative.to_string_lossy().replace('\\', "/");
        self.files.push(ProjectFile {
            path: relative.clone(),
            code: String::new(),
            saved: String::new(),
        });
        self.files.sort_by_key(|file| file.path.to_lowercase());
        if let Some(index) = self.files.iter().position(|file| file.path == relative) {
            self.switch_to(index);
        }
        self.show_new_file_dialog = false;
    }
}

fn is_nex_project(path: &Path) -> bool {
    path.join("nex.toml").is_file()
        || path.join("src").join("main.nex").is_file()
        || path.join("main.nex").is_file()
}

fn collect_nex_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<ProjectFile>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        if file_type.is_dir() {
            if matches!(
                name.to_str(),
                Some(".git" | "target" | "build" | "deps" | ".idea" | ".vscode")
            ) {
                continue;
            }
            collect_nex_files(root, &entry.path(), files)?;
        } else if file_type.is_file() && entry.path().extension().is_some_and(|ext| ext == "nex") {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let code = std::fs::read_to_string(&path)?;
            files.push(ProjectFile {
                path: relative,
                saved: code.clone(),
                code,
            });
        }
    }
    Ok(())
}

fn load_project(path: &Path) -> std::io::Result<Vec<ProjectFile>> {
    let root = path.canonicalize()?;
    if !root.is_dir() {
        return Err(std::io::Error::other("choose a project folder"));
    }
    let mut files = Vec::new();
    collect_nex_files(&root, &root, &mut files)?;
    files.sort_by_key(|file| file.path.to_lowercase());
    if files.is_empty() {
        return Err(std::io::Error::other("this folder contains no .nex files"));
    }
    Ok(files)
}

fn project_entry(root: &Path) -> Option<PathBuf> {
    let manifest = root.join("nex.toml");
    if let Ok(contents) = std::fs::read_to_string(manifest) {
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "entry" {
                let value = value
                    .split('#')
                    .next()
                    .unwrap_or(value)
                    .trim()
                    .trim_matches(['"', '\'']);
                let candidate = root.join(value);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    [root.join("src/main.nex"), root.join("main.nex")]
        .into_iter()
        .find(|path| path.is_file())
}

fn project_file_index(root: &Path, files: &[ProjectFile]) -> Option<usize> {
    let entry = project_entry(root)?;
    let relative = entry
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    files.iter().position(|file| file.path == relative)
}

fn run_project(root: &Path, entry: &Path) -> Result<(String, String), String> {
    let options = nex_driver::CompileOptions {
        build_dir: Some(root.join("build")),
        ..Default::default()
    };
    let result = nex_driver::compile_file(entry, options)
        .map_err(|error| format!("Build failed: {error}"))?;
    let mut output = String::new();
    for diagnostic in &result.diagnostics {
        if let Some(span) = diagnostic.span {
            let (line, column) = result.source.line_col(span.start);
            output.push_str(&format!(
                "{} [{}] {}:{line}:{column}: {}\n",
                diagnostic.severity.label(),
                diagnostic.code,
                result.source.name(),
                diagnostic.message
            ));
        } else {
            output.push_str(&format!(
                "{} [{}]: {}\n",
                diagnostic.severity.label(),
                diagnostic.code,
                diagnostic.message
            ));
        }
    }
    if result.has_errors() {
        return Ok((output, "build failed".to_string()));
    }
    let Some(exe) = result.artifacts.and_then(|artifacts| artifacts.executable) else {
        return Err("The compiler produced no executable.".to_string());
    };
    let run = std::process::Command::new(&exe)
        .current_dir(root)
        .output()
        .map_err(|error| format!("Could not start program: {error}"))?;
    output.push_str(&String::from_utf8_lossy(&run.stdout));
    let stderr = String::from_utf8_lossy(&run.stderr);
    if !stderr.is_empty() {
        output.push_str(&stderr);
    }
    let code = run.status.code().unwrap_or(-1);
    Ok((
        output,
        if code == 0 {
            "Finished successfully".to_string()
        } else {
            format!("Exited with code {code}")
        },
    ))
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
        while let Ok(completion) = self.run_receiver.try_recv() {
            self.output = completion.output;
            self.status = completion.status;
            self.compile_time = Some(completion.elapsed);
            self.is_running = false;
        }
        if self.is_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Keyboard shortcuts.
        if ctx
            .input(|i| i.key_pressed(egui::Key::Enter) && (i.modifiers.ctrl || i.modifiers.command))
        {
            self.run();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::S) && (i.modifiers.ctrl || i.modifiers.command)) {
            match self.save_all() {
                Ok(()) => self.status = "All files saved".to_string(),
                Err(error) => self.status = format!("Save failed: {error}"),
            }
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
                    if ui.button("Open project folder…").clicked() {
                        self.show_project_dialog = true;
                        ui.close_menu();
                    }
                    if ui.button("New file").clicked() {
                        if self.project_root.is_some() {
                            self.show_new_file_dialog = true;
                        } else {
                            self.status = "Open a project before creating files".to_string();
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save all  (Ctrl+S)").clicked() {
                        match self.save_all() {
                            Ok(()) => self.status = "All files saved".to_string(),
                            Err(error) => self.status = format!("Save failed: {error}"),
                        }
                        ui.close_menu();
                    }
                });
                ui.menu_button("Run", |ui| {
                    if ui
                        .add_enabled(
                            !self.is_running,
                            egui::Button::new("Build & Run  (Ctrl+Enter)"),
                        )
                        .clicked()
                    {
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
                        let project_name = self
                            .project_root
                            .as_ref()
                            .and_then(|root| root.file_name())
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| "No project".to_string());
                        ui.colored_label(Color32::WHITE, project_name);
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
                            ui.horizontal(|ui| {
                                ui.colored_label(C_MUTED, "PROJECT");
                                if ui
                                    .small_button("＋")
                                    .on_hover_text("Create a new NEX file")
                                    .clicked()
                                    && self.project_root.is_some()
                                {
                                    self.show_new_file_dialog = true;
                                }
                                if ui
                                    .small_button("↻")
                                    .on_hover_text("Refresh project files")
                                    .clicked()
                                {
                                    if let Some(root) = self.project_root.clone() {
                                        if let Err(error) = self.save_all() {
                                            self.status = format!("Save before refresh: {error}");
                                            return;
                                        }
                                        match load_project(&root) {
                                            Ok(files) => {
                                                let active_path = self
                                                    .files
                                                    .get(self.active)
                                                    .map(|f| f.path.clone());
                                                self.files = files;
                                                self.open_files.clear();
                                                if let Some(path) = active_path {
                                                    if let Some(index) = self
                                                        .files
                                                        .iter()
                                                        .position(|file| file.path == path)
                                                    {
                                                        self.active = index;
                                                        self.open_files.push(index);
                                                        self.code = self.files[index].code.clone();
                                                    }
                                                }
                                                if self.open_files.is_empty()
                                                    && !self.files.is_empty()
                                                {
                                                    self.active = 0;
                                                    self.open_files.push(0);
                                                    self.code = self.files[0].code.clone();
                                                }
                                            }
                                            Err(error) => {
                                                self.status = format!("Refresh failed: {error}")
                                            }
                                        }
                                    }
                                }
                            });
                            ui.add_space(4.0);
                            let project_name = self
                                .project_root
                                .as_ref()
                                .and_then(|p| p.file_name())
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Open a folder".to_string());
                            ui.colored_label(C_TEXT, format!("▾  {project_name}"));
                            let file_names: Vec<String> =
                                self.files.iter().map(|file| file.path.clone()).collect();
                            for (index, name) in file_names.iter().enumerate() {
                                let selected = self.active == index;
                                let indent = 4.0 + name.matches('/').count() as f32 * 12.0;
                                let display_name = name.rsplit('/').next().unwrap_or(name);
                                let dirty = self.files[index].code != self.files[index].saved;
                                ui.horizontal(|ui| {
                                    ui.add_space(indent);
                                    let text = egui::RichText::new(format!(
                                        "{} {display_name}",
                                        if dirty { "●" } else { "◇" }
                                    ))
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
                    .add_enabled(
                        !self.is_running && self.project_root.is_some(),
                        egui::Button::new(if self.is_running {
                            "◌ Building"
                        } else {
                            "▶ Run"
                        })
                        .fill(C_GREEN)
                        .min_size(egui::vec2(92.0, 28.0)),
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
                    if ui
                        .button("Save")
                        .on_hover_text("Save all files (Ctrl+S)")
                        .clicked()
                    {
                        match self.save_all() {
                            Ok(()) => self.status = "All files saved".to_string(),
                            Err(error) => self.status = format!("Save failed: {error}"),
                        }
                    }
                    if ui.button("Open project").clicked() {
                        self.show_project_dialog = true;
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
                        let open_files = self.open_files.clone();
                        for index in open_files {
                            let Some(file) = self.files.get(index) else {
                                continue;
                            };
                            let name = file.path.clone();
                            let dirty = file.code != file.saved;
                            let active = self.active == index;
                            egui::Frame::new()
                                .fill(if active { C_BG } else { C_TOOL })
                                .inner_margin(egui::Margin::symmetric(10, 4))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let display_name = name.rsplit('/').next().unwrap_or(&name);
                                        let text = egui::RichText::new(format!(
                                            "{} {display_name}",
                                            if dirty { "●" } else { "◇" }
                                        ))
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

            if self.project_root.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading(
                            egui::RichText::new("NEX Studio")
                                .size(30.0)
                                .color(Color32::WHITE),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("A native workspace for building with NEX")
                                .color(C_MUTED),
                        );
                        ui.add_space(20.0);
                        if ui
                            .add(
                                egui::Button::new("Open project folder")
                                    .fill(C_ACCENT)
                                    .min_size(egui::vec2(190.0, 38.0)),
                            )
                            .clicked()
                        {
                            self.show_project_dialog = true;
                        }
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(
                                "Choose a folder containing nex.toml or .nex source files",
                            )
                            .small()
                            .color(C_MUTED),
                        );
                    });
                });
            } else {
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
                    self.persist_current();
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
                        ui.colored_label(C_MUTED, "OUTPUT");
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
            }
        });

        if self.show_project_dialog {
            egui::Window::new("Open NEX project")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Choose a folder containing nex.toml or .nex files.");
                    ui.add_space(6.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_path_input)
                            .desired_width(420.0)
                            .hint_text("D:\\Projects\\MyNexApp"),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Open folder").clicked() {
                            self.open_project();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_project_dialog = false;
                        }
                    });
                });
        }
        if self.show_new_file_dialog {
            egui::Window::new("Create NEX file")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Path inside the project");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.create_path_input)
                            .desired_width(320.0),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            self.create_file();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_new_file_dialog = false;
                        }
                    });
                });
        }
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
