//! View daemon logs in the terminal.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

/// Get the log file path for a service.
fn log_file_path(service: &str) -> PathBuf {
    let base = shellexpand::tilde("~/.fgp/services");
    PathBuf::from(base.as_ref())
        .join(service)
        .join("logs")
        .join("daemon.log")
}

/// Run the logs command.
pub fn run(service: &str, follow: bool, lines: usize) -> Result<()> {
    let log_path = log_file_path(service);

    if !log_path.exists() {
        bail!(
            "No logs found for service '{}' at {}",
            service,
            log_path.display()
        );
    }

    if follow {
        follow_logs(&log_path)?;
    } else {
        tail_logs(&log_path, lines)?;
    }

    Ok(())
}

/// Display the last N lines of the log file.
fn tail_logs(path: &PathBuf, lines: usize) -> Result<()> {
    let file = File::open(path).context("Failed to open log file")?;
    let reader = BufReader::new(file);

    // Read all lines and keep the last N
    let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
    let start = if all_lines.len() > lines {
        all_lines.len() - lines
    } else {
        0
    };

    for line in &all_lines[start..] {
        print_log_line(line);
    }

    Ok(())
}

/// Follow log output in real-time (like tail -f).
fn follow_logs(path: &PathBuf) -> Result<()> {
    let mut file = File::open(path).context("Failed to open log file")?;

    // Seek to end of file
    file.seek(SeekFrom::End(0))?;

    println!(
        "{} Following logs... (press Ctrl+C to exit)",
        "→".blue().bold()
    );

    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // No new data, wait and try again
                thread::sleep(Duration::from_millis(100));
            }
            Ok(_) => {
                // Got new data
                print_log_line(line.trim_end());
            }
            Err(e) => {
                eprintln!("{} Read error: {}", "✗".red().bold(), e);
                break;
            }
        }
    }

    Ok(())
}

/// Detect log level from a line using case-insensitive search.
/// Returns the detected level or None for INFO/unknown.
fn detect_log_level(line: &str) -> Option<&'static str> {
    // Use case-insensitive byte search to avoid allocation
    let bytes = line.as_bytes();

    // Check for common log level patterns in order of severity
    if contains_case_insensitive(bytes, b"ERROR") {
        Some("ERROR")
    } else if contains_case_insensitive(bytes, b"WARN") {
        Some("WARN")
    } else if contains_case_insensitive(bytes, b"DEBUG") {
        Some("DEBUG")
    } else if contains_case_insensitive(bytes, b"TRACE") {
        Some("TRACE")
    } else {
        None // INFO or other
    }
}

/// Case-insensitive byte search without allocation.
fn contains_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return needle.is_empty();
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Print a log line with color-coding by level.
fn print_log_line(line: &str) {
    let colored_line = match detect_log_level(line) {
        Some("ERROR") => line.red().to_string(),
        Some("WARN") => line.yellow().to_string(),
        Some("DEBUG") | Some("TRACE") => line.dimmed().to_string(),
        _ => line.to_string(), // INFO or other
    };

    println!("{}", colored_line);
}
