//! Converting a format 1 library to format 2 — ADR 0009, `storage-format.md`.
//!
//! Format 1 has topics, folders holding `topic.toml` and no writing of their own. Format 2 has
//! only pages: every topic becomes a page beside its folder, keeping the topic's id, name and
//! date, with nothing written in it yet, and the folder holds its sub-pages as before.
//!
//! The format's promise is kept to the letter: nothing is converted until a full copy of the
//! library has been written beside it, and `format_version` changes last, so a conversion that
//! stops part way leaves a library that still says format 1 and can simply be converted again.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, IoContext, Result};
use crate::library::{self, Library, WORK_DIR};
use crate::model::document::Meta;
use crate::model::{frontmatter, topic, Document, Topic};
use crate::store::{slug, ORDER_FILE, TOPICS_DIR};

/// The format this converts from.
pub const FROM: u32 = 1;

/// Convert the format 1 library at `root` to the current format, after copying it whole to a
/// folder beside it. Returns where the copy is.
///
/// Every `topic.toml` is read before anything is written, so a library holding one that cannot
/// be read is refused untouched, naming the file, rather than converted half way.
pub fn migrate(root: &Path) -> Result<PathBuf> {
    let library = Library::open_read_only(root)?;
    let found = library.manifest().format_version;
    if found != FROM {
        return Err(Error::invalid(
            "format",
            format!("only a format {FROM} library can be converted; this one is format {found}"),
        ));
    }

    let topics = root.join(TOPICS_DIR);
    if topics.is_dir() {
        check_topics(&topics)?;
    }

    let copy = copy_beside(root)?;
    if topics.is_dir() {
        convert_level(&topics)?;
    }
    library::set_format_version(root, library::FORMAT_VERSION)?;
    // The index describes the old shape. It would be rebuilt anyway, by its own version, but a
    // library should not carry a description of something it no longer is.
    let _ = fs::remove_dir_all(root.join(WORK_DIR));
    Ok(copy)
}

/// Read every `topic.toml` under `dir`, failing on the first that cannot be read.
fn check_topics(dir: &Path) -> Result<()> {
    for entry in sorted_entries(dir)? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join(topic::MANIFEST).is_file() {
            read_topic(&path)?;
        }
        check_topics(&path)?;
    }
    Ok(())
}

fn read_topic(dir: &Path) -> Result<Topic> {
    let file = dir.join(topic::MANIFEST);
    let text = fs::read_to_string(&file).ctx(format!("reading {}", file.display()))?;
    toml::from_str(&text).map_err(|e| {
        Error::library(format!(
            "{} cannot be read, so the library was not converted: {e}",
            file.display()
        ))
    })
}

/// The whole library, copied to a new folder beside it named `<folder>-format-1`, numbered if
/// that is taken. The index is left out: it is derived, and rebuilt from the files.
fn copy_beside(root: &Path) -> Result<PathBuf> {
    let root = std::path::absolute(root).ctx(format!("finding {}", root.display()))?;
    let parent = root.parent().ok_or_else(|| {
        Error::library("a library at the top of a drive cannot be copied beside itself")
    })?;
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "library".into());
    let mut taken = HashSet::new();
    for entry in fs::read_dir(parent).ctx(format!("listing {}", parent.display()))? {
        let entry = entry.ctx(format!("listing {}", parent.display()))?;
        taken.insert(entry.file_name().to_string_lossy().to_lowercase());
    }
    let copy = parent.join(slug::unique(&format!("{name}-format-{FROM}"), &taken));
    copy_tree(&root, &copy)?;
    Ok(copy)
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).ctx(format!("creating {}", to.display()))?;
    for entry in fs::read_dir(from).ctx(format!("listing {}", from.display()))? {
        let entry = entry.ctx(format!("listing {}", from.display()))?;
        let name = entry.file_name();
        if name == WORK_DIR {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        if entry.file_type().ctx("reading a directory entry")?.is_dir() {
            copy_tree(&source, &target)?;
        } else {
            fs::copy(&source, &target).ctx(format!("copying {}", source.display()))?;
        }
    }
    Ok(())
}

/// Convert every topic directly in `dir`, and write the order of that level: the documents
/// first, then the topics, each by file name — the order the sidebar showed them in.
fn convert_level(dir: &Path) -> Result<Vec<String>> {
    let mut documents = Vec::new();
    let mut folders = Vec::new();
    for entry in sorted_entries(dir)? {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == WORK_DIR || name == library::CONFLICTS_DIR {
            continue;
        }
        if path.is_dir() {
            folders.push(path);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Some(stem) = path.file_stem() {
                documents.push(stem.to_string_lossy().to_string());
            }
        }
    }

    let mut order = documents;
    for folder in folders {
        if folder.join(topic::MANIFEST).is_file() {
            order.push(convert_topic(&folder)?);
        } else {
            // A folder that was never a topic is the person's own; what is inside is still
            // converted, so a topic nested in it is not left behind.
            convert_level(&folder)?;
        }
    }
    // A page converted on an earlier, interrupted run is already a document here, and also
    // the topic's stem: listed once.
    let mut seen = HashSet::new();
    order.retain(|stem| seen.insert(stem.clone()));
    write_order(dir, &order)?;
    Ok(order)
}

/// Convert one topic, deepest first. Returns the file name, without `.md`, of its page.
fn convert_topic(dir: &Path) -> Result<String> {
    let topic = read_topic(dir)?;
    convert_level(dir)?;

    let parent = dir
        .parent()
        .ok_or_else(|| Error::library("a topic outside the library"))?;
    let mut stem = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut folder = dir.to_path_buf();

    let page = parent.join(format!("{stem}.md"));
    let already = page.is_file() && page_id(&page) == Some(topic.id);
    if page.exists() && !already {
        // A document already has the name the page needs. The page and its folder take the
        // next free name together, so nothing is overwritten and the two still match.
        let mut taken = HashSet::new();
        for entry in fs::read_dir(parent).ctx(format!("listing {}", parent.display()))? {
            let entry = entry.ctx(format!("listing {}", parent.display()))?;
            let name = PathBuf::from(entry.file_name());
            taken.insert(
                name.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            );
        }
        stem = slug::unique(&topic.name, &taken);
        let renamed = parent.join(&stem);
        crate::atomic::rename(dir, &renamed)?;
        folder = renamed;
    }

    let page = parent.join(format!("{stem}.md"));
    if !already {
        let document = Document {
            meta: Meta {
                id: topic.id,
                title: topic.name.clone(),
                created: topic.created,
                modified: topic.created,
            },
            body: String::new(),
        };
        let text = frontmatter::render(&document.meta, &document.body)?;
        crate::atomic::write(&page, &text)?;
    }

    // Only once the page is written does the topic's own file go.
    let manifest = folder.join(topic::MANIFEST);
    if manifest.exists() {
        crate::atomic::remove(&manifest)?;
    }
    // A topic with nothing in it becomes a page with no folder.
    if fs::read_dir(&folder)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
    {
        fs::remove_dir(&folder).ctx(format!("removing {}", folder.display()))?;
    }
    Ok(stem)
}

/// The id in a page's front matter, if it can be read.
fn page_id(page: &Path) -> Option<crate::id::Id> {
    let text = fs::read_to_string(page).ok()?;
    let (meta, _): (Meta, String) = frontmatter::parse(&text, &page.display().to_string()).ok()?;
    Some(meta.id)
}

fn write_order(dir: &Path, order: &[String]) -> Result<()> {
    if order.is_empty() {
        return Ok(());
    }
    let mut text = order.join("\n");
    text.push('\n');
    crate::atomic::write(&dir.join(ORDER_FILE), &text)
}

fn sorted_entries(dir: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .ctx(format!("listing {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .ctx(format!("listing {}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}
