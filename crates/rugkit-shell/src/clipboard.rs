//! System clipboard access (plain text).

/// Reads plain text from the system clipboard, or returns `None` if unavailable or empty.
pub fn get_text() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    cb.get_text().ok()
}

/// Writes plain text to the system clipboard. Returns `true` on success.
pub fn set_text(text: &str) -> bool {
    let Ok(mut cb) = arboard::Clipboard::new() else {
        return false;
    };
    cb.set_text(text.to_owned()).is_ok()
}
