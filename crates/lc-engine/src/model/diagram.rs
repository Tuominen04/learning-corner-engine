//! Diagrams: a picture somebody builds on purpose, in a file of its own (ADR 0011).
//!
//! A family tree, a historical path, a map of a network. Unlike the library graph, which is
//! computed from what is written and kept nowhere, a diagram's structure is the person's, so
//! it is kept: its boxes and its lines, in `maps/<name>.toml`.
//!
//! A box can point at a page, and then shows that page's title and opens it. A box can also
//! be only a label. Nothing about a page is copied into a diagram, so a diagram cannot go
//! stale; and a diagram never reads a page's text.
//!
//! What this module knows is the file: what it holds, and what makes it valid. Where a box is
//! drawn is the interface's — a *layered* or *timeline* diagram is laid out from its
//! structure, and only a *free* one keeps the positions it was dragged to.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::id::Id;
use crate::timestamp::Timestamp;

/// How a diagram's boxes come to be where they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// A family tree or a hierarchy, laid out in layers along the direction of its lines.
    Layered,
    /// A historical path: boxes in order of their `when`, along one axis.
    Timeline,
    /// A map arranged by hand: each box keeps the position it was dragged to.
    Free,
}

/// Which way a layered diagram runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    #[default]
    Down,
    Right,
}

/// One box on a diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// This box's own identifier, inside this diagram. Not the page's: the same page can be
    /// on one diagram twice, and a box keeps its place in the structure when its page changes.
    pub id: Id,
    /// The page this box points at, if it points at one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<Id>,
    /// What the box says. Without one, it says its page's title as it is now; with one, this
    /// wins — a box can call a page something shorter than its title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What puts the box in order on a timeline. Text, because history is not always a date:
    /// *1850*, *c. 1200*, *the late Bronze Age*. Kept whatever the layout, so changing layout
    /// loses nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Where a *free* diagram's box was dragged to. Both or neither.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
}

impl Node {
    /// A box pointing at a page.
    pub fn page(page: Id) -> Self {
        Node {
            id: Id::new(),
            page: Some(page),
            label: None,
            when: None,
            x: None,
            y: None,
        }
    }

    /// A box that is only a label.
    pub fn label(label: impl Into<String>) -> Self {
        Node {
            id: Id::new(),
            page: None,
            label: Some(label.into()),
            when: None,
            x: None,
            y: None,
        }
    }
}

/// A line from one box to another, with a direction and perhaps a word on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub from: Id,
    pub to: Id,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// A diagram, as `maps/<name>.toml` holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagram {
    pub id: Id,
    pub title: String,
    pub layout: Layout,
    #[serde(default)]
    pub direction: Direction,
    pub created: Timestamp,
    #[serde(default, rename = "node")]
    pub nodes: Vec<Node>,
    #[serde(default, rename = "line")]
    pub lines: Vec<Line>,
}

impl Diagram {
    pub fn new(title: impl Into<String>, layout: Layout) -> Self {
        Diagram {
            id: Id::new(),
            title: title.into(),
            layout,
            direction: Direction::default(),
            created: Timestamp::now(),
            nodes: Vec::new(),
            lines: Vec::new(),
        }
    }

    /// Pages this diagram's boxes point at, each once, in the order they first appear.
    pub fn pages(&self) -> Vec<Id> {
        let mut found = Vec::new();
        for page in self.nodes.iter().filter_map(|n| n.page) {
            if !found.contains(&page) {
                found.push(page);
            }
        }
        found
    }

    /// Everything a person might remember of a diagram: its title and what its boxes say.
    ///
    /// Not a box's `when`, and not a page's title — a page is found as the page it is.
    pub fn searchable_text(&self) -> String {
        let mut text = self.title.clone();
        for label in self.nodes.iter().filter_map(|n| n.label.as_deref()) {
            text.push('\n');
            text.push_str(label);
        }
        text
    }

    /// The same diagram with its words tidied: white space trimmed from the title, labels and
    /// `when`, and one that is empty once trimmed taken to be absent.
    pub fn tidied(mut self) -> Self {
        self.title = self.title.trim().to_string();
        let tidy = |text: &mut Option<String>| {
            *text = text
                .take()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty());
        };
        for node in &mut self.nodes {
            tidy(&mut node.label);
            tidy(&mut node.when);
        }
        for line in &mut self.lines {
            tidy(&mut line.label);
        }
        self
    }

    /// Whether this is a diagram that can be kept, and if not, the first thing wrong.
    ///
    /// Strict about structure, because every client draws from it: a line to a box that is
    /// not there, or a box that says nothing, has no sensible picture. Not strict about pages —
    /// a box may point at a page that has since been deleted, and must still be writable, or
    /// deleting a page would make every diagram it was on unsaveable.
    pub fn check(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            return Err(Error::invalid("title", "a diagram needs a title"));
        }
        for (at, node) in self.nodes.iter().enumerate() {
            if self.nodes[..at].iter().any(|n| n.id == node.id) {
                return Err(Error::invalid(
                    "nodes",
                    format!("two boxes have the identifier {}", node.id),
                ));
            }
            let says = node.label.as_deref().is_some_and(|l| !l.trim().is_empty());
            if node.page.is_none() && !says {
                return Err(Error::invalid(
                    "nodes",
                    "a box needs a page to point at or a label to say",
                ));
            }
            if node.x.is_some() != node.y.is_some() {
                return Err(Error::invalid(
                    "nodes",
                    "a box's position needs both x and y, or neither",
                ));
            }
            if [node.x, node.y]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite())
            {
                return Err(Error::invalid("nodes", "a box's position is not a number"));
            }
        }
        for line in &self.lines {
            for end in [line.from, line.to] {
                if !self.nodes.iter().any(|n| n.id == end) {
                    return Err(Error::invalid(
                        "lines",
                        format!("a line joins {end}, which is not a box on this diagram"),
                    ));
                }
            }
            if line.from == line.to {
                return Err(Error::invalid(
                    "lines",
                    "a line has to join two different boxes",
                ));
            }
        }
        Ok(())
    }

    /// Read a diagram's file. Anything the file holds that [`Diagram::check`] refuses is an
    /// error here too, so one hand-edited into nonsense is reported as a problem with the
    /// library and never drawn.
    pub fn parse(text: &str) -> Result<Self> {
        let diagram: Diagram = toml::from_str(text).map_err(|e| Error::library(e.to_string()))?;
        diagram.check().map_err(|e| Error::library(e.to_string()))?;
        Ok(diagram)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| Error::library(format!("cannot serialise diagram: {e}")))
    }
}

/// The address a page uses to show a diagram: `![alt](diagram:<id>)`'s `diagram:<id>`.
pub const ADDRESS: &str = "diagram:";

/// Take the `![…](diagram:<id>)` lines for `id` out of a page's text.
///
/// A line with nothing else on it goes whole, and a blank line left doubled by it goes too, so
/// the paragraphs either side stay a paragraph apart rather than two. A reference inside a
/// line of other writing goes alone, and the writing stays. Returns `None` when `body` does
/// not show the diagram at all.
pub fn without(body: &str, id: Id) -> Option<String> {
    let address = format!("({ADDRESS}{id})");
    if !body.contains(&address) {
        return None;
    }
    let shows = |line: &str| find_references(line, &address);

    let mut out: Vec<String> = Vec::new();
    let lines: Vec<&str> = body.split('\n').collect();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        let ranges = shows(line);
        if ranges.is_empty() {
            out.push(line.to_string());
            at += 1;
            continue;
        }
        let mut rest = line.to_string();
        for (start, end) in ranges.into_iter().rev() {
            rest.replace_range(start..end, "");
        }
        if rest.trim().is_empty() {
            // The whole line goes. If it sat between two blank lines, one of them goes too.
            let before_blank = out.last().is_some_and(|l| l.trim().is_empty());
            let after_blank = lines.get(at + 1).is_some_and(|l| l.trim().is_empty());
            if before_blank && after_blank {
                at += 1;
            }
        } else {
            out.push(rest);
        }
        at += 1;
    }
    Some(out.join("\n"))
}

/// Byte ranges in `line` of every `![alt](<address>)`.
fn find_references(line: &str, address: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = line[from..].find(address) {
        let close = from + at;
        let end = close + address.len();
        // `![` before it, with the alt text on this line between.
        if let Some(open) = line[..close].strip_suffix(']').and_then(|s| s.rfind('[')) {
            if line[..open].ends_with('!') {
                found.push((open - 1, end));
            }
        }
        from = end;
    }
    found
}

/// A page's text without its diagram addresses, for the search index.
///
/// `![Family tree](diagram:01J8…)` keeps the words in the brackets — the person wrote those —
/// and loses the address, which is an identifier and not a word, as `[[id]]` and `[^cite:id]`
/// are. Only an address that really is `diagram:` and an identifier goes; a parenthesis that
/// merely starts the same way is writing.
pub fn without_addresses(text: &str) -> String {
    const ID: usize = 26;
    let open = format!("({ADDRESS}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(&open) {
        let after = &rest[at + open.len()..];
        let is_address = after.get(..ID).is_some_and(|id| id.parse::<Id>().is_ok())
            && after[ID..].starts_with(')');
        if is_address {
            out.push_str(&rest[..at]);
            rest = &after[ID + 1..];
        } else {
            out.push_str(&rest[..at + open.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Diagrams a page's text shows, each once, in order.
///
/// `![alt](diagram:<id>)`: ordinary Markdown picture syntax, with the diagram's identifier for
/// an address. A line that does not read as one — an unfinished address, text that is not an
/// identifier — is not a reference, and is left for what it is.
pub fn shown_in(body: &str) -> Vec<Id> {
    let mut found = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find(&format!("]({ADDRESS}")) {
        let before = &rest[..at];
        let after = &rest[at + 2 + ADDRESS.len()..];
        rest = after;
        let is_picture = before
            .rfind('[')
            .is_some_and(|open| before[..open].ends_with('!') && !before[open..].contains('\n'));
        let Some(end) = after.find(')') else { break };
        if !is_picture {
            continue;
        }
        if let Ok(id) = after[..end].parse::<Id>() {
            if !found.contains(&id) {
                found.push(id);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Id {
        Id::new()
    }

    #[test]
    fn a_diagram_round_trips_through_its_file() {
        let mut d = Diagram::new("Family tree", Layout::Layered);
        let page = id();
        let a = Node::page(page);
        let mut b = Node::label("Not written about yet");
        b.when = Some("c. 1200".into());
        b.x = Some(12.5);
        b.y = Some(-40.0);
        d.lines.push(Line {
            from: a.id,
            to: b.id,
            label: Some("parent of".into()),
        });
        d.nodes = vec![a, b];

        let text = d.to_toml().unwrap();
        assert!(
            text.contains("[[node]]") && text.contains("[[line]]"),
            "{text}"
        );
        assert_eq!(Diagram::parse(&text).unwrap(), d);
    }

    #[test]
    fn what_is_not_set_is_not_written() {
        let mut d = Diagram::new("Plain", Layout::Free);
        d.nodes.push(Node::label("Only a label"));
        let text = d.to_toml().unwrap();
        for absent in ["page", "when", "x =", "y ="] {
            assert!(!text.contains(absent), "{absent} in {text}");
        }
    }

    #[test]
    fn it_is_refused_for_what_no_client_could_draw() {
        let refused = |d: &Diagram| matches!(d.check(), Err(Error::Invalid { .. }));
        let mut d = Diagram::new("T", Layout::Free);
        d.nodes = vec![Node::label("a"), Node::label("b")];
        assert!(!refused(&d));

        let mut untitled = d.clone();
        untitled.title = "  ".into();
        assert!(refused(&untitled));

        let mut twice = d.clone();
        twice.nodes[1].id = twice.nodes[0].id;
        assert!(refused(&twice));

        let mut says_nothing = d.clone();
        says_nothing.nodes[1].label = Some("  ".into());
        assert!(refused(&says_nothing));

        let mut half_placed = d.clone();
        half_placed.nodes[0].x = Some(1.0);
        assert!(refused(&half_placed));

        let mut not_a_number = d.clone();
        not_a_number.nodes[0].x = Some(f64::NAN);
        not_a_number.nodes[0].y = Some(0.0);
        assert!(refused(&not_a_number));

        let mut nowhere = d.clone();
        nowhere.lines.push(Line {
            from: d.nodes[0].id,
            to: id(),
            label: None,
        });
        assert!(refused(&nowhere));

        let mut loops = d.clone();
        loops.lines.push(Line {
            from: d.nodes[0].id,
            to: d.nodes[0].id,
            label: None,
        });
        assert!(refused(&loops));
    }

    #[test]
    fn a_box_may_point_at_a_page_that_is_not_there_and_two_lines_may_join_the_same_boxes() {
        let mut d = Diagram::new("T", Layout::Layered);
        d.nodes = vec![Node::page(id()), Node::label("b")];
        for label in ["mother of", "mother of"] {
            d.lines.push(Line {
                from: d.nodes[0].id,
                to: d.nodes[1].id,
                label: Some(label.into()),
            });
        }
        d.check().unwrap();
    }

    #[test]
    fn tidying_trims_words_and_drops_empty_ones() {
        let mut d = Diagram::new("  Tree  ", Layout::Timeline);
        let mut n = Node::label("  Cairo ");
        n.when = Some("   ".into());
        d.nodes.push(n);
        let d = d.tidied();
        assert_eq!(d.title, "Tree");
        assert_eq!(d.nodes[0].label.as_deref(), Some("Cairo"));
        assert_eq!(d.nodes[0].when, None);
    }

    #[test]
    fn a_file_that_is_nonsense_is_an_error_rather_than_a_diagram() {
        assert!(Diagram::parse("not toml at all [").is_err());
        let d = Diagram::new("T", Layout::Free);
        let text = d
            .to_toml()
            .unwrap()
            .replace("layout = \"free\"", "layout = \"round\"");
        assert!(Diagram::parse(&text).is_err());
        let mut broken = Diagram::new("T", Layout::Free);
        broken.nodes.push(Node::label("a"));
        broken.lines.push(Line {
            from: broken.nodes[0].id,
            to: id(),
            label: None,
        });
        assert!(Diagram::parse(&broken.to_toml().unwrap()).is_err());
    }

    #[test]
    fn pages_and_words_are_what_a_diagram_can_be_asked() {
        let (p, q) = (id(), id());
        let mut d = Diagram::new("Tree", Layout::Layered);
        d.nodes = vec![
            Node::page(p),
            Node::page(q),
            Node::page(p),
            Node::label("Cairo"),
        ];
        assert_eq!(d.pages(), vec![p, q]);
        assert_eq!(d.searchable_text(), "Tree\nCairo");
    }

    #[test]
    fn the_pages_that_show_a_diagram_are_found_by_their_picture_lines() {
        let (a, b) = (id(), id());
        let body = format!(
            "Intro.\n\n![Family tree](diagram:{a})\n\nAnd ![again](diagram:{a}) and ![other](diagram:{b}).\n\n\
             ![not a diagram](diagram:nonsense) ![a file](tree.png) [a link](diagram:{a}) ![](diagram:{b}"
        );
        assert_eq!(shown_in(&body), vec![a, b]);
        assert!(shown_in("No pictures here.").is_empty());
    }

    #[test]
    fn the_search_text_keeps_the_words_and_loses_the_address() {
        let a = id();
        let text = format!("See ![Family tree](diagram:{a}) now, (diagram: see above) too.");
        assert_eq!(
            without_addresses(&text),
            "See ![Family tree] now, (diagram: see above) too."
        );
        assert_eq!(without_addresses("Nothing."), "Nothing.");
    }

    #[test]
    fn taking_a_diagram_out_of_a_page_leaves_the_writing_and_its_spacing() {
        let (a, b) = (id(), id());
        let body = format!("Before.\n\n![Tree](diagram:{a})\n\nAfter.\n\n![Other](diagram:{b})\n");
        assert_eq!(
            without(&body, a).unwrap(),
            format!("Before.\n\nAfter.\n\n![Other](diagram:{b})\n")
        );

        // In the middle of a line of writing, only the picture goes.
        let inline = format!("See ![Tree](diagram:{a}) for the whole family.");
        assert_eq!(without(&inline, a).unwrap(), "See  for the whole family.");

        // At the very start or end, with nothing to double up.
        let first = format!("![Tree](diagram:{a})\n\nText.");
        assert_eq!(without(&first, a).unwrap(), "\nText.");
        let last = format!("Text.\n\n![Tree](diagram:{a})");
        assert_eq!(without(&last, a).unwrap(), "Text.\n");

        // Twice, and not at all.
        let twice = format!("![T](diagram:{a})\n\nMid.\n\n![T](diagram:{a})\n\nEnd.");
        assert_eq!(without(&twice, a).unwrap(), "\nMid.\n\nEnd.");
        assert_eq!(without("Nothing.", a), None);
        // A link that merely looks the same is not a picture, and stays.
        let link = format!("[T](diagram:{a})");
        assert_eq!(without(&link, a).unwrap(), link);
    }
}
