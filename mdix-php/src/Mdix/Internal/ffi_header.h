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
 */

typedef signed char    int8_t;
typedef unsigned char  uint8_t;
typedef short          int16_t;
typedef unsigned short uint16_t;
typedef int            int32_t;
typedef unsigned int   uint32_t;

/* ── Type discriminants ──────────────────────────────────────────────────── */

typedef enum MdixType {
    MDIX_TYPE_UNKNOWN   = -1,
    MDIX_TYPE_NULL      =  0,
    MDIX_TYPE_BOOL      =  1,
    MDIX_TYPE_INT       =  2,
    MDIX_TYPE_FLOAT     =  3,
    MDIX_TYPE_DOUBLE    =  4,
    MDIX_TYPE_STRING    =  5,
    MDIX_TYPE_DATE      =  6,
    MDIX_TYPE_TIMESTAMP =  7,
    MDIX_TYPE_HEX_COLOR =  8,
    MDIX_TYPE_BLOB      =  9,
    MDIX_TYPE_REGEX     = 10,
    MDIX_TYPE_ARRAY     = 11,
    MDIX_TYPE_OBJECT    = 12,
    MDIX_TYPE_TUPLE     = 13,
    MDIX_TYPE_ENUM      = 14
} MdixType;

typedef enum MdixFormatMode {
    MDIX_FORMAT_MODE_DEFAULT  = 0,
    MDIX_FORMAT_MODE_PRETTY   = 1,
    MDIX_FORMAT_MODE_COMPACT  = 2,
    MDIX_FORMAT_MODE_MINIFIED = 3
} MdixFormatMode;

/* ── Metadata ────────────────────────────────────────────────────────────── */

const char* mdix_version(void);

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

/* ── Type inspection ─────────────────────────────────────────────────────── */

MdixType mdix_get_type(const void* handle, const char* path);
int32_t  mdix_get_array_length(const void* handle, const char* path);

/* ── Typed getters ───────────────────────────────────────────────────────── */

char*   mdix_get_string(const void* handle, const char* path);
int32_t mdix_get_int(const void* handle, const char* path);
float   mdix_get_float(const void* handle, const char* path);
double  mdix_get_double(const void* handle, const char* path);
bool    mdix_get_bool(const void* handle, const char* path);
char*   mdix_get_enum_name(const void* handle, const char* path);
char*   mdix_get_enum_field(const void* handle, const char* path);
char*   mdix_get_json(const void* handle, const char* path);

/* ── Key existence / enumeration ─────────────────────────────────────────── */

bool    mdix_exists(const void* handle, const char* path);
char**  mdix_get_keys(const void* handle, const char* prefix, int32_t* out_count);

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
void* mdix_from_json(const char* source);
void* mdix_from_toml(const char* source);

/* ── Builder lifecycle ───────────────────────────────────────────────────── */

void*   mdix_builder_new(void);
void    mdix_builder_free(void* builder);
int32_t mdix_builder_entry_count(const void* builder);
bool    mdix_builder_clear(void* builder);

/* ── Builder write ───────────────────────────────────────────────────────── */

bool mdix_builder_set_string(void* builder, const char* path, const char* value);
bool mdix_builder_set_int(void* builder, const char* path, int32_t value);
bool mdix_builder_set_float(void* builder, const char* path, float value);
bool mdix_builder_set_double(void* builder, const char* path, double value);
bool mdix_builder_set_bool(void* builder, const char* path, bool value);
bool mdix_builder_remove(void* builder, const char* path);

/* ── Builder read-back ───────────────────────────────────────────────────── */

bool    mdix_builder_has_key(const void* builder, const char* path);
char*   mdix_builder_get_string(const void* builder, const char* path);
int32_t mdix_builder_get_int(const void* builder, const char* path);
float   mdix_builder_get_float(const void* builder, const char* path);
double  mdix_builder_get_double(const void* builder, const char* path);
bool    mdix_builder_get_bool(const void* builder, const char* path);

/* ── Builder persistence ─────────────────────────────────────────────────── */

bool  mdix_builder_save(const void* builder, const char* path);
char* mdix_builder_to_string(const void* builder);
