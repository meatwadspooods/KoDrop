use super::library::{DEVICE_SCRIPT, Device, parse_device};
use std::path::Path;
use std::process::Command;

const PORT: &str = "2222";
const DESTINATION: &str = "/mnt/us/books/";

const SSH_OPTIONS: &[&str] = &[
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=5",
    "-o",
    "StrictHostKeyChecking=accept-new",
    "-o",
    "LogLevel=ERROR",
];

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn remote_dir(folder: &str) -> String {
    if folder.is_empty() {
        DESTINATION.to_string()
    } else {
        format!("{DESTINATION}{folder}/")
    }
}

// Limits: the same filename overwrites and an interrupted copy can leave a partial file.
// Uploading to a temp name and renaming over ssh would fix both
pub fn send(path: &Path, host: &str, folder: &str) -> Result<(), String> {
    create_folder(host, folder)?;
    run(Command::new("/usr/bin/scp")
        .args(["-O", "-P", PORT])
        .args(SSH_OPTIONS)
        .arg(path)
        .arg(format!("root@{host}:{}", shell_quote(&remote_dir(folder)))))
}

pub fn create_folder(host: &str, folder: &str) -> Result<(), String> {
    run(Command::new("/usr/bin/ssh")
        .args(["-p", PORT])
        .args(SSH_OPTIONS)
        .arg(format!("root@{host}"))
        .arg(format!("mkdir -p {}", shell_quote(&remote_dir(folder)))))
}

fn run(command: &mut Command) -> Result<(), String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(error_message(&String::from_utf8_lossy(&output.stderr)))
}

pub fn fetch_device(host: &str) -> Result<Device, String> {
    let output = Command::new("/usr/bin/ssh")
        .args(["-p", PORT])
        .args(SSH_OPTIONS)
        .arg(format!("root@{host}"))
        .arg(DEVICE_SCRIPT)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(parse_device(&String::from_utf8_lossy(&output.stdout)));
    }
    Err(error_message(&String::from_utf8_lossy(&output.stderr)))
}

// scp ends with a generic "lost connection", the real cause is on the lines before it
fn error_message(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        "Connection failed".to_string()
    } else {
        lines.join(" - ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_failure_when_nothing_is_listening() {
        let error = send(Path::new("/etc/hosts"), "127.0.0.1", "").unwrap_err();
        assert!(error.contains("Connection refused"), "{error}");
    }

    #[test]
    fn quotes_remote_paths() {
        assert_eq!(shell_quote("/mnt/us/books/"), "'/mnt/us/books/'");
        assert_eq!(shell_quote("My Book's; rm -rf x"), "'My Book'\\''s; rm -rf x'");
        assert_eq!(remote_dir(""), "/mnt/us/books/");
        assert_eq!(remote_dir("Fiction/Classics"), "/mnt/us/books/Fiction/Classics/");
    }

    #[test]
    fn keeps_the_cause_ahead_of_lost_connection() {
        let stderr = "ssh: connect to host x port 2222: Connection refused\r\nlost connection\r\n";
        assert!(error_message(stderr).starts_with("ssh: connect to host"));
        assert_eq!(error_message(""), "Connection failed");
    }
}
