/*
 * PHP FFI header for mdix_ffi.
 *
 * This is NOT the canonical cbindgen-generated header (mdix_ffi.h).
 * PHP's FFI::cdef() parser accepts a strict subset of C:
 *   - no #include, #pragma, #ifdef or complex macros
 *   - typedef enum { } Name; form required
 *   - stdint types supported natively
 *
 * Keep in sync with mdix-ffi/src/lib.rs when function signatures change.
 *
 * FIX: MdixType below was missing MDIX_TYPE_LONG entirely, shifting every
 * value from Float onward one below its real Rust discriminant (Rust's
 * MdixType has Int=2, Long=3, Float=4, Double=5, String=6, ...; this file
 * had Int=2, Float=3, Double=4, String=5, ... with no Long case at all —
 * and topped out at Enum=14 where Rust's real Enum is 15). Concretely:
 * mdix_get_type() on an actual Long value returned discriminant 3, which
 * this enum mapped to Float (silently wrong type), and on an actual Enum
 * value returned 15, which ValueType::from(15) in PHP would reject with an
 * uncaught \ValueError (no case defined that high) — every DixScript enum
 * field would crash valueTypeAt(). Fixed to match mdix-ffi/src/lib.rs's
 * MdixType exactly; see ValueType.php for the PHP-facing enum this backs.
 */

typedef signed char    int8_t;
typedef unsigned char  uint8_t;
typedef short          int16_t;
typedef unsigned short uint16_t;
typedef int            int32_t;
typedef unsigned int   uint32_t;
typedef long long       int64_t;
typedef unsigned long long uint64_t;

/* ── Type discriminants ──────────────────────────────────────────────────── */

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
    MDIX_FORMAT_MODE_DEFAULT  = 0,
    MDIX_FORMAT_MODE_PRETTY   = 1,
    MDIX_FORMAT_MODE_COMPACT  = 2,
    MDIX_FORMAT_MODE_MINIFIED = 3
} MdixFormatMode;

/**
 * How to resolve a key defined by more than one source in
 * mdix_merge_sources() / mdix_merge_sources_weighted().
 */
typedef enum MdixMergeStrategy {
    MDIX_MERGE_WEIGHTED_PRIORITY = 0,
    MDIX_MERGE_PRIMARY_WINS      = 1,
    MDIX_MERGE_SECONDARY_WINS    = 2,
    MDIX_MERGE_THROW_ON_CONFLICT = 3
} MdixMergeStrategy;

/** How to combine two array-valued entries that share a path across merge sources. */
typedef enum MdixArrayMergeStrategy {
    MDIX_ARRAY_MERGE_REPLACE      = 0,
    MDIX_ARRAY_MERGE_CONCAT       = 1,
    MDIX_ARRAY_MERGE_CONCAT_DEDUP = 2
} MdixArrayMergeStrategy;

/* ── Metadata ────────────────────────────────────────────────────────────── */

const char* mdix_version(void);
char* mdix_get_loaded_version(const void* handle);

/* ── Handle lifecycle — plain .mdix ─────────────────────────────────────── */

void* mdix_load(const char* path);
void* mdix_load_str(const char* source);
void  mdix_free(void* handle);

/* ── Handle lifecycle — encrypted ───────────────────────────────────────── */

void* mdix_load_encrypted(const char* enc_path, const char* key_path);
void* mdix_load_encrypted_password(const char* enc_path, const char* password);
void* mdix_load_encrypted_bytes(
    const uint8_t* encrypted_bytes,
    int32_t        byte_count,
    const char*    key_file_content,
    const char*    password
);

/* ── Validity / metadata ─────────────────────────────────────────────────── */

bool    mdix_is_valid(const void* handle);
int32_t mdix_entry_count(const void* handle);
bool    mdix_is_compressed(const void* handle);
bool    mdix_is_encrypted(const void* handle);

/* ── Type inspection ─────────────────────────────────────────────────────── */

MdixType mdix_get_type(const void* handle, const char* path);
int32_t  mdix_get_array_length(const void* handle, const char* path);

/* ── Typed getters ───────────────────────────────────────────────────────── */

char*   mdix_get_string(const void* handle, const char* path);
int32_t mdix_get_int(const void* handle, const char* path);
int64_t mdix_get_long(const void* handle, const char* path);
float   mdix_get_float(const void* handle, const char* path);
double  mdix_get_double(const void* handle, const char* path);
bool    mdix_get_bool(const void* handle, const char* path);
char*   mdix_get_enum_name(const void* handle, const char* path);
char*   mdix_get_enum_field(const void* handle, const char* path);
char*   mdix_get_json(const void* handle, const char* path);
char*   mdix_get_config_value(const void* handle, const char* key);

/* ── Key existence / enumeration ─────────────────────────────────────────── */

bool    mdix_exists(const void* handle, const char* path);
char**  mdix_get_keys(const void* handle, const char* prefix, int32_t* out_count);
char**  mdix_get_all_keys(const void* handle, int32_t* out_count);

/* ── Query ────────────────────────────────────────────────────────────────── */

char* mdix_select_many_as_json(const void* handle, const char* pattern);

/* ── Validation ───────────────────────────────────────────────────────────── */

bool  mdix_validate(const char* source);
/** Field-level schema check. fields_json: [{"path":..,"required":..,"type":"Int",...}, ...].
 *  Returns the errors as JSON (an empty "[]" means every field passed). */
char* mdix_schema_validate(const void* handle, const char* fields_json);

/* ── Memory management ───────────────────────────────────────────────────── */

void mdix_free_string(char* s);
void mdix_free_string_array(char** arr, int32_t count);

/* ── Error reporting ─────────────────────────────────────────────────────── */

const char* mdix_get_last_error(void);
void        mdix_clear_error(void);

/* ── Conversion — export ─────────────────────────────────────────────────── */

char* mdix_to_json(const void* handle, bool indented);
/* Note: mdix_to_mdix is declared char* here even though the Rust ABI returns
   void*; they are ABI-compatible (both pointer-sized) and we need char* to
   call mdix_free_string on the result. */
char* mdix_to_mdix(const void* handle, MdixFormatMode mode);
char* mdix_to_toml(const void* handle);
char* mdix_format_source(const char* source, MdixFormatMode mode);
char* mdix_minify_source(const char* source);
char* mdix_compact_source(const char* source);
char* mdix_strip_comments(const char* source);
void* mdix_from_json(const char* source);
void* mdix_from_toml(const char* source);

/* ── Merge ────────────────────────────────────────────────────────────────── */

void* mdix_merge_sources(
    const char* const*    sources,
    int32_t                count,
    MdixMergeStrategy      strategy,
    MdixArrayMergeStrategy array_strategy,
    char**                 out_conflicts_json
);
void* mdix_merge_sources_weighted(
    const char* const*    sources,
    const double*          weights,
    int32_t                count,
    MdixMergeStrategy      strategy,
    MdixArrayMergeStrategy array_strategy,
    char**                 out_conflicts_json
);

/* ── Hot reload ───────────────────────────────────────────────────────────── */

void* mdix_watcher_new(const char* path);
void  mdix_watcher_free(void* watcher);
char* mdix_watcher_path(const void* watcher);
bool  mdix_watcher_has_loaded(const void* watcher);
bool  mdix_watcher_has_changed(const void* watcher);
void* mdix_watcher_check_and_reload(void* watcher);
void* mdix_watcher_force_reload(void* watcher);

/* ── Builder lifecycle ───────────────────────────────────────────────────── */

void*   mdix_builder_new(void);
void*   mdix_builder_from_handle(const void* handle);
void    mdix_builder_free(void* builder);
int32_t mdix_builder_entry_count(const void* builder);
bool    mdix_builder_clear(void* builder);

/* ── Builder write ───────────────────────────────────────────────────────── */

bool mdix_builder_set_string(void* builder, const char* path, const char* value);
bool mdix_builder_set_int(void* builder, const char* path, int32_t value);
bool mdix_builder_set_long(void* builder, const char* path, int64_t value);
bool mdix_builder_set_float(void* builder, const char* path, float value);
bool mdix_builder_set_double(void* builder, const char* path, double value);
bool mdix_builder_set_bool(void* builder, const char* path, bool value);
bool mdix_builder_remove(void* builder, const char* path);

/* ── Builder read-back ───────────────────────────────────────────────────── */

bool    mdix_builder_has_key(const void* builder, const char* path);
char*   mdix_builder_get_string(const void* builder, const char* path);
int32_t mdix_builder_get_int(const void* builder, const char* path);
int64_t mdix_builder_get_long(const void* builder, const char* path);
float   mdix_builder_get_float(const void* builder, const char* path);
double  mdix_builder_get_double(const void* builder, const char* path);
bool    mdix_builder_get_bool(const void* builder, const char* path);

/* ── Builder persistence ─────────────────────────────────────────────────── */

bool  mdix_builder_save(const void* builder, const char* path);
char* mdix_builder_to_string(const void* builder);
