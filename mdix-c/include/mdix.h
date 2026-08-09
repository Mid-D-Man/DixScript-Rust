/**
 * mdix.h — DixScript C API
 *
 * Link against the platform library:
 *   Linux:   libmdix_ffi.so
 *   macOS:   libmdix_ffi.dylib
 *   Windows: mdix_ffi.dll  (import lib: mdix_ffi.lib)
 *
 * All char* returns must be freed with mdix_free_string().
 * All handles must be freed with mdix_free(), mdix_builder_free(), or
 * mdix_watcher_free() (matching whichever created them).
 * Passing NULL for handles or paths is safe — returns a sentinel and
 * sets the last-error string.
 *
 * Error-reporting note: several functions below return `bool false` or
 * `NULL` for two different reasons — a legitimate negative result (e.g.
 * "unchanged", "invalid") AND an actual error (bad handle, I/O failure,
 * parse failure). Where that ambiguity exists it's called out on the
 * function; call mdix_get_last_error() to tell them apart — it returns
 * NULL when there was no error.
 */

#ifndef MDIX_H
#define MDIX_H

#include <stdint.h>
#include <stdbool.h>

#if defined(_WIN32) || defined(_WIN64)
  #ifdef MDIX_BUILD_DLL
    #define MDIX_API __declspec(dllexport)
  #else
    #define MDIX_API __declspec(dllimport)
  #endif
#elif defined(__GNUC__) || defined(__clang__)
  #define MDIX_API __attribute__((visibility("default")))
#else
  #define MDIX_API
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ── Type discriminants ───────────────────────────────────────────────── */

/*
 * Numeric types are contiguous: Int=2, Long=3, Float=4, Double=5.
 * These values MUST match mdix-ffi/src/lib.rs's MdixType exactly —
 * this enum is hand-maintained (not cbindgen-generated), so it does
 * NOT auto-correct when the Rust side changes. Update both together.
 */
typedef enum MdixType {
    MDIX_TYPE_UNKNOWN   = -1,
    MDIX_TYPE_NULL      =  0,
    MDIX_TYPE_BOOL      =  1,
    MDIX_TYPE_INT       =  2,
    MDIX_TYPE_LONG      =  3,
    MDIX_TYPE_FLOAT     =  4,
    MDIX_TYPE_DOUBLE    =  5,
    MDIX_TYPE_STRING    =  6,
    MDIX_TYPE_DATE      =  7,
    MDIX_TYPE_TIMESTAMP =  8,
    MDIX_TYPE_HEX_COLOR =  9,
    MDIX_TYPE_BLOB      = 10,
    MDIX_TYPE_REGEX     = 11,
    MDIX_TYPE_ARRAY     = 12,
    MDIX_TYPE_OBJECT    = 13,
    MDIX_TYPE_TUPLE     = 14,
    MDIX_TYPE_ENUM      = 15
} MdixType;

typedef enum MdixFormatMode {
    MDIX_FORMAT_DEFAULT  = 0,
    MDIX_FORMAT_PRETTY   = 1,
    MDIX_FORMAT_COMPACT  = 2,
    MDIX_FORMAT_MINIFIED = 3
} MdixFormatMode;

/**
 * How to resolve a key defined by more than one source in
 * mdix_merge_sources() / mdix_merge_sources_weighted().
 * Hand-maintained — must match mdix-ffi/src/lib.rs's MdixMergeStrategy.
 */
typedef enum MdixMergeStrategy {
    /** Each source's weight decides the winner; equal weights fall back to
     *  the lower-indexed (primary) source. What mdix_merge_sources() (no
     *  explicit weights) effectively resolves to — it auto-assigns
     *  descending weights, source 0 gets 1.0, the last source gets ~0.0. */
    MDIX_MERGE_WEIGHTED_PRIORITY = 0,
    /** The lower-indexed source always wins, regardless of weight. */
    MDIX_MERGE_PRIMARY_WINS = 1,
    /** The higher-indexed source always wins, regardless of weight. */
    MDIX_MERGE_SECONDARY_WINS = 2,
    /** Any key defined by more than one source is a hard error — the merge
     *  fails outright (mdix_merge_sources* returns NULL). */
    MDIX_MERGE_THROW_ON_CONFLICT = 3
} MdixMergeStrategy;

/**
 * How to combine two array-valued entries (GroupArray, or an array-valued
 * SimpleProperty) that share a path across merge sources.
 * Hand-maintained — must match mdix-ffi/src/lib.rs's ArrayMergeStrategy.
 */
typedef enum MdixArrayMergeStrategy {
    /** The winning source's array entirely replaces the losing one's. */
    MDIX_ARRAY_MERGE_REPLACE = 0,
    /** Both arrays are concatenated, winner's items first. */
    MDIX_ARRAY_MERGE_CONCAT = 1,
    /** Concatenated (winner first), with exact-duplicate primitive values
     *  removed. Complex values (objects, nested arrays) are never deduped. */
    MDIX_ARRAY_MERGE_CONCAT_DEDUP = 2
} MdixArrayMergeStrategy;

/* ── Metadata ─────────────────────────────────────────────────────────── */

/** Static pointer — do NOT free. */
MDIX_API const char* mdix_version(void);

/** Runtime version string recorded in the loaded data itself (may differ
 *  from mdix_version() if the file was produced by a different mdix-cli).
 *  Caller must free with mdix_free_string(). NULL if handle is NULL. */
MDIX_API char* mdix_get_loaded_version(const void* handle);

/* ── Handle lifecycle — plain .mdix ──────────────────────────────────── */

MDIX_API void* mdix_load    (const char* path);
MDIX_API void* mdix_load_str(const char* source);
MDIX_API void  mdix_free    (void* handle);

/* ── Handle lifecycle — encrypted .mdix.enc ──────────────────────────── */

/** key_path may be NULL to auto-detect next to the .enc file. */
MDIX_API void* mdix_load_encrypted(
    const char* enc_path,
    const char* key_path);

MDIX_API void* mdix_load_encrypted_password(
    const char* enc_path,
    const char* password);

/** password may be NULL when using key-file mode. */
MDIX_API void* mdix_load_encrypted_bytes(
    const uint8_t* encrypted_bytes,
    int32_t        byte_count,
    const char*    key_file_content,
    const char*    password);

/* ── Validity and metadata ────────────────────────────────────────────── */

MDIX_API bool    mdix_is_valid    (const void* handle);
MDIX_API int32_t mdix_entry_count (const void* handle);
/** DLM compression flag recorded when the source was loaded. False (not an error) if handle is NULL. */
MDIX_API bool    mdix_is_compressed(const void* handle);
/** DLM encryption flag recorded when the source was loaded. False (not an error) if handle is NULL. */
MDIX_API bool    mdix_is_encrypted (const void* handle);

/* ── Type inspection ──────────────────────────────────────────────────── */

MDIX_API MdixType mdix_get_type        (const void* handle, const char* path);
MDIX_API int32_t  mdix_get_array_length(const void* handle, const char* path);

/* ── Typed getters ────────────────────────────────────────────────────── */

/** Returned char* must be freed with mdix_free_string(). Returns NULL on failure. */
MDIX_API char*   mdix_get_string    (const void* handle, const char* path);
MDIX_API int32_t mdix_get_int       (const void* handle, const char* path);
/** Get a 64-bit integer at path. Also accepts Int values (widened without loss). */
MDIX_API int64_t mdix_get_long      (const void* handle, const char* path);
MDIX_API float   mdix_get_float     (const void* handle, const char* path);
MDIX_API double  mdix_get_double    (const void* handle, const char* path);
MDIX_API bool    mdix_get_bool      (const void* handle, const char* path);
MDIX_API char*   mdix_get_json      (const void* handle, const char* path);
MDIX_API char*   mdix_get_enum_name (const void* handle, const char* path);
MDIX_API char*   mdix_get_enum_field(const void* handle, const char* path);

/** Reads a key from the loaded @CONFIG section (e.g. "version", "author",
 *  "debug_mode" — all @CONFIG values are strings). Caller must free with
 *  mdix_free_string(). Returns NULL if the key isn't set or handle is NULL. */
MDIX_API char* mdix_get_config_value(const void* handle, const char* key);

/* ── Key existence and enumeration ───────────────────────────────────── */

MDIX_API bool mdix_exists(const void* handle, const char* path);

/**
 * Returns a heap-allocated array of null-terminated strings.
 * *out_count receives the count. prefix may be NULL or "" for top-level keys.
 * Free the result with mdix_free_string_array(result, *out_count).
 */
MDIX_API char** mdix_get_keys(
    const void* handle,
    const char* prefix,
    int32_t*    out_count);

/**
 * Like mdix_get_keys, but every key in the entire flattened data set
 * (recursive — not just direct children of a prefix). *out_count receives
 * the count. Free the result with mdix_free_string_array(result, *out_count).
 */
MDIX_API char** mdix_get_all_keys(
    const void* handle,
    int32_t*    out_count);

/* ── Query ────────────────────────────────────────────────────────────── */

/**
 * Sibling-path glob query (whole-segment `*` only, e.g. "levels.*.enemies")
 * — every value matching the pattern across paths that share structure,
 * gathered via dixscript::Runtime::DixData::select_many. Returns a JSON
 * array of the matched values (caller must free with mdix_free_string()).
 * For a single array/value at one fixed path, mdix_get_json() already
 * covers it — this is specifically for the wildcarded, multi-path case.
 */
MDIX_API char* mdix_select_many_as_json(const void* handle, const char* pattern);

/* ── Validation ───────────────────────────────────────────────────────── */

/**
 * Parses `source` and reports only whether it's syntactically valid
 * DixScript — this is NOT schema validation against expected fields/types,
 * just "does it parse". False on either a real parse failure or a NULL/
 * empty source — check mdix_get_last_error() to tell them apart.
 */
MDIX_API bool mdix_validate(const char* source);

/* ── Memory management ────────────────────────────────────────────────── */

MDIX_API void mdix_free_string      (char*  s);
MDIX_API void mdix_free_string_array(char** arr, int32_t count);

/* ── Error reporting ──────────────────────────────────────────────────── */

/** Valid until the next FFI call. Do NOT free. Returns NULL when no error. */
MDIX_API const char* mdix_get_last_error(void);
MDIX_API void        mdix_clear_error   (void);

/* ── Conversion — export ──────────────────────────────────────────────── */

MDIX_API char* mdix_to_json(const void* handle, bool indented);
MDIX_API char* mdix_to_toml(const void* handle);
/** Returns a char* cast as void* — free with mdix_free_string(). */
MDIX_API void* mdix_to_mdix(const void* handle, MdixFormatMode mode);

/* ── Conversion — source text formatting ─────────────────────────────── */

MDIX_API char* mdix_format_source (const char* source, MdixFormatMode mode);
MDIX_API char* mdix_minify_source (const char* source);
/** Removes blank/redundant whitespace without touching comments or overall structure — see mdix_minify_source for the more aggressive pass. */
MDIX_API char* mdix_compact_source(const char* source);
/** Strips line and block comments, leaving formatting otherwise untouched. */
MDIX_API char* mdix_strip_comments(const char* source);

/* ── Conversion — foreign format import ──────────────────────────────── */

/** Returned handle must be freed with mdix_free(). */
MDIX_API void* mdix_from_json(const char* source);
MDIX_API void* mdix_from_toml(const char* source);

/* ── Merge — weighted AST-level merge of multiple sources ────────────── */

/**
 * Merges `count` DixScript source strings with auto-descending weights
 * (source 0 highest) and the given strategies. On success, returns a new
 * read handle (free with mdix_free()) AND writes a JSON conflict report to
 * *out_conflicts_json (caller must free with mdix_free_string(); an empty
 * "[]" means no key was defined by more than one source) — pass NULL for
 * out_conflicts_json to skip the report. Returns NULL on failure (a source
 * that fails to parse, or MDIX_MERGE_THROW_ON_CONFLICT hitting an actual
 * conflict) — check mdix_get_last_error() for why.
 */
MDIX_API void* mdix_merge_sources(
    const char* const*     sources,
    int32_t                count,
    MdixMergeStrategy      strategy,
    MdixArrayMergeStrategy array_strategy,
    char**                 out_conflicts_json);

/**
 * As mdix_merge_sources, but with explicit per-source weights — `weights`
 * must have exactly `count` entries, one per source, in the same order.
 */
MDIX_API void* mdix_merge_sources_weighted(
    const char* const*     sources,
    const double*          weights,
    int32_t                count,
    MdixMergeStrategy      strategy,
    MdixArrayMergeStrategy array_strategy,
    char**                 out_conflicts_json);

/* ── Hot reload — poll-based file watching ────────────────────────────── */

/**
 * Watches a single plaintext `.mdix` path via dixscript::Runtime::
 * HotReloadWatcher — a cheap stat()-based poll, not an OS filesystem-event
 * subscription (see hot_reload.rs's own doc comment: no notify/inotify/
 * FSEvents dependency, identical behavior on every platform this ships to).
 * Cheap enough to call mdix_watcher_check_and_reload from a game loop /
 * timer tick every frame. Does not read the file yet — the first
 * mdix_watcher_has_changed/check_and_reload call always reports a change.
 * Returns an opaque handle, or NULL on failure (NULL/invalid path).
 * Caller must free with mdix_watcher_free(). Encrypted .mdix files are
 * NOT supported here — HotReloadWatcher::force_reload() always reloads
 * through the plaintext loader path, a core Runtime limitation.
 */
MDIX_API void* mdix_watcher_new(const char* path);

MDIX_API void  mdix_watcher_free(void* watcher);

/** Caller must free with mdix_free_string(). NULL if watcher is NULL. */
MDIX_API char* mdix_watcher_path(const void* watcher);

/** True once a successful reload has happened at least once. */
MDIX_API bool  mdix_watcher_has_loaded(const void* watcher);

/** Checks the file's modified-time without reloading. False means either
 *  "unchanged" or "error" (bad handle, file missing) — check
 *  mdix_get_last_error() to tell them apart. */
MDIX_API bool  mdix_watcher_has_changed(const void* watcher);

/** Reloads only if the file changed since the last successful reload (or
 *  since construction, on the first call). Returns a new read handle (free
 *  with mdix_free()) on a successful reload, or NULL when unchanged OR on
 *  error — check mdix_get_last_error() to tell them apart. On a reload
 *  failure the watcher's internal modified-time stamp is NOT updated, so
 *  the next call retries against the same file state rather than silently
 *  giving up on that change. */
MDIX_API void* mdix_watcher_check_and_reload(void* watcher);

/** Reloads unconditionally. Returns a new read handle (free with
 *  mdix_free()), or NULL on failure. */
MDIX_API void* mdix_watcher_force_reload(void* watcher);

/* ── Builder — lifecycle ──────────────────────────────────────────────── */

MDIX_API void*   mdix_builder_new         (void);
/** Creates a builder pre-populated with `handle`'s root-level values — for
 *  round-trip editing of an already-loaded file (load → modify a few keys
 *  → save), rather than rebuilding one from scratch. Synthetic indexed
 *  children (tags[0], server.host, ...) are already stripped; only
 *  aggregate/root values that map back to valid .mdix identifiers carry
 *  over. Returns NULL if handle is NULL. */
MDIX_API void*   mdix_builder_from_handle (const void* handle);
MDIX_API void    mdix_builder_free        (void* builder);
MDIX_API int32_t mdix_builder_entry_count (const void* builder);
MDIX_API bool    mdix_builder_clear       (void* builder);

/* ── Builder — write ──────────────────────────────────────────────────── */

MDIX_API bool mdix_builder_set_string(void* builder, const char* path, const char* value);
MDIX_API bool mdix_builder_set_int   (void* builder, const char* path, int32_t value);
MDIX_API bool mdix_builder_set_long  (void* builder, const char* path, int64_t value);
MDIX_API bool mdix_builder_set_float (void* builder, const char* path, float value);
MDIX_API bool mdix_builder_set_double(void* builder, const char* path, double value);
MDIX_API bool mdix_builder_set_bool  (void* builder, const char* path, bool value);
MDIX_API bool mdix_builder_remove    (void* builder, const char* path);

/* ── Builder — read back ──────────────────────────────────────────────── */

MDIX_API bool    mdix_builder_has_key   (const void* builder, const char* path);
MDIX_API char*   mdix_builder_get_string(const void* builder, const char* path);
MDIX_API int32_t mdix_builder_get_int   (const void* builder, const char* path);
MDIX_API int64_t mdix_builder_get_long  (const void* builder, const char* path);
MDIX_API float   mdix_builder_get_float (const void* builder, const char* path);
MDIX_API double  mdix_builder_get_double(const void* builder, const char* path);
MDIX_API bool    mdix_builder_get_bool  (const void* builder, const char* path);

/* ── Builder — persistence ────────────────────────────────────────────── */

/** Returned char* must be freed with mdix_free_string(). */
MDIX_API char* mdix_builder_to_string(const void* builder);
MDIX_API bool  mdix_builder_save     (const void* builder, const char* path);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* MDIX_H */
