//! The library-wide pass.
//!
//! Three APA 7 rules cannot be applied to a citation on its own. Each depends on what *else*
//! the library contains, and the same source must cite identically in every document — so the
//! answer is computed once over the whole set and the formatters read it.
//!
//! Getting this wrong is silent. Every citation looks right in isolation; only a reader
//! holding two of them notices they are indistinguishable.

use std::collections::HashMap;

use super::source::{Role, Source};
use crate::id::Id;

/// What the formatters need to know about one source, given everything around it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// `a`, `b`, `c` — appended to the year when the same authors published twice that year.
    pub year_suffix: Option<char>,
    /// Whether the first author's initials must appear in text, because another author in the
    /// library shares their family name.
    pub needs_initials: bool,
    /// How many author names to print before `et al.`, normally one. Raised only when two
    /// works would otherwise shorten to the same thing.
    pub names_before_et_al: usize,
}

impl Entry {
    fn new() -> Self {
        Entry {
            year_suffix: None,
            needs_initials: false,
            names_before_et_al: 1,
        }
    }
}

/// The answers for a whole library.
#[derive(Debug, Clone, Default)]
pub struct Disambiguation {
    entries: HashMap<Id, Entry>,
}

impl Disambiguation {
    /// Work out every rule at once, over every source in the library.
    pub fn compute(sources: &[Source]) -> Self {
        let mut entries: HashMap<Id, Entry> =
            sources.iter().map(|s| (s.id, Entry::new())).collect();

        expand_et_al(sources, &mut entries);
        assign_year_suffixes(sources, &mut entries);
        require_initials(sources, &mut entries);

        Disambiguation { entries }
    }

    pub fn get(&self, id: Id) -> Entry {
        self.entries.get(&id).cloned().unwrap_or_else(Entry::new)
    }
}

/// The author list a source cites by, as comparable keys.
fn author_keys(source: &Source) -> Vec<String> {
    source.authors().iter().map(|c| c.sort_key()).collect()
}

/// **Different works shortening to the same `et al.` form.**
///
/// `(Chen et al., 2020)` twice, for two different papers, is worse than useless: a reader
/// cannot tell which is meant, and neither can the author six months later. Expand the name
/// list until they differ.
///
/// Applied before year suffixes, because two works with *different* authors are distinguished
/// by names rather than by a letter after the year.
fn expand_et_al(sources: &[Source], entries: &mut HashMap<Id, Entry>) {
    // Only sources of three or more authors shorten at all.
    let mut groups: HashMap<(String, Option<i32>), Vec<&Source>> = HashMap::new();
    for source in sources.iter().filter(|s| s.authors().len() >= 3) {
        let first = source
            .authors()
            .first()
            .map(|c| c.sort_key())
            .unwrap_or_default();
        groups
            .entry((first, source.date.year()))
            .or_default()
            .push(source);
    }

    for group in groups.values() {
        let lists: Vec<Vec<String>> = group.iter().map(|s| author_keys(s)).collect();
        if lists.len() < 2 {
            continue;
        }

        // Already distinct as whole works? Then only a clash of *short forms* matters, and a
        // clash exists precisely because the first name is shared. Find the smallest prefix
        // that tells every list in the group apart.
        let longest = lists.iter().map(|l| l.len()).max().unwrap_or(1);
        let mut needed = 1;
        for n in 1..=longest {
            let prefixes: Vec<&[String]> = lists.iter().map(|l| &l[..n.min(l.len())]).collect();
            needed = n;
            if all_distinct(&prefixes) {
                break;
            }
        }

        for source in group {
            if let Some(entry) = entries.get_mut(&source.id) {
                entry.names_before_et_al = needed;
            }
        }
    }
}

fn all_distinct(prefixes: &[&[String]]) -> bool {
    for (i, a) in prefixes.iter().enumerate() {
        for b in &prefixes[i + 1..] {
            if a == b {
                return false;
            }
        }
    }
    true
}

/// **Same authors, same year, different works.**
///
/// Suffix the year `a`, `b`, `c`, ordered by the reference-list title so the letters are
/// stable rather than an accident of insertion order.
fn assign_year_suffixes(sources: &[Source], entries: &mut HashMap<Id, Entry>) {
    let mut groups: HashMap<(Vec<String>, Option<i32>), Vec<&Source>> = HashMap::new();
    for source in sources {
        groups
            .entry((author_keys(source), source.date.year()))
            .or_default()
            .push(source);
    }

    for group in groups.values_mut() {
        if group.len() < 2 {
            continue;
        }
        group.sort_by_key(|source| source.title.to_lowercase());
        for (position, source) in group.iter().enumerate() {
            if let Some(entry) = entries.get_mut(&source.id) {
                // 26 works by the same authors in one year is beyond what a letter can carry;
                // leaving it unsuffixed is wrong but visible, which is the better failure.
                entry.year_suffix = u8::try_from(position)
                    .ok()
                    .filter(|p| *p < 26)
                    .map(|p| (b'a' + p) as char);
            }
        }
    }
}

/// **Different authors, the same surname.**
///
/// `(Nieminen, 2021)` and `(Nieminen, 2019)` by two different people are two different claims
/// wearing the same label. Initials separate them: `(A. Nieminen, 2021)`.
fn require_initials(sources: &[Source], entries: &mut HashMap<Id, Entry>) {
    // family name -> the distinct people carrying it
    let mut people: HashMap<String, Vec<String>> = HashMap::new();
    for source in sources {
        if let Some(first) = source.authors().first() {
            if first.is_group() {
                continue; // a group name is already unique in full
            }
            if let Some(family) = &first.family {
                let given = first.given.clone().unwrap_or_default();
                let seen = people.entry(family.clone()).or_default();
                if !seen.contains(&given) {
                    seen.push(given);
                }
            }
        }
    }

    for source in sources {
        if let Some(first) = source.authors().first() {
            if first.is_group() {
                continue;
            }
            if let Some(family) = &first.family {
                if people.get(family).is_some_and(|names| names.len() > 1) {
                    if let Some(entry) = entries.get_mut(&source.id) {
                        entry.needs_initials = true;
                    }
                }
            }
        }
    }
}

/// Unused import guard: `Role` is part of the public shape this module reasons about.
#[allow(dead_code)]
fn _roles_are_modelled(_: Role) {}
