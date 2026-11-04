use std::fs;
use std::path::PathBuf;

fn config_path(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/kodrop").join(name))
}

pub fn load_setting(name: &str) -> String {
    config_path(name)
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

pub fn save_setting(name: &str, value: &str) {
    let Some(path) = config_path(name) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, value);
}
