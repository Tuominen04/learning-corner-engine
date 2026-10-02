//! APA 7, rule by rule.
//!
//! The product's whole claim is that it gets this right, so being approximately correct here
//! is the same as being wrong. Every rule in `citation-model.md` gets at least one case, and
//! every disambiguation rule gets a case with more than one source — because those rules
//! cannot be observed from a single citation at all.

use plc_engine::citation::disambiguate::Disambiguation;
use plc_engine::citation::format::{
    in_text, in_text_cite, parenthetical_together, reference_entry, reference_list, Cite, Form,
    Locator,
};
use plc_engine::citation::source::{Contributor, Date, Role, Source, SourceType};

fn author(family: &str, given: &str) -> Contributor {
    Contributor::person(Role::Author, family, given)
}

fn article(title: &str, year: i32, authors: Vec<Contributor>) -> Source {
    let mut source = Source::new(SourceType::JournalArticle, title, Date::Year(year));
    source.contributors = authors;
    source.container = Some("Journal of Nutrition".into());
    source.volume = Some("151".into());
    source
}

/// In-text form for a source considered on its own.
fn alone(source: &Source, form: Form) -> String {
    let sources = vec![source.clone()];
    in_text(source, &Disambiguation::compute(&sources), form, None)
}

// ---- author counts -------------------------------------------------------------

#[test]
fn one_author() {
    let s = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    assert_eq!(alone(&s, Form::Parenthetical), "(Nieminen, 2021)");
    assert_eq!(alone(&s, Form::Narrative), "Nieminen (2021)");
}

#[test]
fn two_authors_use_an_ampersand_only_inside_parentheses() {
    let s = article(
        "Protein",
        2021,
        vec![author("Nieminen", "Anna"), author("Kallio", "Kai")],
    );
    assert_eq!(alone(&s, Form::Parenthetical), "(Nieminen & Kallio, 2021)");
    assert_eq!(alone(&s, Form::Narrative), "Nieminen and Kallio (2021)");
}

#[test]
fn three_or_more_shorten_from_the_very_first_citation() {
    // This changed in APA 7. Anyone checking against memory of APA 6 will expect the full
    // list on first use and a shortened one afterwards.
    let s = article(
        "Protein",
        2021,
        vec![
            author("Nieminen", "Anna"),
            author("Kallio", "Kai"),
            author("Virtanen", "Vi"),
        ],
    );
    assert_eq!(alone(&s, Form::Parenthetical), "(Nieminen et al., 2021)");
    assert_eq!(alone(&s, Form::Narrative), "Nieminen et al. (2021)");
}

#[test]
fn a_group_author_introduces_its_abbreviation_then_uses_it() {
    let mut s = article("Protein intake", 2019, vec![]);
    s.contributors = vec![
        Contributor::group(Role::Author, "World Health Organization").with_abbreviation("WHO"),
    ];
    assert_eq!(
        alone(&s, Form::Parenthetical),
        "(World Health Organization [WHO], 2019)"
    );
    // In a sentence the short form shares the parentheses with the year, and a page joins
    // them there.
    assert_eq!(
        alone(&s, Form::Narrative),
        "World Health Organization (WHO, 2019)"
    );
    let sources = vec![s.clone()];
    assert_eq!(
        in_text(
            &s,
            &Disambiguation::compute(&sources),
            Form::Narrative,
            Some(&Locator::Page("12".into()))
        ),
        "World Health Organization (WHO, 2019, p. 12)"
    );

    let mut later = s.clone();
    later.contributors = vec![Contributor::group(Role::Author, "WHO")];
    assert_eq!(alone(&later, Form::Parenthetical), "(WHO, 2019)");
    assert_eq!(alone(&later, Form::Narrative), "WHO (2019)");
}

#[test]
fn no_author_puts_the_title_in_the_author_position() {
    let s = article("Protein requirements", 2021, vec![]);
    assert_eq!(
        alone(&s, Form::Parenthetical),
        "(\u{201c}Protein requirements,\u{201d} 2021)"
    );
    // The comma inside the quote separates the title from the year. In a sentence there is
    // nothing to separate.
    assert_eq!(
        alone(&s, Form::Narrative),
        "\u{201c}Protein requirements\u{201d} (2021)"
    );
}

#[test]
fn no_date_is_n_d_rather_than_an_empty_space() {
    let mut s = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    s.date = Date::NoDate;
    assert_eq!(alone(&s, Form::Parenthetical), "(Nieminen, n.d.)");
    assert_eq!(alone(&s, Form::Narrative), "Nieminen (n.d.)");
}

#[test]
fn initials_keep_a_hyphen_and_pass_over_a_stray_one() {
    let initials = |given: &str| Contributor::person(Role::Author, "Sartre", given).initials();
    assert_eq!(initials("Anna Maria"), "A. M.");
    assert_eq!(initials("Jean-Paul"), "J.-P.");
    // A hyphen typed on its own between two names is not a third name.
    assert_eq!(initials("Jean - Paul"), "J. P.");
}

// ---- several works at one place, and short forms once introduced ----------------

fn cite<'a>(source: &'a Source, locator: Option<&'a Locator>) -> Cite<'a> {
    Cite {
        source,
        locator,
        introduced: false,
    }
}

fn who(year: i32, title: &str) -> Source {
    let mut s = article(title, year, vec![]);
    s.contributors = vec![
        Contributor::group(Role::Author, "World Health Organization").with_abbreviation("WHO"),
    ];
    s
}

#[test]
fn works_cited_together_share_one_pair_of_parentheses_in_list_order() {
    // APA 7 §8.12: in the order of the reference list, separated by semicolons.
    let nieminen = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    let kallio = article(
        "Iron",
        2019,
        vec![author("Kallio", "Kai"), author("Virtanen", "Vi")],
    );
    let sources = vec![nieminen.clone(), kallio.clone()];
    let d = Disambiguation::compute(&sources);
    assert_eq!(
        parenthetical_together(&[cite(&nieminen, None), cite(&kallio, None)], &d),
        "(Kallio & Virtanen, 2019; Nieminen, 2021)"
    );
}

#[test]
fn one_authors_works_name_them_once_then_the_years() {
    let mut undated = article("Zinc", 2000, vec![author("Nieminen", "Anna")]);
    undated.date = Date::NoDate;
    let early = article("Iron", 2019, vec![author("Nieminen", "Anna")]);
    let a = article("Amino acids", 2021, vec![author("Nieminen", "Anna")]);
    let b = article("Basics", 2021, vec![author("Nieminen", "Anna")]);
    let sources = vec![b.clone(), a.clone(), early.clone(), undated.clone()];
    let d = Disambiguation::compute(&sources);
    assert_eq!(
        parenthetical_together(
            &[
                cite(&b, None),
                cite(&early, None),
                cite(&undated, None),
                cite(&a, None)
            ],
            &d
        ),
        "(Nieminen, n.d., 2019, 2021a, 2021b)"
    );
}

#[test]
fn a_page_keeps_its_work_apart_from_the_authors_others() {
    // "Nieminen, 2019, p. 4, 2021" would read as one work's locator.
    let early = article("Iron", 2019, vec![author("Nieminen", "Anna")]);
    let late = article("Zinc", 2021, vec![author("Nieminen", "Anna")]);
    let sources = vec![early.clone(), late.clone()];
    let d = Disambiguation::compute(&sources);
    let page = Locator::Page("4".into());
    assert_eq!(
        parenthetical_together(&[cite(&late, None), cite(&early, Some(&page))], &d),
        "(Nieminen, 2019, p. 4; Nieminen, 2021)"
    );
}

#[test]
fn the_same_work_cited_twice_in_one_place_appears_once() {
    let s = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    let sources = vec![s.clone()];
    let d = Disambiguation::compute(&sources);
    assert_eq!(
        parenthetical_together(&[cite(&s, None), cite(&s, None)], &d),
        "(Nieminen, 2021)"
    );
}

#[test]
fn one_work_alone_is_its_own_parenthetical_citation() {
    let person = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    let group = who(2019, "Protein intake");
    let untitled = article("Protein requirements", 2021, vec![]);
    let sources = vec![person.clone(), group.clone(), untitled.clone()];
    let d = Disambiguation::compute(&sources);
    let page = Locator::Page("44-46".into());
    for source in &sources {
        for locator in [None, Some(&page)] {
            assert_eq!(
                parenthetical_together(&[cite(source, locator)], &d),
                in_text(source, &d, Form::Parenthetical, locator)
            );
        }
    }
}

#[test]
fn a_group_is_cited_by_its_short_form_once_it_is_introduced() {
    // APA 7 §8.21. Ordered by the full name all the same: Wilson before the World Health
    // Organization, as the reference list has them.
    let report = who(2019, "Protein intake");
    let wilson = article("Iron", 2021, vec![author("Wilson", "Wendy")]);
    let sources = vec![report.clone(), wilson.clone()];
    let d = Disambiguation::compute(&sources);
    let later = Cite {
        source: &report,
        locator: None,
        introduced: true,
    };
    assert_eq!(in_text_cite(later, &d, Form::Parenthetical), "(WHO, 2019)");
    assert_eq!(in_text_cite(later, &d, Form::Narrative), "WHO (2019)");
    assert_eq!(
        in_text_cite(cite(&report, None), &d, Form::Narrative),
        "World Health Organization (WHO, 2019)"
    );
    assert_eq!(
        parenthetical_together(&[later, cite(&wilson, None)], &d),
        "(Wilson, 2021; WHO, 2019)"
    );
    assert_eq!(
        parenthetical_together(&[cite(&report, None), cite(&wilson, None)], &d),
        "(Wilson, 2021; World Health Organization [WHO], 2019)"
    );
}

// ---- locators ------------------------------------------------------------------

#[test]
fn locators_are_labelled_by_kind_and_ranges_take_an_en_dash() {
    let s = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    let sources = vec![s.clone()];
    let d = Disambiguation::compute(&sources);

    let with = |l: Locator| in_text(&s, &d, Form::Parenthetical, Some(&l));

    assert_eq!(with(Locator::Page("44".into())), "(Nieminen, 2021, p. 44)");
    assert_eq!(
        with(Locator::Page("44-46".into())),
        "(Nieminen, 2021, pp. 44\u{2013}46)"
    );
    assert_eq!(
        with(Locator::Paragraph("3".into())),
        "(Nieminen, 2021, para. 3)"
    );
    assert_eq!(
        with(Locator::Chapter("2".into())),
        "(Nieminen, 2021, Chapter 2)"
    );
    assert_eq!(
        with(Locator::Timestamp("1:14:22".into())),
        "(Nieminen, 2021, 1:14:22)"
    );
}

// ---- disambiguation: none of this is visible from one citation -----------------

#[test]
fn same_authors_same_year_get_letters_ordered_by_title() {
    let first = article("Amino acids", 2021, vec![author("Nieminen", "Anna")]);
    let second = article("Zinc and protein", 2021, vec![author("Nieminen", "Anna")]);
    let sources = vec![second.clone(), first.clone()]; // deliberately out of order
    let d = Disambiguation::compute(&sources);

    // Letters follow the reference-list title order, not insertion order.
    assert_eq!(
        in_text(&first, &d, Form::Parenthetical, None),
        "(Nieminen, 2021a)"
    );
    assert_eq!(
        in_text(&second, &d, Form::Parenthetical, None),
        "(Nieminen, 2021b)"
    );
}

#[test]
fn letters_follow_titles_without_their_article() {
    // "The amino story" files under A, so it is the 2021a — under T it would be the b.
    let the = article("The amino story", 2021, vec![author("Nieminen", "Anna")]);
    let basics = article("Basics of protein", 2021, vec![author("Nieminen", "Anna")]);
    let sources = vec![basics.clone(), the.clone()];
    let d = Disambiguation::compute(&sources);
    assert_eq!(
        in_text(&the, &d, Form::Parenthetical, None),
        "(Nieminen, 2021a)"
    );
    assert_eq!(
        in_text(&basics, &d, Form::Parenthetical, None),
        "(Nieminen, 2021b)"
    );
    let list = reference_list(&sources, &d);
    assert!(list[0].contains("(2021a). The amino story."), "{}", list[0]);
}

#[test]
fn undated_works_by_one_author_take_their_letter_after_a_hyphen() {
    // "n.d.a" reads as a longer abbreviation. APA 7 writes n.d.-a.
    let mut first = article("Amino acids", 2021, vec![author("Nieminen", "Anna")]);
    first.date = Date::NoDate;
    let mut second = article("Zinc", 2021, vec![author("Nieminen", "Anna")]);
    second.date = Date::NoDate;
    let sources = vec![second.clone(), first.clone()];
    let d = Disambiguation::compute(&sources);
    assert_eq!(
        in_text(&first, &d, Form::Parenthetical, None),
        "(Nieminen, n.d.-a)"
    );
    assert_eq!(
        in_text(&second, &d, Form::Narrative, None),
        "Nieminen (n.d.-b)"
    );
    let list = reference_list(&sources, &d);
    assert!(list[0].starts_with("Nieminen, A. (n.d.-a)."), "{}", list[0]);
}

#[test]
fn two_authors_sharing_a_surname_are_told_apart_by_initials() {
    let anna = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    let jussi = article("Iron", 2019, vec![author("Nieminen", "Jussi")]);
    let sources = vec![anna.clone(), jussi.clone()];
    let d = Disambiguation::compute(&sources);

    assert_eq!(
        in_text(&anna, &d, Form::Parenthetical, None),
        "(A. Nieminen, 2021)"
    );
    assert_eq!(
        in_text(&jussi, &d, Form::Parenthetical, None),
        "(J. Nieminen, 2019)"
    );
}

#[test]
fn one_author_alone_keeps_their_surname_unadorned() {
    let anna = article("Protein", 2021, vec![author("Nieminen", "Anna")]);
    let other = article("Iron", 2019, vec![author("Kallio", "Kai")]);
    let sources = vec![anna.clone(), other];
    let d = Disambiguation::compute(&sources);
    assert_eq!(
        in_text(&anna, &d, Form::Parenthetical, None),
        "(Nieminen, 2021)"
    );
}

#[test]
fn works_that_would_shorten_alike_expand_until_they_differ() {
    // The trap in the spec: both shorten to "(Chen et al., 2020)" and a reader cannot tell
    // them apart. Neither can be fixed by looking at either one alone.
    let wang = article(
        "Protein A",
        2020,
        vec![
            author("Chen", "Li"),
            author("Wang", "Wei"),
            author("Zhao", "Zi"),
        ],
    );
    let liu = article(
        "Protein B",
        2020,
        vec![
            author("Chen", "Li"),
            author("Liu", "Lan"),
            author("Zhao", "Zi"),
        ],
    );
    let sources = vec![wang.clone(), liu.clone()];
    let d = Disambiguation::compute(&sources);

    assert_eq!(
        in_text(&wang, &d, Form::Parenthetical, None),
        "(Chen, Wang, et al., 2020)"
    );
    assert_eq!(
        in_text(&liu, &d, Form::Parenthetical, None),
        "(Chen, Liu, et al., 2020)"
    );
}

#[test]
fn works_that_already_differ_are_left_short() {
    let a = article(
        "Protein A",
        2020,
        vec![
            author("Chen", "Li"),
            author("Wang", "Wei"),
            author("Zhao", "Zi"),
        ],
    );
    let b = article(
        "Iron",
        2020,
        vec![
            author("Kallio", "Kai"),
            author("Virtanen", "Vi"),
            author("Aho", "Ari"),
        ],
    );
    let sources = vec![a.clone(), b];
    let d = Disambiguation::compute(&sources);
    assert_eq!(
        in_text(&a, &d, Form::Parenthetical, None),
        "(Chen et al., 2020)"
    );
}

// ---- reference entries, per type -----------------------------------------------

fn entry(source: &Source) -> String {
    let sources = vec![source.clone()];
    reference_entry(source, &Disambiguation::compute(&sources))
}

#[test]
fn a_journal_article_entry() {
    let mut s = article(
        "Protein requirements in adults",
        2021,
        vec![author("Nieminen", "Anna")],
    );
    s.issue = Some("4".into());
    s.pages = Some("612-620".into());
    s.doi = Some("10.1000/example".into());

    assert_eq!(
        entry(&s),
        "Nieminen, A. (2021). Protein requirements in adults. *Journal of Nutrition*, *151*(4), \
         612\u{2013}620. https://doi.org/10.1000/example"
    );
}

#[test]
fn a_book_entry() {
    let mut s = Source::new(SourceType::Book, "Protein and the body", Date::Year(2021));
    s.contributors = vec![author("Nieminen", "Anna"), author("Kallio", "Kai")];
    s.publisher = Some("Kustannus".into());
    s.edition = Some("2nd ed.".into());

    assert_eq!(
        entry(&s),
        "Nieminen, A., & Kallio, K. (2021). *Protein and the body* (2nd ed.). Kustannus."
    );
}

#[test]
fn a_book_chapter_entry_names_its_editors() {
    let mut s = Source::new(SourceType::BookChapter, "Amino acids", Date::Year(2021));
    s.contributors = vec![
        author("Nieminen", "Anna"),
        Contributor::person(Role::Editor, "Kallio", "Kai"),
    ];
    s.container = Some("Nutrition handbook".into());
    s.pages = Some("44-46".into());
    s.publisher = Some("Kustannus".into());

    assert_eq!(
        entry(&s),
        "Nieminen, A. (2021). Amino acids. In K. Kallio (Ed.), *Nutrition handbook* \
         (pp. 44\u{2013}46). Kustannus."
    );
}

#[test]
fn a_web_page_entry_carries_a_full_date() {
    let mut s = Source::new(
        SourceType::WebPage,
        "Protein basics",
        Date::Full(2021, 9, 6),
    );
    s.contributors = vec![author("Nieminen", "Anna")];
    s.site = Some("Terveyskirjasto".into());
    s.url = Some("https://example.org/protein".into());

    // The page's title in italics: a web page stands alone, as a book does (APA 7 §10.16).
    assert_eq!(
        entry(&s),
        "Nieminen, A. (2021, September 6). *Protein basics*. Terveyskirjasto. \
         https://example.org/protein"
    );
}

#[test]
fn a_retrieval_date_appears_only_when_the_source_carries_one() {
    let mut s = Source::new(SourceType::WebPage, "Live figures", Date::Full(2021, 9, 6));
    s.contributors = vec![author("Nieminen", "Anna")];
    s.site = Some("Statistics".into());
    s.url = Some("https://example.org/live".into());

    assert!(
        !entry(&s).contains("Retrieved"),
        "most pages must not carry one"
    );

    s.retrieved = Some(Date::Full(2026, 9, 7));
    assert!(
        entry(&s).contains("Retrieved September 7, 2026, from"),
        "{}",
        entry(&s)
    );
}

#[test]
fn twenty_one_authors_are_elided_rather_than_shortened_to_et_al() {
    let mut s = article("Many hands", 2021, vec![]);
    s.contributors = (0..21)
        .map(|i| author(&format!("Author{i:02}"), &format!("Given{i}")))
        .collect();

    let text = entry(&s);
    assert!(
        text.contains(". . . Author20, G."),
        "the final author must survive: {text}"
    );
    assert!(
        !text.contains("et al."),
        "et al. belongs in text, never in the reference list"
    );
    assert!(
        text.contains("Author18, G."),
        "nineteen must be listed: {text}"
    );
    assert!(
        !text.contains("Author19, G."),
        "the twentieth must be elided: {text}"
    );
}

// ---- reference list ------------------------------------------------------------

#[test]
fn the_list_is_alphabetical_then_by_year_then_by_title() {
    let sources = vec![
        article("Zinc", 2019, vec![author("Virtanen", "Vi")]),
        article("Beta", 2021, vec![author("Nieminen", "Anna")]),
        article("Alpha", 2021, vec![author("Nieminen", "Anna")]),
        article("Iron", 2005, vec![author("Aho", "Ari")]),
    ];
    let d = Disambiguation::compute(&sources);
    let list = reference_list(&sources, &d);

    let starts: Vec<&str> = list.iter().map(|e| e.split(',').next().unwrap()).collect();
    assert_eq!(starts, vec!["Aho", "Nieminen", "Nieminen", "Virtanen"]);

    // And the two by the same author in one year carry their letters here too.
    assert!(list[1].contains("(2021a)"), "{}", list[1]);
    assert!(list[2].contains("(2021b)"), "{}", list[2]);
}

#[test]
fn a_work_with_no_date_comes_before_the_dated_works_of_its_author() {
    // APA 7 §9.47: n.d. first, then the years in order.
    let dated = article("Protein", 2019, vec![author("Nieminen", "Anna")]);
    let mut undated = article("Zinc", 2019, vec![author("Nieminen", "Anna")]);
    undated.date = Date::NoDate;
    let sources = vec![dated, undated];
    let list = reference_list(&sources, &Disambiguation::compute(&sources));
    assert!(list[0].starts_with("Nieminen, A. (n.d.)."), "{}", list[0]);
    assert!(list[1].starts_with("Nieminen, A. (2019)."), "{}", list[1]);
}

#[test]
fn finnish_and_cyrillic_names_sort_without_falling_back_to_byte_order() {
    let sources = vec![
        article("Zinc", 2021, vec![author("Öhman", "Olli")]),
        article("Iron", 2021, vec![author("Aho", "Ari")]),
        article("Protein", 2021, vec![author("Никитин", "Иван")]),
    ];
    let list = reference_list(&sources, &Disambiguation::compute(&sources));
    let starts: Vec<&str> = list.iter().map(|e| e.split(',').next().unwrap()).collect();
    assert_eq!(starts, vec!["Aho", "Öhman", "Никитин"]);
}

#[test]
fn a_work_with_no_author_files_under_its_title_without_the_article() {
    // "The protein book" files under P, ahead of Smith. Filed under T it would follow him.
    let untitled = article("The protein book", 2021, vec![]);
    let smith = article("Zinc", 2021, vec![author("Smith", "Sam")]);
    let sources = vec![smith, untitled];
    let list = reference_list(&sources, &Disambiguation::compute(&sources));
    assert!(list[0].starts_with("The protein book."), "{}", list[0]);
    assert!(list[1].starts_with("Smith"), "{}", list[1]);
}

#[test]
fn a_group_author_files_under_its_full_name_and_not_its_short_form() {
    // Under "WHO" it would come before Wilson. The list spells the group out, and files it
    // where the spelled-out name falls.
    let who = {
        let mut s = article("Protein", 2007, vec![]);
        s.contributors = vec![
            Contributor::group(Role::Author, "World Health Organization").with_abbreviation("WHO"),
        ];
        s
    };
    let wilson = article("Iron", 2021, vec![author("Wilson", "Wendy")]);
    let sources = vec![who, wilson];
    let list = reference_list(&sources, &Disambiguation::compute(&sources));
    assert!(list[0].starts_with("Wilson, W."), "{}", list[0]);
    assert!(
        list[1].starts_with("World Health Organization. (2007)."),
        "{}",
        list[1]
    );
}

// ---- through a real library ----------------------------------------------------
//
// Everything above is pure logic over values. These go through files on disk, the index, and
// the API a client actually calls — because M0 taught that a layer nothing exercises is a
// layer where both sides can be correct and the product still not work.

use plc_engine::api::Engine;
use plc_engine::citation::Field;

fn library() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::create(dir.path(), "Learning").unwrap();
    (dir, engine)
}

#[test]
fn a_source_round_trips_through_a_file() {
    let (dir, mut engine) = library();
    let mut source = article(
        "Protein requirements in adults",
        2021,
        vec![author("Nieminen", "Anna")],
    );
    source.pages = Some("612-620".into());
    source.doi = Some("10.1000/example".into());

    let (id, missing) = engine.source_create(source.clone()).unwrap();
    assert!(missing.is_empty(), "this source is complete: {missing:?}");

    // On disk, readable, in the flat sources directory.
    let path = dir.path().join("sources/nieminen-2021.toml");
    assert!(path.is_file(), "expected {}", path.display());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("title = \"Protein requirements in adults\""),
        "{text}"
    );

    assert_eq!(engine.source_read(id).unwrap(), source);
}

#[test]
fn an_incomplete_source_is_saved_and_reported_rather_than_refused() {
    // The rule that decides whether somebody keeps a source they cannot fully describe.
    let (_dir, mut engine) = library();
    let mut source = Source::new(SourceType::Book, "Half-remembered book", Date::Year(2021));
    source.contributors = vec![author("Nieminen", "Anna")];
    // no publisher

    let (id, missing) = engine.source_create(source).unwrap();
    assert_eq!(missing, vec![Field::Publisher]);

    // Saved, readable, and citable despite the gap.
    assert!(engine.source_read(id).is_ok());
    assert!(engine
        .citation_format(id, Form::Parenthetical, None)
        .unwrap()
        .contains("Nieminen"));
}

#[test]
fn an_episode_with_a_host_and_no_author_is_not_missing_its_author() {
    // APA puts a podcast's host where the author goes, so a host is all an episode needs.
    let mut episode = Source::new(SourceType::Audio, "Amusement park", Date::Year(2011));
    episode.container = Some("This American Life".into());
    episode.contributors = vec![Contributor::person(Role::Host, "Glass", "Ira")];
    assert_eq!(episode.missing_fields(), vec![]);

    episode.contributors.clear();
    assert_eq!(episode.missing_fields(), vec![Field::Authors]);
}

#[test]
fn citing_a_source_from_a_document_builds_its_reference_list() {
    let (_dir, mut engine) = library();

    let mut nieminen = article(
        "Protein requirements in adults",
        2021,
        vec![author("Nieminen", "Anna")],
    );
    nieminen.pages = Some("612-620".into());
    let (cited, _) = engine.source_create(nieminen).unwrap();

    let uncited = article("Something else entirely", 2019, vec![author("Aho", "Ari")]);
    engine.source_create(uncited).unwrap();

    let topic = engine.topic_create(None, "Protein").unwrap();
    let doc = engine.document_create(topic, "What protein is").unwrap();
    let revision = engine.document_read(doc).unwrap().revision;
    engine
        .document_write(
            doc,
            format!("Nine essential amino acids.[^cite:{cited}@44-46]"),
            revision,
        )
        .unwrap();

    let list = engine.citation_reference_list(doc).unwrap();
    assert_eq!(
        list.len(),
        1,
        "only what the document actually cites: {list:?}"
    );
    assert!(list[0].starts_with("Nieminen, A. (2021)."), "{}", list[0]);
}

#[test]
fn a_source_knows_which_documents_lean_on_it() {
    // A study gets retracted. This is how you find the places you built on it.
    let (_dir, mut engine) = library();
    let (source, _) = engine
        .source_create(article("Protein", 2021, vec![author("Nieminen", "Anna")]))
        .unwrap();

    let topic = engine.topic_create(None, "Protein").unwrap();
    for title in ["First", "Second"] {
        let doc = engine.document_create(topic, title).unwrap();
        let revision = engine.document_read(doc).unwrap().revision;
        engine
            .document_write(doc, format!("Claimed.[^cite:{source}]"), revision)
            .unwrap();
    }

    let citing = engine.source_citing_documents(source).unwrap();
    let mut titles: Vec<String> = citing.into_iter().map(|d| d.title).collect();
    titles.sort();
    assert_eq!(titles, vec!["First", "Second"]);

    // And deleting is two-phase: the plan names them before anything is removed.
    assert_eq!(engine.source_delete_plan(source).unwrap().len(), 2);
}

#[test]
fn disambiguation_is_computed_across_the_library_not_the_document() {
    // Two sources by the same author in the same year. The letters must be identical no
    // matter which document is asking, which is only true if the pass sees the whole library.
    let (_dir, mut engine) = library();
    let (first, _) = engine
        .source_create(article(
            "Amino acids",
            2021,
            vec![author("Nieminen", "Anna")],
        ))
        .unwrap();
    let (second, _) = engine
        .source_create(article(
            "Zinc and protein",
            2021,
            vec![author("Nieminen", "Anna")],
        ))
        .unwrap();

    assert_eq!(
        engine
            .citation_format(first, Form::Parenthetical, None)
            .unwrap(),
        "(Nieminen, 2021a)"
    );
    assert_eq!(
        engine
            .citation_format(second, Form::Parenthetical, None)
            .unwrap(),
        "(Nieminen, 2021b)"
    );
}

#[test]
fn sources_survive_the_index_being_deleted() {
    // The property M0 established, now with sources in the library too.
    let (dir, mut engine) = library();
    let (id, _) = engine
        .source_create(article("Protein", 2021, vec![author("Nieminen", "Anna")]))
        .unwrap();
    let before = engine.source_read(id).unwrap();
    let cited_before = engine
        .citation_format(id, Form::Parenthetical, None)
        .unwrap();

    std::fs::remove_dir_all(dir.path().join(".plc")).unwrap();
    let engine = Engine::open(dir.path()).unwrap();

    assert_eq!(engine.source_read(id).unwrap(), before);
    assert_eq!(
        engine
            .citation_format(id, Form::Parenthetical, None)
            .unwrap(),
        cited_before
    );
}

#[test]
fn a_source_is_findable_by_its_author() {
    let (_dir, mut engine) = library();
    engine
        .source_create(article(
            "Protein requirements",
            2021,
            vec![author("Nieminen", "Anna")],
        ))
        .unwrap();

    let hits = engine.search("Nieminen").unwrap();
    assert!(
        !hits.is_empty(),
        "a saved source must be findable by who wrote it"
    );
}

#[test]
fn a_source_is_findable_by_where_it_appeared_and_who_put_it_out() {
    // What someone remembers of a source saved months ago is as often the journal, the
    // organisation or the site as its title: "the WHO report", "the one in the Journal of
    // Nutrition".
    let (_dir, mut engine) = library();
    engine
        .source_create(article(
            "Protein intake in Finnish adults",
            2019,
            vec![author("Kallio", "Kari")],
        ))
        .unwrap();
    let mut report = Source::new(
        SourceType::Report,
        "Protein and amino acid requirements in human nutrition",
        Date::Year(2007),
    );
    report.contributors = vec![
        Contributor::group(Role::Author, "World Health Organization").with_abbreviation("WHO"),
    ];
    report.publisher = Some("World Health Organization".into());
    engine.source_create(report).unwrap();
    let mut book = Source::new(SourceType::Book, "Ravinto ja terveys", Date::Year(2021));
    book.contributors = vec![author("Nieminen", "Anna Maria")];
    book.publisher = Some("Otava".into());
    engine.source_create(book).unwrap();
    let mut page = Source::new(SourceType::WebPage, "Protein requirements", Date::NoDate);
    page.site = Some("Terveyskirjasto".into());
    engine.source_create(page).unwrap();

    for (query, found) in [
        ("Journal of Nutrition", "Protein intake in Finnish adults"),
        (
            "WHO",
            "Protein and amino acid requirements in human nutrition",
        ),
        ("Otava", "Ravinto ja terveys"),
        ("Terveyskirjasto", "Protein requirements"),
        // A whole name, not only the surname a reference list prints.
        ("Anna Maria", "Ravinto ja terveys"),
    ] {
        let hits: Vec<String> = engine
            .search(query)
            .unwrap()
            .into_iter()
            .map(|hit| hit.title)
            .collect();
        assert_eq!(hits, [found], "{query}");
    }
}

#[test]
fn one_author_precedes_the_same_author_with_co_authors() {
    // APA 7 §9.47. Sorting on the first author alone leaves these in title order, which puts
    // the book first and is wrong.
    let alone = article("Zzz last by title", 2021, vec![author("Nieminen", "Anna")]);
    let together = article(
        "Aaa first by title",
        2021,
        vec![author("Nieminen", "Anna"), author("Kallio", "Kai")],
    );
    let sources = vec![together.clone(), alone.clone()];
    let list = reference_list(&sources, &Disambiguation::compute(&sources));

    assert!(
        list[0].starts_with("Nieminen, A. (2021)"),
        "solo author first: {}",
        list[0]
    );
    assert!(
        list[1].starts_with("Nieminen, A., & Kallio"),
        "co-authored second: {}",
        list[1]
    );
}

#[test]
fn co_authored_works_order_by_the_second_author() {
    let with_v = article(
        "A",
        2021,
        vec![author("Nieminen", "Anna"), author("Virtanen", "Vi")],
    );
    let with_k = article(
        "B",
        2021,
        vec![author("Nieminen", "Anna"), author("Kallio", "Kai")],
    );
    let sources = vec![with_v, with_k];
    let list = reference_list(&sources, &Disambiguation::compute(&sources));
    assert!(list[0].contains("Kallio"), "{}", list[0]);
    assert!(list[1].contains("Virtanen"), "{}", list[1]);
}

// ---- the syntax as it reaches a file ------------------------------------------
//
// The parser has its own unit tests. These go the whole way: written into a document, read
// back off disk, formatted against the library. What a person typed has to survive all three.

#[test]
fn a_narrative_citation_reads_differently_from_a_parenthetical_one() {
    // The difference the syntax was grown for. Same source, same document, two sentences that
    // use it differently — and before `!narrative` existed, only one of them was expressible.
    let (_dir, mut engine) = library();
    let (id, _) = engine
        .source_create(article(
            "Protein requirements in adults",
            2021,
            vec![author("Nieminen", "Anna")],
        ))
        .unwrap();

    let topic = engine.topic_create(None, "Protein").unwrap();
    let doc = engine.document_create(topic, "What protein is").unwrap();
    let revision = engine.document_read(doc).unwrap().revision;
    engine
        .document_write(
            doc,
            format!("[^cite:{id}!narrative] found nine.[^cite:{id}@44]"),
            revision,
        )
        .unwrap();

    let body = engine.document_read(doc).unwrap().body;
    let refs = plc_engine::citation::reference::parse_all(&body);
    assert_eq!(refs.len(), 2);

    let requests: Vec<_> = refs
        .iter()
        .map(|r| (r.source, r.form, r.locator.clone()))
        .collect();
    let rendered = engine.citation_format_many(&requests).unwrap();

    assert_eq!(rendered[0].as_deref(), Some("Nieminen (2021)"));
    assert_eq!(rendered[1].as_deref(), Some("(Nieminen, 2021, p. 44)"));
}

#[test]
fn a_timestamp_cites_as_a_timestamp_and_not_as_a_page() {
    // Before locator kinds, `@1:04:22` was stored the same way `@44` was, so a lecture cited at
    // an hour in would have rendered as "p. 1:04:22".
    let (_dir, mut engine) = library();
    let mut talk = Source::new(SourceType::Video, "How muscle repairs", Date::Year(2020));
    talk.contributors = vec![author("Chen", "Wei")];
    talk.site = Some("Aalto University".into());
    let (id, _) = engine.source_create(talk).unwrap();

    let refs = plc_engine::citation::reference::parse_all(&format!("[^cite:{id}@time:1:04:22]"));
    let requests: Vec<_> = refs
        .iter()
        .map(|r| (r.source, r.form, r.locator.clone()))
        .collect();

    let rendered = engine.citation_format_many(&requests).unwrap();
    assert_eq!(rendered[0].as_deref(), Some("(Chen, 2020, 1:04:22)"));
}

#[test]
fn formatting_many_disambiguates_across_the_whole_library() {
    // The property the batch call has to keep. Two works by the same author in the same year
    // are 2021a and 2021b — a fact about the library, not about either citation, so a batch
    // that formatted each in isolation would silently drop the suffixes.
    let (_dir, mut engine) = library();
    let (first, _) = engine
        .source_create(article(
            "Amino acids in adults",
            2021,
            vec![author("Nieminen", "Anna")],
        ))
        .unwrap();
    let (second, _) = engine
        .source_create(article(
            "Zinc in adults",
            2021,
            vec![author("Nieminen", "Anna")],
        ))
        .unwrap();

    let rendered = engine
        .citation_format_many(&[
            (first, Form::Parenthetical, None),
            (second, Form::Parenthetical, None),
        ])
        .unwrap();

    assert_eq!(rendered[0].as_deref(), Some("(Nieminen, 2021a)"));
    assert_eq!(rendered[1].as_deref(), Some("(Nieminen, 2021b)"));
}

#[test]
fn a_deleted_source_leaves_a_hole_rather_than_blanking_the_document() {
    // One citation pointing nowhere must not take the other fourteen down with it. The hole is
    // exactly what the reader needs to see.
    let (_dir, mut engine) = library();
    let (alive, _) = engine
        .source_create(article("Still here", 2021, vec![author("Aho", "Ari")]))
        .unwrap();
    let gone: plc_engine::Id = "01J8ZQ8D4Q6S8W0W2Y4A6C8E0G".parse().unwrap();

    let rendered = engine
        .citation_format_many(&[
            (gone, Form::Parenthetical, None),
            (alive, Form::Parenthetical, None),
        ])
        .unwrap();

    assert_eq!(rendered[0], None);
    assert!(rendered[1].is_some());
}

#[test]
fn a_document_citing_one_source_eleven_times_lists_it_once() {
    let (_dir, mut engine) = library();
    let (id, _) = engine
        .source_create(article("Once", 2021, vec![author("Aho", "Ari")]))
        .unwrap();

    let topic = engine.topic_create(None, "Protein").unwrap();
    let doc = engine.document_create(topic, "Repetitive").unwrap();
    let revision = engine.document_read(doc).unwrap().revision;
    let body = (0..11)
        .map(|n| format!("Point {n}.[^cite:{id}@{n}]"))
        .collect::<Vec<_>>()
        .join(" ");
    engine.document_write(doc, body, revision).unwrap();

    assert_eq!(engine.citation_reference_list(doc).unwrap().len(), 1);
}
