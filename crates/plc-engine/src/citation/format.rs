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
            // A group introducing its short form puts it inside the parentheses, ahead of the
            // year: World Health Organization (WHO, 2019).
            let mut inside: Vec<String> =
                introduced(source).map(str::to_string).into_iter().collect();
            inside.push(year);
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
        // No author: the title moves into the author position — in italics where the
        // reference list italicises it, in quotation marks where it does not (APA 7 §8.14).
        return if italic_title(source.source_type) {
            format!("*{}*", source.title)
        } else if form == Form::Parenthetical {
            // The comma that separates it from the year goes inside the closing quote.
            format!("\u{201c}{},\u{201d}", source.title)
        } else {
            format!("\u{201c}{}\u{201d}", source.title)
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
        // In a sentence the short form shares the parentheses with the year; `in_text` puts
        // it there.
        (Some(_), Form::Narrative) | (None, _) => full,
    }
}

/// The short form a group author introduces, if it is the only author and has one.
fn introduced(source: &Source) -> Option<&str> {
    match source.authors()[..] {
        [only] if only.is_group() => only.abbreviation.as_deref(),
        _ => None,
    }
}

// ---- reference list ------------------------------------------------------------

/// One reference-list entry.
pub fn reference_entry(source: &Source, disambiguation: &Disambiguation) -> String {
    let entry = disambiguation.get(source.id);
    let authors = source.authors();
    let date = format!("({}).", reference_date(source, &entry));
    let title = sentence(&title_part(source));

    // Who, when, what — or, with nobody credited, what, when: the title takes the author's
    // place and the date follows it (APA 7 §9.12).
    let mut parts = if authors.is_empty() {
        vec![title, date]
    } else {
        vec![reference_names(source, &authors), date, title]
    };
    let rest = rest_part(source, &authors);
    if !rest.is_empty() {
        parts.push(rest);
    }
    let mut out = parts.join(" ");

    // A retrieval date belongs only on content designed to change and not archived. Putting
    // one on every web page is the commonest APA 7 error in the wild, so it is printed only
    // where the source actually carries one.
    if let Some(retrieved) = source.retrieved {
        out.push_str(&format!(" Retrieved {}, from", long_date(retrieved)));
    }

    if let Some(doi) = present(&source.doi) {
        out.push_str(&format!(
            " https://doi.org/{}",
            doi.trim_start_matches("https://doi.org/")
        ));
    } else if let Some(url) = present(&source.url) {
        out.push_str(&format!(" {url}"));
    }

    out.trim_end().to_string()
}

/// Whether a type's title is italicised: works that stand alone are, parts of something
/// larger — an article, a chapter, an episode — are not (APA 7 §6.22).
fn italic_title(source_type: SourceType) -> bool {
    matches!(
        source_type,
        SourceType::Book | SourceType::Report | SourceType::WebPage | SourceType::Video
    )
}

/// The title, with what travels with it: an edition, the translators, a number, and the
/// bracketed description of what kind of thing it is.
fn title_part(source: &Source) -> String {
    let mut out = if italic_title(source.source_type) {
        format!("*{}*", source.title.trim())
    } else {
        source.title.trim().to_string()
    };

    match source.source_type {
        SourceType::Book => {
            // Translators first, then the edition: (H. Weaver, Trans.; 2nd ed.).
            let mut inside = Vec::new();
            let translators = source.contributors_in(Role::Translator);
            if !translators.is_empty() {
                inside.push(format!("{}, Trans.", names_in_order(&translators)));
            }
            if let Some(edition) = present(&source.edition) {
                inside.push(edition_label(edition));
            }
            if !inside.is_empty() {
                out.push_str(&format!(" ({})", inside.join("; ")));
            }
        }
        SourceType::Report => {
            if let Some(edition) = present(&source.edition) {
                out.push_str(&format!(" ({})", edition_label(edition)));
            }
            if let Some(number) = present(&source.number) {
                out.push_str(&format!(" ({})", number_label(number, "Report No.")));
            }
        }
        SourceType::Video => out.push_str(" [Video]"),
        SourceType::Audio => {
            if let Some(number) = present(&source.number) {
                out.push_str(&format!(" ({})", number_label(number, "No.")));
            }
            out.push_str(" [Audio podcast episode]");
        }
        SourceType::JournalArticle | SourceType::BookChapter | SourceType::WebPage => {}
    }
    out
}

/// Everything after the title: where the work sits, and who published it.
fn rest_part(source: &Source, authors: &[&Contributor]) -> String {
    let mut out = String::new();
    // A publisher or a site that is the author over again is left out (APA 7 §9.24, §9.25).
    let unless_author = |value: Option<&str>| -> Option<String> {
        value.filter(|v| !same_as_author(authors, v)).map(sentence)
    };

    match source.source_type {
        SourceType::Book | SourceType::Report => {
            if let Some(publisher) = unless_author(present(&source.publisher)) {
                out.push_str(&publisher);
            }
        }
        SourceType::JournalArticle => {
            if let Some(journal) = present(&source.container) {
                out.push_str(&format!("*{journal}*"));
                if let Some(volume) = present(&source.volume) {
                    out.push_str(&format!(", *{volume}*"));
                    if let Some(issue) = present(&source.issue) {
                        out.push_str(&format!("({issue})"));
                    }
                }
                if let Some(pages) = present(&source.pages) {
                    out.push_str(&format!(", {}", en_dash(pages)));
                }
                out.push('.');
            }
        }
        SourceType::BookChapter => {
            let editors = source.contributors_in(Role::Editor);
            let mut in_part = String::from("In ");
            if !editors.is_empty() {
                let label = if editors.len() == 1 { "Ed." } else { "Eds." };
                in_part.push_str(&format!("{} ({label}), ", names_in_order(&editors)));
            }
            if let Some(book) = present(&source.container) {
                in_part.push_str(&format!("*{book}*"));
            }
            if let Some(pages) = present(&source.pages) {
                in_part.push_str(&format!(" (pp. {})", en_dash(pages)));
            }
            if in_part != "In " {
                out.push_str(&sentence(in_part.trim_end_matches(", ")));
            }
            if let Some(publisher) = unless_author(present(&source.publisher)) {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&publisher);
            }
        }
        SourceType::WebPage | SourceType::Video => {
            if let Some(site) = unless_author(present(&source.site)) {
                out.push_str(&site);
            }
        }
        SourceType::Audio => {
            if let Some(podcast) = present(&source.container) {
                out.push_str(&format!("In *{podcast}*."));
            }
            if let Some(publisher) = unless_author(present(&source.publisher)) {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&publisher);
            }
        }
    }
    out
}

/// A field with something in it, trimmed; `None` for an absent or blank one.
fn present(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// End a part of an entry with a full stop — unless it ends with one already, or with a
/// question mark or an exclamation, which APA keeps and does not follow with a full stop.
fn sentence(part: &str) -> String {
    let part = part.trim_end();
    let last = part.trim_end_matches(['*', '"', '\u{201d}']).chars().last();
    if matches!(last, Some('.' | '?' | '!')) {
        part.to_string()
    } else {
        format!("{part}.")
    }
}

/// "2" as "2nd ed."; anything already written out — "2nd ed.", "Rev. ed." — as it is.
fn edition_label(edition: &str) -> String {
    match edition.parse::<u32>() {
        Ok(n) => format!("{}{} ed.", n, ordinal_suffix(n)),
        Err(_) => edition.to_string(),
    }
}

fn ordinal_suffix(n: u32) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

/// A bare number takes the usual label; one written with its own — "NIH Publication No.
/// 18-2059" — is printed as the work prints it.
fn number_label(number: &str, label: &str) -> String {
    if number.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{label} {number}")
    } else {
        number.to_string()
    }
}

/// Whether a publisher or site is the author over again: one group author, by the same name.
fn same_as_author(authors: &[&Contributor], value: &str) -> bool {
    matches!(authors, [only] if only.is_group()
        && only.sort_key().trim().to_lowercase() == value.trim().to_lowercase())
}

/// Editors and translators as APA sets them inside an entry: initials first — "R. F.
/// Subotnik" — joined with an ampersand, and a comma before it from three on.
fn names_in_order(people: &[&Contributor]) -> String {
    let names: Vec<String> = people
        .iter()
        .map(|c| {
            if c.is_group() {
                c.sort_key()
            } else {
                format!("{} {}", c.initials(), c.sort_key())
                    .trim()
                    .to_string()
            }
        })
        .collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} & {second}"),
        [rest @ .., last] => format!("{}, & {last}", rest.join(", ")),
    }
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
            vec![title_sort_key(&s.title)]
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

/// A title as it files in a reference list: by its first significant word, so "The" and
/// "A" and "An" at the start are passed over (APA 7 §9.47).
fn title_sort_key(title: &str) -> String {
    let title = title.trim().to_lowercase();
    for article in ["the ", "an ", "a "] {
        if let Some(rest) = title.strip_prefix(article) {
            return rest.trim_start().to_string();
        }
    }
    title
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
fn reference_names(source: &Source, authors: &[&Contributor]) -> String {
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

    let names = match formatted.len() {
        0 => return String::new(),
        1 => formatted[0].clone(),
        2..=20 => {
            let (last, rest) = formatted.split_last().unwrap();
            format!("{}, & {last}", rest.join(", "))
        }
        _ => {
            let first_nineteen = formatted[..19].join(", ");
            let last = formatted.last().unwrap();
            format!("{first_nineteen}, . . . {last}")
        }
    };

    // Hosts standing in the author's place say so: "Glass, I. (Host)." (APA 7 §10.13).
    if authors.iter().all(|c| c.role == Role::Host)
        && source.contributors_in(Role::Author).is_empty()
    {
        let label = if authors.len() == 1 { "Host" } else { "Hosts" };
        return format!("{names} ({label}).");
    }
    terminate(names)
}
