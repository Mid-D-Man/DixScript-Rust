/**
 * mdix.h — DixScript C API
 *
 * Link against the platform library:
 *   Linux:   libmdix_ffi.so
 *   macOS:   libmdix_ffi.dylib
 *   Windows: mdix_ffi.dll  (import lib: mdix_ffi.lib)
 *
 * All char* returns must be freed with mdix_free_string().
 * All handles must be freed with mdix_free() or mdix_builder_free().
 * Passing NULL for handles or paths is safe — returns a sentinel and
 * sets the last-error string.
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
    MDIX_FORMAT_DEFAULT  = 0,
    MDIX_FORMAT_PRETTY   = 1,
    MDIX_FORMAT_COMPACT  = 2,
    MDIX_FORMAT_MINIFIED = 3
} MdixFormatMode;

/* ── Metadata ─────────────────────────────────────────────────────────── */

/** Static pointer — do NOT free. */
MDIX_API const char* mdix_version(void);

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

/* ── Type inspection ──────────────────────────────────────────────────── */

MDIX_API MdixType mdix_get_type        (const void* handle, const char* path);
MDIX_API int32_t  mdix_get_array_length(const void* handle, const char* path);

/* ── Typed getters ────────────────────────────────────────────────────── */

/** Returned char* must be freed with mdix_free_string(). Returns NULL on failure. */
MDIX_API char*   mdix_get_string    (const void* handle, const char* path);
MDIX_API int32_t mdix_get_int       (const void* handle, const char* path);
MDIX_API float   mdix_get_float     (const void* handle, const char* path);
MDIX_API double  mdix_get_double    (const void* handle, const char* path);
MDIX_API bool    mdix_get_bool      (const void* handle, const char* path);
MDIX_API char*   mdix_get_json      (const void* handle, const char* path);
MDIX_API char*   mdix_get_enum_name (const void* handle, const char* path);
MDIX_API char*   mdix_get_enum_field(const void* handle, const char* path);

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

MDIX_API char* mdix_format_source(const char* source, MdixFormatMode mode);
MDIX_API char* mdix_minify_source(const char* source);

/* ── Conversion — foreign format import ──────────────────────────────── */

/** Returned handle must be freed with mdix_free(). */
MDIX_API void* mdix_from_json(const char* source);
MDIX_API void* mdix_from_toml(const char* source);

/* ── Builder — lifecycle ──────────────────────────────────────────────── */

MDIX_API void*   mdix_builder_new         (void);
MDIX_API void    mdix_builder_free        (void* builder);
MDIX_API int32_t mdix_builder_entry_count (const void* builder);
MDIX_API bool    mdix_builder_clear       (void* builder);

/* ── Builder — write ──────────────────────────────────────────────────── */

MDIX_API bool mdix_builder_set_string(void* builder, const char* path, const char* value);
MDIX_API bool mdix_builder_set_int   (void* builder, const char* path, int32_t value);
MDIX_API bool mdix_builder_set_float (void* builder, const char* path, float value);
MDIX_API bool mdix_builder_set_double(void* builder, const char* path, double value);
MDIX_API bool mdix_builder_set_bool  (void* builder, const char* path, bool value);
MDIX_API bool mdix_builder_remove    (void* builder, const char* path);

/* ── Builder — read back ──────────────────────────────────────────────── */

MDIX_API bool    mdix_builder_has_key   (const void* builder, const char* path);
MDIX_API char*   mdix_builder_get_string(const void* builder, const char* path);
MDIX_API int32_t mdix_builder_get_int   (const void* builder, const char* path);
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
