use anyhow::{Context, Result};
use std::process::Command;
use tracing::{debug, error, info, warn};

/// Text input handler for Wayland
/// Uses wtype (preferred) or ydotool for text input simulation
pub struct TextInputHandler {
    method: InputMethod,
}

#[derive(Debug, Clone, Copy)]
enum InputMethod {
    Wtype,
    Ydotool,
    WlCopy, // Fallback: copy to clipboard
}

impl TextInputHandler {
    pub fn new() -> Self {
        // Detect available input method
        let method = if is_command_available("wtype") {
            info!("Using wtype for text input");
            InputMethod::Wtype
        } else if is_command_available("ydotool") {
            info!("Using ydotool for text input");
            InputMethod::Ydotool
        } else if is_command_available("wl-copy") {
            warn!("wtype and ydotool not found, falling back to wl-copy (clipboard)");
            InputMethod::WlCopy
        } else {
            warn!("No suitable text input method found! Install wtype, ydotool, or wl-copy");
            InputMethod::WlCopy
        };

        Self { method }
    }

    /// Type the given text as keyboard input
    pub fn type_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        debug!("Typing text: {}", text);

        match self.method {
            InputMethod::Wtype => self.type_with_wtype(text),
            InputMethod::Ydotool => self.type_with_ydotool(text),
            InputMethod::WlCopy => self.copy_to_clipboard(text),
        }
    }

    fn type_with_wtype(&self, text: &str) -> Result<()> {
        // wtype directly types the text
        let output = Command::new("wtype")
            .arg("--")
            .arg(text)
            .output()
            .context("Failed to execute wtype")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("wtype failed: {}", stderr);
            return Err(anyhow::anyhow!("wtype failed: {}", stderr));
        }

        Ok(())
    }

    fn type_with_ydotool(&self, text: &str) -> Result<()> {
        // ydotool needs ydotoold daemon running
        let output = Command::new("ydotool")
            .arg("type")
            .arg("--")
            .arg(text)
            .output()
            .context("Failed to execute ydotool")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("ydotool failed: {}", stderr);
            return Err(anyhow::anyhow!("ydotool failed: {}", stderr));
        }

        Ok(())
    }

    fn copy_to_clipboard(&self, text: &str) -> Result<()> {
        // Fallback: copy to clipboard and let user paste
        let mut child = Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .context("Failed to execute wl-copy")?;

        use std::io::Write;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("wl-copy may have failed: {}", stderr);
        }

        info!("Text copied to clipboard. Press Ctrl+V to paste.");
        Ok(())
    }

    /// Type text incrementally (for real-time transcription)
    /// This replaces the previous partial text with the new text
    pub fn update_text(&self, previous: &str, new: &str) -> Result<()> {
        if previous == new {
            return Ok(());
        }

        // Find the common prefix
        let common_len = previous
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .count();

        let prev_chars: Vec<char> = previous.chars().collect();
        let new_chars: Vec<char> = new.chars().collect();

        // Number of characters to delete
        let delete_count = prev_chars.len() - common_len;

        // Characters to add
        let add_text: String = new_chars[common_len..].iter().collect();

        // Delete previous characters
        if delete_count > 0 {
            self.delete_chars(delete_count)?;
        }

        // Type new characters
        if !add_text.is_empty() {
            self.type_text(&add_text)?;
        }

        Ok(())
    }

    fn delete_chars(&self, count: usize) -> Result<()> {
        if count == 0 {
            return Ok(());
        }

        debug!("Deleting {} characters", count);

        match self.method {
            InputMethod::Wtype => {
                // wtype can send key presses
                for _ in 0..count {
                    Command::new("wtype")
                        .arg("-k")
                        .arg("BackSpace")
                        .output()
                        .context("Failed to send backspace via wtype")?;
                }
                Ok(())
            }
            InputMethod::Ydotool => {
                // ydotool can send key presses
                for _ in 0..count {
                    Command::new("ydotool")
                        .arg("key")
                        .arg("14:1") // BackSpace key code
                        .arg("14:0")
                        .output()
                        .context("Failed to send backspace via ydotool")?;
                }
                Ok(())
            }
            InputMethod::WlCopy => {
                // Can't delete with clipboard method
                warn!("Cannot delete characters with clipboard method");
                Ok(())
            }
        }
    }
}

fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

impl Default for TextInputHandler {
    fn default() -> Self {
        Self::new()
    }
}
