//! The source model: what a citation is made of.
//!
//! Structured data, never formatted text. That single decision is what makes correcting a
//! source once correct every document citing it, and what makes a second citation style a
//! second formatter rather than a re-entry of the whole library.
//!
//! No formatting lives here. See [`super::format`].

use serde::{Deserialize, Serialize};

use crate::id::Id;

/// What kind of thing a source is. The type selects which fields APA 7 requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceType {
    Book,
    BookChapter,
    JournalArticle,
    WebPage,
    Video,
    Audio,
    Report,
}

impl SourceType {
    pub fn all() -> &'static [SourceType] {
        use SourceType::*;
        &[
            Book,
            BookChapter,
            JournalArticle,
            WebPage,
            Video,
            Audio,
            Report,
        ]
    }

    /// The fields APA 7 needs for this type. A source missing one is *incomplete*, never
    /// rejected — see [`Source::missing_fields`].
    pub fn required_fields(self) -> &'static [Field] {
        use Field::*;
        use SourceType::*;
        match self {
            Book => &[Authors, Date, Title, Publisher],
            BookChapter => &[Authors, Date, Title, Editors, Container, Publisher, Pages],
            JournalArticle => &[Authors, Date, Title, Container, Volume],
            WebPage => &[Authors, Date, Title, Site],
            Video => &[Authors, Date, Title, Site],
            Audio => &[Authors, Date, Title, Container],
            Report => &[Authors, Date, Title, Publisher],
        }
    }
}

/// A field a type can require. Named rather than stringly-typed so a missing field can be
/// reported precisely and a form can highlight it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Field {
    Authors,
    Editors,
    Date,
    Title,
    Container,
    Publisher,
    Site,
    Volume,
    Pages,
}

impl Field {
    /// What to call this field to a person filling in a form.
    pub fn label(self) -> &'static str {
        match self {
            Field::Authors => "author",
            Field::Editors => "editor",
            Field::Date => "date",
            Field::Title => "title",
            Field::Container => "journal or book title",
            Field::Publisher => "publisher",
            Field::Site => "site name",
            Field::Volume => "volume",
            Field::Pages => "pages",
        }
    }
}

/// Who made the thing, and in what capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Author,
    Editor,
    Translator,
    Director,
    Host,
}

/// One contributor.
///
/// `family` plus `given`, **or** `literal`. The literal form carries group authors ("World
/// Health Organization") and names that do not decompose into two Western parts — assuming
/// every name in the world does is a bug, not a simplification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contributor {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub given: Option<String>,
    /// A group author, or a name that is not two parts.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub literal: Option<String>,
    /// The short form a group author is known by after first use: `WHO`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub abbreviation: Option<String>,
}

impl Contributor {
    pub fn person(role: Role, family: impl Into<String>, given: impl Into<String>) -> Self {
        Contributor {
            role,
            family: Some(family.into()),
            given: Some(given.into()),
            literal: None,
            abbreviation: None,
        }
    }

    pub fn group(role: Role, name: impl Into<String>) -> Self {
        Contributor {
            role,
            family: None,
            given: None,
            literal: Some(name.into()),
            abbreviation: None,
        }
    }

    pub fn with_abbreviation(mut self, short: impl Into<String>) -> Self {
        self.abbreviation = Some(short.into());
        self
    }

    pub fn is_group(&self) -> bool {
        self.literal.is_some()
    }

    /// What this contributor sorts and cites by: a family name, or the whole group name.
    pub fn sort_key(&self) -> String {
        self.literal
            .clone()
            .or_else(|| self.family.clone())
            .unwrap_or_default()
    }

    /// Initials from the given names: "Anna Maria" becomes "A. M."
    pub fn initials(&self) -> String {
        self.given
            .as_deref()
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|part| part.chars().next())
            .map(|c| format!("{}.", c.to_uppercase()))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// When a source was published.
///
/// `NoDate` is a value, not a missing field. APA 7 prints `n.d.`, and modelling it as an
/// absent value would print an empty string instead — which reads as a bug to anyone who
/// knows the style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Date {
    NoDate,
    Year(i32),
    /// Year, month, day — for web pages, videos, and anything where the day matters.
    Full(i32, u8, u8),
}

impl Date {
    pub fn year(self) -> Option<i32> {
        match self {
            Date::NoDate => None,
            Date::Year(y) | Date::Full(y, _, _) => Some(y),
        }
    }

    /// The year as it appears in text, without any disambiguating suffix.
    pub fn year_text(self) -> String {
        match self.year() {
            Some(y) => y.to_string(),
            None => "n.d.".to_string(),
        }
    }
}

/// The user's own judgement of a source. Not a score the application computed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appraisal {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub trust: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
}

/// A source: something you learned from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: Id,
    #[serde(rename = "type")]
    pub source_type: SourceType,
    pub title: String,
    pub date: Date,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contributors: Vec<Contributor>,

    /// The journal, book, or collection this sits inside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Only where content is designed to change and is not archived — not on every web page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved: Option<Date>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appraisal: Option<Appraisal>,

    /// The library's own copy of this source, as a **filename inside `sources/files/`**.
    ///
    /// A filename and never a path, because the library has to stay portable: recording where
    /// the PDF sat on one computer would break the moment the folder moved, which is exactly
    /// what a user is promised cannot happen. The copy is the point — the original can be
    /// deleted out of a downloads folder and the source still has its paper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl Source {
    pub fn new(source_type: SourceType, title: impl Into<String>, date: Date) -> Self {
        Source {
            id: Id::new(),
            source_type,
            title: title.into(),
            date,
            contributors: Vec::new(),
            container: None,
            publisher: None,
            site: None,
            volume: None,
            issue: None,
            pages: None,
            edition: None,
            doi: None,
            url: None,
            retrieved: None,
            appraisal: None,
            file: None,
        }
    }

    pub fn with(mut self, contributor: Contributor) -> Self {
        self.contributors.push(contributor);
        self
    }

    pub fn contributors_in(&self, role: Role) -> Vec<&Contributor> {
        self.contributors
            .iter()
            .filter(|c| c.role == role)
            .collect()
    }

    pub fn authors(&self) -> Vec<&Contributor> {
        self.contributors_in(Role::Author)
    }

    /// The required fields this source has not got.
    ///
    /// Reported, never enforced. Somebody who cannot find a publisher should end up with an
    /// incomplete source rather than no source at all, so nothing here rejects and nothing is
    /// ever defaulted or invented.
    pub fn missing_fields(&self) -> Vec<Field> {
        self.source_type
            .required_fields()
            .iter()
            .copied()
            .filter(|field| !self.has(*field))
            .collect()
    }

    pub fn is_complete(&self) -> bool {
        self.missing_fields().is_empty()
    }

    fn has(&self, field: Field) -> bool {
        let present =
            |value: &Option<String>| value.as_deref().is_some_and(|v| !v.trim().is_empty());
        match field {
            Field::Authors => !self.authors().is_empty(),
            Field::Editors => !self.contributors_in(Role::Editor).is_empty(),
            // A deliberate absence of date is an answer, not a gap: `n.d.` is what APA prints.
            Field::Date => true,
            Field::Title => !self.title.trim().is_empty(),
            Field::Container => present(&self.container),
            Field::Publisher => present(&self.publisher),
            Field::Site => present(&self.site),
            Field::Volume => present(&self.volume),
            Field::Pages => present(&self.pages),
        }
    }

    /// The name this source alphabetises under: first author, else the title.
    pub fn sort_key(&self) -> String {
        self.authors()
            .first()
            .map(|c| c.sort_key())
            .filter(|k| !k.is_empty())
            .unwrap_or_else(|| self.title.clone())
    }
}
