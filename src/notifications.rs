//! System notification helpers.
//!
//! Provides cross-platform notification support for FGP alerts.

/// Send a system notification.
///
/// On macOS, uses osascript to display a native notification.
/// On other platforms, this is a no-op (could be extended with notify-rust).
pub fn notify(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        // Escape quotes in title and message
        let title = title.replace('"', "\\\"");
        let message = message.replace('"', "\\\"");

        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            message, title
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
        let title = title.replace('"', "\\\"");
        let message = message.replace('"', "\\\"");

        let script = format!(
            "display notification \"{}\" with title \"{}\" sound name \"{}\"",
            message, title, sound
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
