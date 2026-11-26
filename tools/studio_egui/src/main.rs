//! NEX Studio (egui) — a PyCharm-style desktop IDE for the NEX language.
//!
//! Rust + egui (GPU) + egui_code_editor (syntax-highlighted editable code).
//! Compiles NEX in process (nex_driver) and runs the result.

#![allow(deprecated)]

use eframe::egui;
use egui::Color32;
use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use std::collections::BTreeSet;
use std::ffi::c_void;
use std::fs;
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
    recent_projects: Vec<PathBuf>,
    project_path_input: String,
    create_path_input: String,
    create_folder_path_input: String,
    show_project_dialog: bool,
    show_new_file_dialog: bool,
    show_new_folder_dialog: bool,
    check_sender: Sender<CheckCompletion>,
    check_receiver: Receiver<CheckCompletion>,
    check_due: Option<Instant>,
    check_revision: u64,
    diagnostics: Vec<EditorDiagnostic>,
    jump_to_line: Option<usize>,
    show_problems: bool,
    show_exit_prompt: bool,
    pending_close_tab: Option<usize>,
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

struct CheckCompletion {
    revision: u64,
    diagnostics: Vec<EditorDiagnostic>,
}

#[derive(Clone)]
struct EditorDiagnostic {
    file: String,
    line: Option<usize>,
    column: Option<usize>,
    severity: String,
    code: String,
    message: String,
}

fn recent_projects_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| home.join("AppData").join("Roaming"))
        })
        .map(|config| {
            config
                .join("NEX")
                .join("Studio")
                .join("recent-projects.txt")
        })
}

fn load_recent_projects() -> Vec<PathBuf> {
    let Some(path) = recent_projects_path() else {
        return Vec::new();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut recent = Vec::new();
    for line in contents.lines() {
        let path = PathBuf::from(line.trim());
        if path.is_dir() && !recent.contains(&path) {
            recent.push(path);
            if recent.len() == 10 {
                break;
            }
        }
    }
    recent
}

fn remember_project(path: &Path, recent: &mut Vec<PathBuf>) -> std::io::Result<()> {
    recent.retain(|item| item != path);
    recent.insert(0, path.to_path_buf());
    recent.truncate(10);
    if let Some(file) = recent_projects_path() {
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = recent
            .iter()
            .map(|item| item.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(file, contents)?;
    }
    Ok(())
}

#[cfg(windows)]
#[repr(C)]
struct BrowseInfoW {
    hwnd_owner: isize,
    pidl_root: *mut c_void,
    display_name: *mut u16,
    title: *const u16,
    flags: u32,
    callback: Option<unsafe extern "system" fn(isize, u32, isize, isize) -> i32>,
    l_param: isize,
    image: i32,
}

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHBrowseForFolderW(info: *const BrowseInfoW) -> *mut c_void;
    fn SHGetPathFromIDListW(item: *const c_void, path: *mut u16) -> i32;
}

#[cfg(windows)]
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoTaskMemFree(memory: *mut c_void);
}

#[cfg(windows)]
fn pick_project_folder() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;

    let mut display_name = vec![0u16; 260];
    let title: Vec<u16> = "Select a NEX project folder\0".encode_utf16().collect();
    let info = BrowseInfoW {
        hwnd_owner: 0,
        pidl_root: std::ptr::null_mut(),
        display_name: display_name.as_mut_ptr(),
        title: title.as_ptr(),
        flags: 0x0001 | 0x0010 | 0x0040,
        callback: None,
        l_param: 0,
        image: 0,
    };
    let item = unsafe { SHBrowseForFolderW(&info) };
    if item.is_null() {
        return None;
    }
    let mut path = vec![0u16; 32_768];
    let success = unsafe { SHGetPathFromIDListW(item, path.as_mut_ptr()) } != 0;
    unsafe { CoTaskMemFree(item) };
    if !success {
        return None;
    }
    let length = path.iter().position(|unit| *unit == 0)?;
    Some(PathBuf::from(std::ffi::OsString::from_wide(
        &path[..length],
    )))
}

#[cfg(not(windows))]
fn pick_project_folder() -> Option<PathBuf> {
    None
}

impl Studio {
    fn new() -> Self {
        let (run_sender, run_receiver) = mpsc::channel();
        let (check_sender, check_receiver) = mpsc::channel();
        let recent_projects = load_recent_projects();
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
        let initial_check_due = project_root
            .as_ref()
            .map(|_| Instant::now() + std::time::Duration::from_millis(100));
        Self {
            code,
            output: "Welcome to NEX Studio. Open a project folder to begin.\n".to_string(),
            status,
            files,
            open_files,
            active,
            project_root,
            recent_projects,
            project_path_input,
            create_path_input: "src/new_file.nex".to_string(),
            create_folder_path_input: "src/new_folder".to_string(),
            show_project_dialog: false,
            show_new_file_dialog: false,
            show_new_folder_dialog: false,
            check_sender,
            check_receiver,
            check_due: initial_check_due,
            check_revision: 0,
            diagnostics: Vec::new(),
            jump_to_line: None,
            show_problems: false,
            show_exit_prompt: false,
            pending_close_tab: None,
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
        self.diagnostics.clear();
        self.queue_check();
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
                if let Err(error) = remember_project(&root, &mut self.recent_projects) {
                    self.status = format!("Project opened; could not update recent list: {error}");
                }
                self.queue_check();
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

    fn open_project_folder(&mut self, root: PathBuf) {
        self.project_path_input = root.display().to_string();
        self.open_project();
    }

    fn refresh_project(&mut self) {
        let Some(root) = self.project_root.clone() else {
            return;
        };
        if let Err(error) = self.save_all() {
            self.status = format!("Save before refresh: {error}");
            return;
        }
        let active_path = self.files.get(self.active).map(|file| file.path.clone());
        let open_paths: Vec<String> = self
            .open_files
            .iter()
            .filter_map(|&index| self.files.get(index).map(|file| file.path.clone()))
            .collect();
        match load_project(&root) {
            Ok(files) => {
                self.files = files;
                self.open_files = open_paths
                    .iter()
                    .filter_map(|path| self.files.iter().position(|file| &file.path == path))
                    .collect();
                self.active = active_path
                    .as_ref()
                    .and_then(|path| self.files.iter().position(|file| &file.path == path))
                    .or_else(|| project_file_index(&root, &self.files))
                    .unwrap_or(0);
                if !self.open_files.contains(&self.active) {
                    self.open_files.push(self.active);
                }
                self.code = self.files[self.active].code.clone();
                self.status = "Project refreshed".to_string();
            }
            Err(error) => self.status = format!("Refresh failed: {error}"),
        }
    }

    fn create_file(&mut self) {
        let Some(root) = self.project_root.as_ref() else {
            self.status = "Open a project first".to_string();
            return;
        };
        let relative = Path::new(self.create_path_input.trim());
        if !safe_relative_path(relative) || relative.extension().is_none_or(|ext| ext != "nex") {
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

    fn create_folder(&mut self) {
        let Some(root) = self.project_root.as_ref() else {
            self.status = "Open a project first".to_string();
            return;
        };
        let relative = Path::new(self.create_folder_path_input.trim());
        if !safe_relative_path(relative) {
            self.status = "Choose a folder path inside the project".to_string();
            return;
        }
        let destination = root.join(relative);
        match fs::create_dir_all(&destination) {
            Ok(()) => self.show_new_folder_dialog = false,
            Err(error) => self.status = format!("Could not create folder: {error}"),
        }
    }

    fn queue_check(&mut self) {
        self.check_revision = self.check_revision.wrapping_add(1);
        self.check_due = Some(Instant::now() + std::time::Duration::from_millis(350));
    }

    fn start_due_check(&mut self) {
        let Some(due) = self.check_due else {
            return;
        };
        if Instant::now() < due {
            return;
        }
        self.check_due = None;
        self.persist_current();
        let (Some(root), Some(file)) = (
            self.project_root.clone(),
            self.files.get(self.active).map(|file| file.path.clone()),
        ) else {
            self.diagnostics.clear();
            return;
        };
        let Some(source) = self.files.get(self.active).map(|file| file.code.clone()) else {
            return;
        };
        let revision = self.check_revision;
        let sender = self.check_sender.clone();
        self.status = "Checking source…".to_string();
        std::thread::spawn(move || {
            let diagnostics = check_source(&root, &file, source);
            let _ = sender.send(CheckCompletion {
                revision,
                diagnostics,
            });
        });
    }

    fn has_unsaved_changes(&self) -> bool {
        self.files.iter().any(|file| file.code != file.saved)
    }

    fn request_close_tab(&mut self, index: usize) {
        if self
            .files
            .get(index)
            .is_some_and(|file| file.code != file.saved)
        {
            self.pending_close_tab = Some(index);
        } else {
            self.close_tab(index, false);
        }
    }

    fn close_tab(&mut self, index: usize, save: bool) {
        if save {
            if let Err(error) = self.save_all() {
                self.status = format!("Could not save before closing tab: {error}");
                return;
            }
        } else if let Some(file) = self.files.get_mut(index) {
            file.code.clone_from(&file.saved);
            if self.active == index {
                self.code.clone_from(&file.saved);
            }
        }
        self.open_files.retain(|&open| open != index);
        if self.active == index {
            self.active = self.open_files.last().copied().unwrap_or(0);
            self.code = self
                .files
                .get(self.active)
                .map(|file| file.code.clone())
                .unwrap_or_default();
            self.cursor_line = 1;
            self.cursor_col = 1;
            if !self.open_files.is_empty() {
                self.queue_check();
            }
        }
        self.pending_close_tab = None;
    }

    fn save_and_exit(&mut self, ctx: &egui::Context) {
        match self.save_all() {
            Ok(()) => {
                self.show_exit_prompt = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(error) => self.status = format!("Could not save before exit: {error}"),
        }
    }

    fn discard_and_exit(&mut self, ctx: &egui::Context) {
        self.persist_current();
        for file in &mut self.files {
            file.saved.clone_from(&file.code);
        }
        self.show_exit_prompt = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn show_project_tree(&mut self, ui: &mut egui::Ui, prefix: &str) {
        let Some(root) = self.project_root.clone() else {
            return;
        };
        for node in project_nodes(&root, &self.files, prefix) {
            match node {
                ProjectNode::Directory { name, path } => {
                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!("▸  {name}")).color(C_TEXT),
                    )
                    .id_salt(format!("project-folder:{path}"))
                    .default_open(true)
                    .show(ui, |ui| self.show_project_tree(ui, &path));
                }
                ProjectNode::File { index, name } => {
                    let selected = self.active == index;
                    let dirty = self.files[index].code != self.files[index].saved;
                    let label = format!("{}  {name}", if dirty { "●" } else { "◇" });
                    if ui
                        .selectable_label(
                            selected,
                            egui::RichText::new(label)
                                .monospace()
                                .size(13.0)
                                .color(if selected { Color32::WHITE } else { C_TEXT }),
                        )
                        .clicked()
                    {
                        self.switch_to(index);
                    }
                }
            }
        }
    }
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && !path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
}

enum ProjectNode {
    Directory { name: String, path: String },
    File { index: usize, name: String },
}

fn project_nodes(root: &Path, files: &[ProjectFile], prefix: &str) -> Vec<ProjectNode> {
    let mut directories = BTreeSet::new();
    let mut nodes = Vec::new();
    let prefix_with_separator = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };
    let directory = if prefix.is_empty() {
        root.to_path_buf()
    } else {
        root.join(prefix)
    };
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if !matches!(
                    name.as_str(),
                    ".git" | "target" | "build" | "deps" | ".idea" | ".vscode"
                ) {
                    directories.insert(name);
                }
            }
        }
    }
    for (index, file) in files.iter().enumerate() {
        let Some(relative) = file.path.strip_prefix(&prefix_with_separator) else {
            continue;
        };
        if let Some((directory, _)) = relative.split_once('/') {
            directories.insert(directory.to_string());
        } else {
            nodes.push(ProjectNode::File {
                index,
                name: relative.to_string(),
            });
        }
    }
    nodes.extend(directories.into_iter().map(|name| ProjectNode::Directory {
        path: if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        },
        name,
    }));
    nodes.sort_by_key(|node| match node {
        ProjectNode::Directory { name, .. } | ProjectNode::File { name, .. } => name.to_lowercase(),
    });
    nodes
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

fn parse_imported_location(
    message: &str,
    root: &Path,
) -> Option<(String, Option<usize>, Option<usize>, String)> {
    let end = message.find(".nex:")? + ".nex".len();
    let absolute = PathBuf::from(&message[..end]);
    let relative = absolute
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace('\\', "/");
    let mut parts = message[end + 1..].splitn(3, ':');
    let first = parts.next().unwrap_or_default();
    if let (Ok(line), Some(second)) = (first.parse::<usize>(), parts.next()) {
        if let Ok(column) = second.parse::<usize>() {
            let message = parts.next().unwrap_or_default().trim().to_string();
            return Some((relative, Some(line), Some(column), message));
        }
    }
    Some((relative, None, None, first.trim().to_string()))
}

fn check_source(root: &Path, relative: &str, source: String) -> Vec<EditorDiagnostic> {
    let path = root.join(relative);
    let options = nex_driver::CompileOptions {
        emit: nex_driver::Emit::Check,
        build_dir: Some(root.join("build")),
        ..Default::default()
    };
    let result = match nex_driver::compile_source(path.display().to_string(), source, options) {
        Ok(result) => result,
        Err(error) => {
            return vec![EditorDiagnostic {
                file: relative.to_string(),
                line: None,
                column: None,
                severity: "error".to_string(),
                code: "NXC".to_string(),
                message: error.to_string(),
            }];
        }
    };
    result
        .diagnostics
        .iter()
        .map(|diagnostic| {
            if let Some(span) = diagnostic.span {
                let (line, column) = result.source.line_col(span.start);
                EditorDiagnostic {
                    file: relative.to_string(),
                    line: Some(line),
                    column: Some(column),
                    severity: diagnostic.severity.label().to_string(),
                    code: diagnostic.code.clone(),
                    message: diagnostic.message.clone(),
                }
            } else if let Some((file, line, column, message)) =
                parse_imported_location(&diagnostic.message, root)
            {
                EditorDiagnostic {
                    file,
                    line,
                    column,
                    severity: diagnostic.severity.label().to_string(),
                    code: diagnostic.code.clone(),
                    message,
                }
            } else {
                EditorDiagnostic {
                    file: relative.to_string(),
                    line: None,
                    column: None,
                    severity: diagnostic.severity.label().to_string(),
                    code: diagnostic.code.clone(),
                    message: diagnostic.message.clone(),
                }
            }
        })
        .collect()
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
        while let Ok(completion) = self.check_receiver.try_recv() {
            if completion.revision == self.check_revision {
                self.diagnostics = completion.diagnostics;
                if !self.diagnostics.is_empty() {
                    self.show_problems = true;
                }
                self.status = if self.diagnostics.is_empty() {
                    "No source issues".to_string()
                } else {
                    format!("{} source issue(s)", self.diagnostics.len())
                };
            }
        }
        self.start_due_check();
        if let Some(due) = self.check_due {
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.has_unsaved_changes() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_exit_prompt = true;
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
        if ctx.input(|i| i.key_pressed(egui::Key::O) && (i.modifiers.ctrl || i.modifiers.command)) {
            self.show_project_dialog = true;
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
                            ui.colored_label(C_MUTED, "SEARCH IN PROJECT");
                            ui.add_space(4.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.find)
                                    .hint_text("Search all .nex files…")
                                    .desired_width(f32::INFINITY),
                            );
                            ui.add_space(6.0);
                            if self.find.is_empty() {
                                ui.colored_label(
                                    C_MUTED,
                                    "Results from every project source file appear here.",
                                );
                            } else {
                                let needle = self.find.to_lowercase();
                                let matches: Vec<(usize, usize, String, String)> = self
                                    .files
                                    .iter()
                                    .enumerate()
                                    .flat_map(|(index, file)| {
                                        file.code.lines().enumerate().filter_map({
                                            let needle = needle.clone();
                                            let path = file.path.clone();
                                            move |(line, text)| {
                                                text.to_lowercase().contains(&needle).then(|| {
                                                    (
                                                        index,
                                                        line + 1,
                                                        path.clone(),
                                                        text.trim().to_string(),
                                                    )
                                                })
                                            }
                                        })
                                    })
                                    .take(300)
                                    .collect();
                                if matches.is_empty() {
                                    ui.colored_label(C_MUTED, "No matches");
                                } else {
                                    ui.colored_label(C_MUTED, format!("{} matches", matches.len()));
                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        for (index, line, path, text) in matches {
                                            if ui
                                                .small_button(format!("{path}:{line}  {text}"))
                                                .clicked()
                                            {
                                                self.switch_to(index);
                                                self.jump_to_line = Some(line);
                                            }
                                        }
                                    });
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
                                    .small_button("📁+")
                                    .on_hover_text("Create a folder")
                                    .clicked()
                                    && self.project_root.is_some()
                                {
                                    self.show_new_folder_dialog = true;
                                }
                                if ui
                                    .small_button("↻")
                                    .on_hover_text("Refresh project files")
                                    .clicked()
                                {
                                    self.refresh_project();
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
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| self.show_project_tree(ui, ""));
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
                                        if ui.small_button("×").on_hover_text("Close tab").clicked()
                                        {
                                            self.request_close_tab(index);
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
                        if !self.recent_projects.is_empty() {
                            ui.add_space(28.0);
                            ui.heading(egui::RichText::new("Recent projects").size(17.0));
                            ui.add_space(5.0);
                            for project in self.recent_projects.clone() {
                                let name = project
                                    .file_name()
                                    .map(|part| part.to_string_lossy().to_string())
                                    .unwrap_or_else(|| project.display().to_string());
                                if ui
                                    .selectable_label(
                                        false,
                                        format!("{name}  ·  {}", project.display()),
                                    )
                                    .clicked()
                                {
                                    self.open_project_folder(project);
                                }
                            }
                        }
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
                    self.queue_check();
                }
                if let Some(line) = self.jump_to_line.take() {
                    let char_index = self
                        .code
                        .lines()
                        .take(line.saturating_sub(1))
                        .map(|text| text.chars().count() + 1)
                        .sum();
                    let id = response.response.id;
                    let mut state =
                        egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(char_index),
                        )));
                    state.store(ctx, id);
                    response.response.request_focus();
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
                let col = self.code[..offset]
                    .rsplit('\n')
                    .next()
                    .map(|line| line.chars().count() + 1)
                    .unwrap_or(1);
                self.cursor_line = line;
                self.cursor_col = col;

                if self.show_console {
                    ui.add_space(5.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!self.show_problems, "OUTPUT").clicked() {
                            self.show_problems = false;
                        }
                        if ui
                            .selectable_label(
                                self.show_problems,
                                format!("PROBLEMS ({})", self.diagnostics.len()),
                            )
                            .clicked()
                        {
                            self.show_problems = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.show_problems && ui.small_button("Clear").clicked() {
                                self.output.clear();
                            }
                        });
                    });
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(160.0)
                        .show(ui, |ui| {
                            if self.show_problems {
                                if self.diagnostics.is_empty() {
                                    ui.colored_label(
                                        C_MUTED,
                                        "No source issues in the active file.",
                                    );
                                }
                                for diagnostic in self.diagnostics.clone() {
                                    let location = diagnostic.line.map_or_else(
                                        || diagnostic.file.clone(),
                                        |line| {
                                            diagnostic.column.map_or_else(
                                                || format!("{}:{line}", diagnostic.file),
                                                |column| {
                                                    format!("{}:{line}:{column}", diagnostic.file)
                                                },
                                            )
                                        },
                                    );
                                    let label = format!(
                                        "{} [{}] {}: {}",
                                        diagnostic.severity.to_uppercase(),
                                        diagnostic.code,
                                        location,
                                        diagnostic.message
                                    );
                                    let color = if diagnostic.severity == "error" {
                                        Color32::from_rgb(255, 130, 130)
                                    } else {
                                        C_TEXT
                                    };
                                    if ui
                                        .selectable_label(
                                            false,
                                            egui::RichText::new(label).color(color),
                                        )
                                        .clicked()
                                    {
                                        if let Some(index) = self
                                            .files
                                            .iter()
                                            .position(|file| file.path == diagnostic.file)
                                        {
                                            self.switch_to(index);
                                            self.jump_to_line = diagnostic.line;
                                        }
                                    }
                                }
                            } else {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.output)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(6)
                                        .interactive(false)
                                        .text_color(C_TEXT),
                                );
                            }
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
                        if ui.button("Browse…").clicked() {
                            if let Some(path) = pick_project_folder() {
                                self.project_path_input = path.display().to_string();
                            }
                        }
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
        if self.show_new_folder_dialog {
            egui::Window::new("Create project folder")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Folder path inside the project");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.create_folder_path_input)
                            .desired_width(320.0),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Create folder").clicked() {
                            self.create_folder();
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_new_folder_dialog = false;
                        }
                    });
                });
        }
        if let Some(index) = self.pending_close_tab {
            let name = self
                .files
                .get(index)
                .map(|file| file.path.clone())
                .unwrap_or_default();
            egui::Window::new("Save changes?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(format!("Save changes to {name} before closing this tab?"));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            self.close_tab(index, true);
                        }
                        if ui.button("Discard").clicked() {
                            self.close_tab(index, false);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_close_tab = None;
                        }
                    });
                });
        }
        if self.show_exit_prompt {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Save your project changes before exiting?");
                    ui.horizontal(|ui| {
                        if ui.button("Save and exit").clicked() {
                            self.save_and_exit(ctx);
                        }
                        if ui.button("Discard and exit").clicked() {
                            self.discard_and_exit(ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_exit_prompt = false;
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
