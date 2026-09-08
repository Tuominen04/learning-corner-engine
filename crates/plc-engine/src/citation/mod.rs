//! APA 7 citation: the source model, the library-wide disambiguation pass, and the formatters.
//!
//! Three layers, in order of dependency, and only the last is style-specific. Adding a second
//! citation style later touches the formatters and nothing else, which is the point of keeping
//! sources as structured data rather than as formatted text.

pub mod disambiguate;
pub mod format;
pub mod source;

pub use disambiguate::Disambiguation;
pub use source::{Appraisal, Contributor, Date, Field, Role, Source, SourceType};
