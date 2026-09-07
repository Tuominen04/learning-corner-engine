//! Filenames.
//!
//! A slug is for humans reading their own folder, not for the machine: identity is the ULID
//! inside the file, so a slug can change freely and collisions only need to be avoided, never
//! resolved. Non-ASCII letters are kept — this is an application for people learning Russian
//! and writing Finnish, and transliterating their filenames for them would be presumptuous.

use std::collections::HashSet;

const MAX_LENGTH: usize = 80;

/// Characters no filesystem we target tolerates in a name.
const HOSTILE: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Names Windows refuses regardless of extension. Cheap to avoid now, painful to discover
/// during the M5 port.
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Turn a title into a filename stem.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut pending_separator = false;

    for ch in title.trim().chars() {
        if ch.is_alphanumeric() {
            if pending_separator && !out.is_empty() {
                out.push('-');
            }
            pending_separator = false;
            out.extend(ch.to_lowercase());
        } else if ch.is_whitespace() || ch == '-' || ch == '_' || HOSTILE.contains(&ch) {
            pending_separator = true;
        }
        // Everything else — punctuation, control characters — is dropped outright.

        if out.chars().count() >= MAX_LENGTH {
            break;
        }
    }

    let out = out.trim_matches(['-', '.']).to_string();

    if out.is_empty() || RESERVED.contains(&out.as_str()) {
        format!("{out}-untitled")
            .trim_start_matches('-')
            .to_string()
    } else {
        out
    }
}

/// A slug that is not already taken, suffixed `-2`, `-3`, … if it is.
pub fn unique(title: &str, taken: &HashSet<String>) -> String {
    let base = slugify(title);
    if !taken.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|c| !taken.contains(c))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_and_joins_words() {
        assert_eq!(slugify("Types of Protein"), "types-of-protein");
    }

    #[test]
    fn keeps_finnish_and_cyrillic_letters() {
        assert_eq!(slugify("Ääkköset ja Öljy"), "ääkköset-ja-öljy");
        assert_eq!(slugify("Я вас любил"), "я-вас-любил");
    }

    #[test]
    fn strips_characters_the_filesystem_hates() {
        assert_eq!(slugify("what/is: protein?"), "what-is-protein");
        assert_eq!(slugify("a\\b*c|d"), "a-b-c-d");
    }

    #[test]
    fn drops_punctuation_without_leaving_gaps() {
        assert_eq!(slugify("Nieminen's (2021) study"), "nieminens-2021-study");
    }

    #[test]
    fn collapses_runs_of_separators() {
        assert_eq!(slugify("a   ---   b"), "a-b");
        assert_eq!(slugify("  spaced  "), "spaced");
    }

    #[test]
    fn never_returns_an_empty_name() {
        assert_eq!(slugify(""), "untitled");
        assert_eq!(slugify("!!!"), "untitled");
        assert_eq!(slugify("   "), "untitled");
    }

    #[test]
    fn avoids_names_windows_refuses() {
        assert_eq!(slugify("CON"), "con-untitled");
        assert_eq!(slugify("aux"), "aux-untitled");
    }

    #[test]
    fn is_bounded_in_length() {
        assert!(slugify(&"word ".repeat(200)).chars().count() <= MAX_LENGTH + 1);
    }

    #[test]
    fn suffixes_collisions() {
        let taken: HashSet<String> = ["protein".to_string(), "protein-2".to_string()]
            .into_iter()
            .collect();
        assert_eq!(unique("Protein", &taken), "protein-3");
        assert_eq!(unique("Other", &taken), "other");
    }
}
