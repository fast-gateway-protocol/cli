//! System notification helpers.
//!
//! Provides cross-platform notification support for FGP alerts.

/// Escape a string for use in AppleScript string literals.
/// Handles backslashes, double quotes, and newlines which have special meaning in AppleScript.
#[cfg(target_os = "macos")]
fn escape_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', " ")
}

/// Send a system notification.
///
/// On macOS, uses osascript to display a native notification.
/// On other platforms, this is a no-op (could be extended with notify-rust).
pub fn notify(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        // Use AppleScript with proper escaping to prevent command injection
        let escaped_title = escape_applescript_string(title);
        let escaped_message = escape_applescript_string(message);

        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            escaped_message, escaped_title
        );

        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Could use notify-rust here for Linux/Windows support
        let _ = (title, message); // Silence unused warnings
    }
}

/// Send a notification with a sound.
#[allow(dead_code)]
pub fn notify_with_sound(title: &str, message: &str, sound: &str) {
    #[cfg(target_os = "macos")]
    {
        let escaped_title = escape_applescript_string(title);
        let escaped_message = escape_applescript_string(message);
        let escaped_sound = escape_applescript_string(sound);

        let script = format!(
            "display notification \"{}\" with title \"{}\" sound name \"{}\"",
            escaped_message, escaped_title, escaped_sound
        );

        let _ = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output();
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (title, message, sound);
    }
}
