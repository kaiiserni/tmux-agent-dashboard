use unicode_width::UnicodeWidthStr;

pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Truncate string to fit within max display width, adding … if needed.
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let dw = display_width(text);
    if dw <= max_width {
        return text.to_string();
    }
    let mut result = String::new();
    let mut w = 0;
    for ch in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw + 1 > max_width {
            break;
        }
        result.push(ch);
        w += cw;
    }
    result.push('\u{2026}');
    result
}
