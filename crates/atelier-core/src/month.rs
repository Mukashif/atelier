//! Parsing a month folder's name into (year, month).
//!
//! Rules (case-insensitive, spec §4):
//! 1. A full English month name **and** a whole-word year `19xx`/`20xx`.
//! 2. Else `YYYY` followed by a space, `-`, `_` or `/` and a 1–2 digit month.
//! 3. Else undated.

const MONTHS: [&str; 12] = [
    "january", "february", "march", "april", "may", "june", "july", "august", "september",
    "october", "november", "december",
];

/// English name of a month (1–12).
pub fn month_name(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    NAMES[(month.clamp(1, 12) - 1) as usize]
}

/// Folder name Atelier uses when it creates a month folder, e.g. "November 2025".
pub fn month_folder_name(year: i32, month: u32) -> String {
    format!("{} {}", month_name(month), year)
}

pub fn parse_month_folder(name: &str) -> Option<(i32, u32)> {
    let lower = name.to_lowercase();

    if let Some(idx) = MONTHS.iter().position(|m| lower.contains(m)) {
        if let Some(year) = whole_word_year(&lower) {
            return Some((year, idx as u32 + 1));
        }
    }

    numeric_year_month(&lower)
}

/// A 4-digit token starting with 19 or 20 that stands alone as a word.
fn whole_word_year(s: &str) -> Option<i32> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|tok| tok.len() == 4 && tok.bytes().all(|b| b.is_ascii_digit()))
        .find(|tok| tok.starts_with("19") || tok.starts_with("20"))
        .and_then(|tok| tok.parse().ok())
}

/// `YYYY` + separator + 1–2 digit month, e.g. `2025-09`, `2025 9`, `2025_10`.
fn numeric_year_month(s: &str) -> Option<(i32, u32)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i + 5 < b.len() + 1 {
        let starts_run = i == 0 || !b[i - 1].is_ascii_digit();
        if starts_run && i + 5 <= b.len() && b[i..i + 4].iter().all(u8::is_ascii_digit) {
            let sep = b[i + 4];
            if matches!(sep, b' ' | b'-' | b'_' | b'/') {
                let start = i + 5;
                let mut j = start;
                while j < b.len() && j < start + 2 && b[j].is_ascii_digit() {
                    j += 1;
                }
                let ends_run = j == b.len() || !b[j].is_ascii_digit();
                if j > start && ends_run {
                    let year: i32 = s[i..i + 4].parse().ok()?;
                    let month: u32 = s[start..j].parse().ok()?;
                    if (1..=12).contains(&month) {
                        return Some((year, month));
                    }
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_name_and_year() {
        assert_eq!(parse_month_folder("November 2025"), Some((2025, 11)));
        assert_eq!(parse_month_folder("september 2022"), Some((2022, 9)));
        assert_eq!(parse_month_folder("2021 - December"), Some((2021, 12)));
    }

    #[test]
    fn numeric_forms() {
        assert_eq!(parse_month_folder("2025-09"), Some((2025, 9)));
        assert_eq!(parse_month_folder("2025 9"), Some((2025, 9)));
        assert_eq!(parse_month_folder("2024_10 extra"), Some((2024, 10)));
        assert_eq!(parse_month_folder("2025/1"), Some((2025, 1)));
    }

    #[test]
    fn undated() {
        assert_eq!(parse_month_folder("Misc"), None);
        assert_eq!(parse_month_folder("November"), None);
        assert_eq!(parse_month_folder("2025-13"), None);
        assert_eq!(parse_month_folder("May12025"), None);
    }

    #[test]
    fn folder_name_round_trip() {
        assert_eq!(month_folder_name(2025, 11), "November 2025");
        assert_eq!(parse_month_folder(&month_folder_name(2026, 3)), Some((2026, 3)));
    }
}
