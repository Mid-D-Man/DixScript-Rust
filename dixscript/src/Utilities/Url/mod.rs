// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Url"
// ============================================================================
//! Hand-rolled replacement for the one thing this crate used the `url` crate
//! for: pulling the last path segment out of a cloud-import URL so it can name
//! the cache file (`CloudFileCache::get_cache_path`).
//!
//! Crate-internal (`pub(crate)`): it replaces a dependency, it is not API.

pub(crate) mod path_segments;
pub(crate) use path_segments::last_path_segment;
