use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub version: String,
    pub entry: PathBuf,
}

pub fn discover(start: &Path) -> Result<Project, String> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let root = find_root(start)?;
    load(&root)
}

pub fn find_root(start: &Path) -> Result<PathBuf, String> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("nex.toml").is_file() || dir.join("src").join("main.nex").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    Err("could not find a NEX project root (expected `nex.toml` or `src/main.nex`)".to_string())
}

pub fn load(root: &Path) -> Result<Project, String> {
    let config_path = root.join("nex.toml");
    if config_path.is_file() {
        let text = fs::read_to_string(&config_path).map_err(|error| error.to_string())?;
        return Ok(parse_config(&text, root));
    }

    let entry = root.join("src").join("main.nex");
    if !entry.is_file() {
        return Err(format!(
            "project `{}` has no `nex.toml` and no `src/main.nex`",
            root.display()
        ));
    }
    Ok(Project {
        root: root.to_path_buf(),
        name: root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("nex-project")
            .to_string(),
        version: "0.1.0".to_string(),
        entry,
    })
}

fn parse_config(text: &str, root: &Path) -> Project {
    let mut section = String::new();
    let mut name = None;
    let mut version = None;
    let mut entry = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line.to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        match (section.as_str(), key) {
            ("[package]", "name") => name = Some(value.to_string()),
            ("[package]", "version") => version = Some(value.to_string()),
            ("[build]", "entry") => entry = Some(value.to_string()),
            _ => {}
        }
    }

    let entry = root.join(entry.unwrap_or_else(|| "src/main.nex".to_string()));
    Project {
        root: root.to_path_buf(),
        name: name.unwrap_or_else(|| {
            root.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("nex-project")
                .to_string()
        }),
        version: version.unwrap_or_else(|| "0.1.0".to_string()),
        entry,
    }
}

pub fn create(root: &Path, name: &str) -> Result<(), String> {
    if root.exists() {
        return Err(format!("directory already exists: {}", root.display()));
    }
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("nex.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\ndescription = \"\"\n\n[build]\nentry = \"src/main.nex\"\n"
        ),
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        root.join("src").join("main.nex"),
        "fn main():\n    print(\"Hello from NEX!\")\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        root.join("README.md"),
        format!("# {name}\n\nA NEX project.\n"),
    )
    .map_err(|error| error.to_string())?;
    fs::write(root.join(".gitignore"), "build/\n*.exe\n*.o\n*.obj\n*.ll\n")
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn init(root: &Path) -> Result<Vec<String>, String> {
    let mut created = Vec::new();
    if !root.join("nex.toml").is_file() {
        let name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("nex-project");
        fs::create_dir_all(root).map_err(|error| error.to_string())?;
        fs::write(
            root.join("nex.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\ndescription = \"\"\n\n[build]\nentry = \"src/main.nex\"\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        created.push("nex.toml".to_string());
    }
    if !root.join("src").join("main.nex").is_file() {
        fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        fs::write(
            root.join("src").join("main.nex"),
            "fn main():\n    print(\"Hello from NEX!\")\n",
        )
        .map_err(|error| error.to_string())?;
        created.push("src/main.nex".to_string());
    }
    if !root.join(".gitignore").is_file() {
        fs::write(root.join(".gitignore"), "build/\n*.exe\n*.o\n*.obj\n*.ll\n")
            .map_err(|error| error.to_string())?;
        created.push(".gitignore".to_string());
    }
    Ok(created)
}

pub fn clean(root: &Path) -> Result<(), String> {
    let build = root.join("build");
    if build.exists() {
        fs::remove_dir_all(&build).map_err(|error| error.to_string())?;
    }
    Ok(())
}
