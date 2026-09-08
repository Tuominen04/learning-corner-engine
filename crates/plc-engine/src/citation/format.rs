//! Turning a source into APA 7 text.
//!
//! Pure functions from (source, disambiguation, options) to a string. The only style-specific
//! layer: everything below it is a source model that a second style would reuse unchanged.
//!
//! Emphasis is written as Markdown `*…*`, because that is what a document in this application
//! is made of and what an export renders from.

use super::disambiguate::{Disambiguation, Entry};
use super::source::{Contributor, Date, Role, Source, SourceType};

/// Where the citation sits in the sentence, which decides `&` versus `and`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `(Nieminen, 2021, p. 44)` — the fact matters, not who said it.
    Parenthetical,
    /// `Nieminen (2021)` — who said it is part of the point.
    Narrative,
}

/// A page, paragraph, chapter or timestamp, as the user typed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Page(String),
    Paragraph(String),
    Chapter(String),
    Timestamp(String),
}

impl Locator {
    fn render(&self) -> String {
        match self {
            Locator::Page(v) => {
                // A range takes "pp.", a single page takes "p.".
                let dashed = v.contains('-') || v.contains('–');
                let prefix = if dashed { "pp." } else { "p." };
                format!("{prefix} {}", en_dash(v))
            }
            Locator::Paragraph(v) => format!("para. {v}"),
            Locator::Chapter(v) => format!("Chapter {v}"),
            Locator::Timestamp(v) => v.clone(),
        }
    }
}

/// Ranges are set with an en dash, which is the difference between typeset and typed.
fn en_dash(value: &str) -> String {
    value.replace('-', "\u{2013}")
}

// ---- in-text -------------------------------------------------------------------

/// The in-text citation for one source.
pub fn in_text(
    source: &Source,
    disambiguation: &Disambiguation,
    form: Form,
    locator: Option<&Locator>,
) -> String {
    let entry = disambiguation.get(source.id);
    let names = author_label(source, &entry, form);
    let year = year_label(source, &entry);

    match form {
        Form::Parenthetical => {
            // A title standing in for an absent author already ends with a comma inside its
            // closing quote — APA sets ("Protein requirements," 2021) — so adding the usual
            // separator would double it.
            let mut out = if names.ends_with(',') || names.ends_with('\u{201d}') {
                format!("{names} {year}")
            } else {
                format!("{names}, {year}")
            };
            if let Some(locator) = locator {
                out.push_str(&format!(", {}", locator.render()));
            }
            format!("({out})")
        }
        Form::Narrative => {
            let mut inside = vec![year];
            if let Some(locator) = locator {
                inside.push(locator.render());
            }
            format!("{names} ({})", inside.join(", "))
        }
    }
}

/// The year, plus any disambiguating letter. `n.d.` when there is no date.
fn year_label(source: &Source, entry: &Entry) -> String {
    let mut year = source.date.year_text();
    if let Some(suffix) = entry.year_suffix {
        year.push(suffix);
    }
    year
}

/// The names, per the author-count rules — and this is where APA 7 differs from APA 6.
fn author_label(source: &Source, entry: &Entry, form: Form) -> String {
    let authors = source.authors();

    if authors.is_empty() {
        // No author: the title moves into the author position, quoted for shorter works.
        return match source.source_type {
            SourceType::Book | SourceType::Report => format!("*{}*", source.title),
            _ => format!("\u{201c}{},\u{201d}", source.title),
        };
    }

    if authors.len() == 1 && authors[0].is_group() {
        return group_label(authors[0], form);
    }

    let name = |c: &Contributor| -> String {
        if entry.needs_initials && !c.is_group() {
            let initials = c.initials();
            if initials.is_empty() {
                c.sort_key()
            } else {
                format!("{initials} {}", c.sort_key())
            }
        } else {
            c.sort_key()
        }
    };

    match authors.len() {
        1 => name(authors[0]),
        2 => {
            let joiner = match form {
                // `&` inside parentheses, the word `and` in a sentence. APA 7, and precisely
                // the sort of thing nobody should have to remember.
                Form::Parenthetical => "&",
                Form::Narrative => "and",
            };
            format!("{} {joiner} {}", name(authors[0]), name(authors[1]))
        }
        // Three or more shorten from the **first** citation onwards. This changed in APA 7;
        // anyone checking against memory of APA 6 will expect the full list first time.
        _ => {
            let shown = entry.names_before_et_al.clamp(1, authors.len() - 1);
            let listed: Vec<String> = authors[..shown].iter().map(|c| name(c)).collect();
            // One name runs straight into `et al.`; two or more take a comma before it.
            // "Nieminen et al." but "Chen, Wang, et al." — APA 7 §8.17 and §8.18.
            if listed.len() == 1 {
                format!("{} et al.", listed[0])
            } else {
                format!("{}, et al.", listed.join(", "))
            }
        }
    }
}

fn group_label(group: &Contributor, form: Form) -> String {
    let full = group.sort_key();
    match (&group.abbreviation, form) {
        // First use spells the group out and introduces the short form. Whether a use is the
        // *first* is a property of a document, so the caller decides by supplying the
        // abbreviation only where it should be introduced.
        (Some(short), Form::Parenthetical) => format!("{full} [{short}]"),
        (Some(short), Form::Narrative) => format!("{full} ({short}"),
        (None, _) => full,
    }
}

// ---- reference list ------------------------------------------------------------

/// One reference-list entry.
pub fn reference_entry(source: &Source, disambiguation: &Disambiguation) -> String {
    let entry = disambiguation.get(source.id);
    let mut out = String::new();

    let authors = source.authors();
    if !authors.is_empty() {
        out.push_str(&reference_names(&authors));
        out.push(' ');
    }

    out.push_str(&format!("({}). ", reference_date(source, &entry)));

    match source.source_type {
        SourceType::Book | SourceType::Report => {
            out.push_str(&format!("*{}*", source.title));
            if let Some(edition) = &source.edition {
                out.push_str(&format!(" ({edition})"));
            }
            out.push_str(". ");
            if let Some(publisher) = &source.publisher {
                out.push_str(&format!("{publisher}. "));
            }
        }
        SourceType::JournalArticle => {
            out.push_str(&format!("{}. ", source.title));
            if let Some(journal) = &source.container {
                out.push_str(&format!("*{journal}*"));
                if let Some(volume) = &source.volume {
                    out.push_str(&format!(", *{volume}*"));
                    if let Some(issue) = &source.issue {
                        out.push_str(&format!("({issue})"));
                    }
                }
                if let Some(pages) = &source.pages {
                    out.push_str(&format!(", {}", en_dash(pages)));
                }
                out.push_str(". ");
            }
        }
        SourceType::BookChapter => {
            out.push_str(&format!("{}. ", source.title));
            let editors = source.contributors_in(Role::Editor);
            if !editors.is_empty() {
                let names: Vec<String> = editors
                    .iter()
                    .map(|e| {
                        format!("{} {}", e.initials(), e.sort_key())
                            .trim()
                            .to_string()
                    })
                    .collect();
                let label = if editors.len() == 1 { "Ed." } else { "Eds." };
                out.push_str(&format!("In {} ({label}), ", names.join(", ")));
            }
            if let Some(book) = &source.container {
                out.push_str(&format!("*{book}*"));
            }
            if let Some(pages) = &source.pages {
                out.push_str(&format!(" (pp. {})", en_dash(pages)));
            }
            out.push_str(". ");
            if let Some(publisher) = &source.publisher {
                out.push_str(&format!("{publisher}. "));
            }
        }
        SourceType::WebPage => {
            out.push_str(&format!("{}. ", source.title));
            if let Some(site) = &source.site {
                out.push_str(&format!("{site}. "));
            }
        }
        SourceType::Video => {
            out.push_str(&format!("*{}* [Video]. ", source.title));
            if let Some(site) = &source.site {
                out.push_str(&format!("{site}. "));
            }
        }
        SourceType::Audio => {
            out.push_str(&format!("*{}* [Audio]. ", source.title));
            if let Some(container) = &source.container {
                out.push_str(&format!("{container}. "));
            }
        }
    }

    // A retrieval date belongs only on content designed to change and not archived. Putting
    // one on every web page is the commonest APA 7 error in the wild, so it is printed only
    // where the source actually carries one.
    if let Some(retrieved) = source.retrieved {
        out.push_str(&format!("Retrieved {}, from ", long_date(retrieved)));
    }

    if let Some(doi) = &source.doi {
        out.push_str(&format!(
            "https://doi.org/{}",
            doi.trim_start_matches("https://doi.org/")
        ));
    } else if let Some(url) = &source.url {
        out.push_str(url);
    }

    out.trim_end().to_string()
}

/// The order APA 7 puts a reference list in.
///
/// By author, then year, then title — but "by author" means the **whole list of them**, not
/// just the first. Comparing author lists element by element is what puts `Nieminen, A.`
/// before `Nieminen, A., & Kallio, K.`: the shorter list is a prefix of the longer one and so
/// sorts first. Ordering on the first author alone leaves those two in whatever order their
/// titles happen to fall, which is wrong (APA 7 §9.47).
pub fn reference_order(a: &Source, b: &Source) -> std::cmp::Ordering {
    // A source with no author alphabetises by its title, which stands in the author position.
    let keys = |s: &Source| -> Vec<String> {
        let authors: Vec<String> = s
            .authors()
            .iter()
            .map(|c| c.sort_key().to_lowercase())
            .collect();
        if authors.is_empty() {
            vec![s.title.to_lowercase()]
        } else {
            authors
        }
    };

    // Case-folded comparison. Locale-aware collation is a later refinement; byte order is
    // simply wrong for the Finnish and Cyrillic names this application is built for.
    keys(a)
        .cmp(&keys(b))
        .then(
            a.date
                .year()
                .unwrap_or(i32::MAX)
                .cmp(&b.date.year().unwrap_or(i32::MAX)),
        )
        .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
}

/// Every reference entry for a set of sources, in APA order.
pub fn reference_list(sources: &[Source], disambiguation: &Disambiguation) -> Vec<String> {
    let mut ordered: Vec<&Source> = sources.iter().collect();
    ordered.sort_by(|a, b| reference_order(a, b));
    ordered
        .iter()
        .map(|s| reference_entry(s, disambiguation))
        .collect()
}

fn reference_date(source: &Source, entry: &Entry) -> String {
    let base = match source.date {
        Date::NoDate => "n.d.".to_string(),
        Date::Year(y) => y.to_string(),
        Date::Full(y, m, d) => format!("{y}, {} {d}", month_name(m)),
    };
    match entry.year_suffix {
        // The letter follows the year, not the whole date: "2021a, September 6" is wrong.
        Some(suffix) => match source.date {
            Date::Full(y, m, d) => format!("{y}{suffix}, {} {d}", month_name(m)),
            _ => format!("{base}{suffix}"),
        },
        None => base,
    }
}

fn long_date(date: Date) -> String {
    match date {
        Date::NoDate => "n.d.".to_string(),
        Date::Year(y) => y.to_string(),
        Date::Full(y, m, d) => format!("{} {d}, {y}", month_name(m)),
    }
}

fn month_name(month: u8) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

/// Author names as the reference list sets them: `Family, A. A.`, joined with `&`.
///
/// Twenty or fewer are all listed. Twenty-one or more take the first nineteen, an ellipsis,
/// and the final author — never `et al.`, which belongs in text and not here.
fn reference_names(authors: &[&Contributor]) -> String {
    let formatted: Vec<String> = authors
        .iter()
        .map(|c| {
            if c.is_group() {
                c.sort_key()
            } else {
                let initials = c.initials();
                if initials.is_empty() {
                    c.sort_key()
                } else {
                    format!("{}, {initials}", c.sort_key())
                }
            }
        })
        .collect();

    // Initials already end in a full stop. Appending another gives "Nieminen, A.." — small,
    // and exactly the sort of thing a reader of a reference list notices immediately.
    let terminate = |name: String| {
        if name.ends_with('.') {
            name
        } else {
            format!("{name}.")
        }
    };

    match formatted.len() {
        0 => String::new(),
        1 => terminate(formatted[0].clone()),
        2..=20 => {
            let (last, rest) = formatted.split_last().unwrap();
            terminate(format!("{}, & {last}", rest.join(", ")))
        }
        _ => {
            let first_nineteen = formatted[..19].join(", ");
            let last = formatted.last().unwrap();
            terminate(format!("{first_nineteen}, . . . {last}"))
        }
    }
}
