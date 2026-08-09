/**
 * mdix.hpp — DixScript C++ API (header-only, C++17)
 *
 * RAII wrapper over mdix.h. Include this instead of mdix.h for C++ projects.
 * Link against the same native library as for C.
 *
 * Quick start:
 *
 *   #include <mdix.hpp>
 *
 *   auto db = mdix::Database::load_str("@DATA( port = 8080, host = \"localhost\" )");
 *   if (!db) { std::cerr << db.error().message() << '\n'; return 1; }
 *
 *   int  port = db->get_int("port").value_or(0);
 *   auto host = db->get_string("host").value_or("?");
 *   std::cout << host << ':' << port << '\n';
 */

#pragma once

#include "mdix.h"

#include <string>
#include <string_view>
#include <optional>
#include <vector>
#include <stdexcept>
#include <cstdint>
#include <cctype>
#include <utility>

namespace mdix {

/* ── OwnedString — RAII for char* returned by the C API ──────────────── */

class OwnedString {
public:
    OwnedString() noexcept : ptr_(nullptr) {}
    explicit OwnedString(char* raw) noexcept : ptr_(raw) {}
    ~OwnedString() { reset(); }

    OwnedString(OwnedString&& o) noexcept : ptr_(o.ptr_) { o.ptr_ = nullptr; }
    OwnedString& operator=(OwnedString&& o) noexcept {
        if (this != &o) { reset(); ptr_ = o.ptr_; o.ptr_ = nullptr; }
        return *this;
    }
    OwnedString(const OwnedString&)            = delete;
    OwnedString& operator=(const OwnedString&) = delete;

    void        reset()   noexcept { if (ptr_) { ::mdix_free_string(ptr_); ptr_ = nullptr; } }
    explicit    operator bool()  const noexcept { return ptr_ != nullptr; }
    bool        empty()          const noexcept { return !ptr_ || ptr_[0] == '\0'; }
    const char* c_str()          const noexcept { return ptr_ ? ptr_ : ""; }
    std::string str()            const          { return ptr_ ? std::string(ptr_) : std::string{}; }
    char*       release()        noexcept       { char* p = ptr_; ptr_ = nullptr; return p; }

private:
    char* ptr_;
};

/* ── Error ────────────────────────────────────────────────────────────── */

class Error {
public:
    Error() : msg_("unknown error") {}
    explicit Error(std::string msg) : msg_(std::move(msg)) {}
    const std::string& message() const noexcept { return msg_; }
    const char*        what()    const noexcept { return msg_.c_str(); }
private:
    std::string msg_;
};

inline Error last_error(const char* fallback = "unknown error") noexcept {
    const char* e = ::mdix_get_last_error();
    return Error{ e && e[0] ? std::string(e) : std::string(fallback) };
}

/* ── Result<T> ────────────────────────────────────────────────────────── */

template<typename T>
class Result {
public:
    static Result ok(T v)     { Result r; r.ok_ = true;  r.value_.emplace(std::move(v)); return r; }
    static Result err(Error e) { Result r; r.ok_ = false; r.error_ = std::move(e);        return r; }

    explicit operator bool() const noexcept { return ok_; }
    bool has_value()         const noexcept { return ok_; }

    T& value() & {
        if (!ok_) throw std::runtime_error(error_.message());
        return *value_;
    }
    const T& value() const & {
        if (!ok_) throw std::runtime_error(error_.message());
        return *value_;
    }

    T  value_or(T fallback) const noexcept { return ok_ ? *value_ : std::move(fallback); }

    const Error& error() const noexcept { return error_; }

    T*       operator->()       { return ok_ ? &*value_ : nullptr; }
    const T* operator->() const { return ok_ ? &*value_ : nullptr; }
    T&       operator*()  &     { return value(); }
    const T& operator*()  const & { return value(); }

private:
    Result() = default;
    bool             ok_    = false;
    std::optional<T> value_;
    Error            error_;
};

/* ── Database ─────────────────────────────────────────────────────────── */

class Database {
public:
    Database() noexcept : h_(nullptr) {}
    ~Database() { reset(); }

    Database(Database&& o) noexcept : h_(o.h_) { o.h_ = nullptr; }
    Database& operator=(Database&& o) noexcept {
        if (this != &o) { reset(); h_ = o.h_; o.h_ = nullptr; }
        return *this;
    }
    Database(const Database&)            = delete;
    Database& operator=(const Database&) = delete;

    void reset() noexcept { if (h_) { ::mdix_free(h_); h_ = nullptr; } }

    bool    valid()       const noexcept { return h_ && ::mdix_is_valid(h_); }
    explicit operator bool() const noexcept { return valid(); }
    int32_t entry_count() const noexcept   { return h_ ? ::mdix_entry_count(h_) : -1; }

    /** The underlying opaque handle. For interop with Builder::from_handle() and similar —
     *  Database retains ownership; do not free this pointer yourself. */
    void* raw() const noexcept { return h_; }

    /* ── Factory functions ─────────────────────────────────────────────── */

    static Result<Database> load(std::string_view path) {
        ::mdix_clear_error();
        void* h = ::mdix_load(std::string(path).c_str());
        if (!h) return Result<Database>::err(last_error("mdix_load failed"));
        return Result<Database>::ok(Database{h});
    }

    static Result<Database> load_str(std::string_view source) {
        ::mdix_clear_error();
        void* h = ::mdix_load_str(std::string(source).c_str());
        if (!h) return Result<Database>::err(last_error("mdix_load_str failed"));
        return Result<Database>::ok(Database{h});
    }

    static Result<Database> from_json(std::string_view json) {
        ::mdix_clear_error();
        void* h = ::mdix_from_json(std::string(json).c_str());
        if (!h) return Result<Database>::err(last_error("mdix_from_json failed"));
        return Result<Database>::ok(Database{h});
    }

    static Result<Database> from_toml(std::string_view toml) {
        ::mdix_clear_error();
        void* h = ::mdix_from_toml(std::string(toml).c_str());
        if (!h) return Result<Database>::err(last_error("mdix_from_toml failed"));
        return Result<Database>::ok(Database{h});
    }

    static Result<Database> load_encrypted(
        std::string_view enc_path,
        std::optional<std::string_view> key_path = std::nullopt)
    {
        ::mdix_clear_error();
        std::string ep(enc_path);
        std::string kp = key_path ? std::string(*key_path) : std::string{};
        void* h = ::mdix_load_encrypted(ep.c_str(), key_path ? kp.c_str() : nullptr);
        if (!h) return Result<Database>::err(last_error("mdix_load_encrypted failed"));
        return Result<Database>::ok(Database{h});
    }

    static Result<Database> load_encrypted_password(
        std::string_view enc_path,
        std::string_view password)
    {
        ::mdix_clear_error();
        void* h = ::mdix_load_encrypted_password(
            std::string(enc_path).c_str(),
            std::string(password).c_str());
        if (!h) return Result<Database>::err(last_error("mdix_load_encrypted_password failed"));
        return Result<Database>::ok(Database{h});
    }

    /** Wraps a raw handle produced elsewhere (merge_sources(), Watcher::check_and_reload(), ...)
     *  in an owning Database. Passing a handle not produced by this library, or one already
     *  owned elsewhere, is undefined behavior — same contract as every mdix_free() caller. */
    static Database adopt(void* raw_handle) noexcept { return Database(raw_handle); }

    /* ── Type inspection ───────────────────────────────────────────────── */

    MdixType get_type(std::string_view path) const noexcept {
        return h_ ? ::mdix_get_type(h_, std::string(path).c_str()) : MDIX_TYPE_UNKNOWN;
    }
    bool exists(std::string_view path) const noexcept {
        return h_ && ::mdix_exists(h_, std::string(path).c_str());
    }
    int32_t get_array_length(std::string_view path) const noexcept {
        return h_ ? ::mdix_get_array_length(h_, std::string(path).c_str()) : -1;
    }

    /* ── Typed getters ─────────────────────────────────────────────────── */

    Result<std::string> get_string(std::string_view path) const {
        ::mdix_clear_error();
        OwnedString s{::mdix_get_string(h_, std::string(path).c_str())};
        if (!s) return Result<std::string>::err(last_error("get_string failed"));
        return Result<std::string>::ok(s.str());
    }

    Result<int32_t> get_int(std::string_view path) const {
        ::mdix_clear_error();
        int32_t v = ::mdix_get_int(h_, std::string(path).c_str());
        if (::mdix_get_last_error()) return Result<int32_t>::err(last_error("get_int failed"));
        return Result<int32_t>::ok(v);
    }

    /** Also accepts Int values (widened without loss). */
    Result<int64_t> get_long(std::string_view path) const {
        ::mdix_clear_error();
        int64_t v = ::mdix_get_long(h_, std::string(path).c_str());
        if (::mdix_get_last_error()) return Result<int64_t>::err(last_error("get_long failed"));
        return Result<int64_t>::ok(v);
    }

    Result<float> get_float(std::string_view path) const {
        ::mdix_clear_error();
        float v = ::mdix_get_float(h_, std::string(path).c_str());
        if (::mdix_get_last_error()) return Result<float>::err(last_error("get_float failed"));
        return Result<float>::ok(v);
    }

    Result<double> get_double(std::string_view path) const {
        ::mdix_clear_error();
        double v = ::mdix_get_double(h_, std::string(path).c_str());
        if (::mdix_get_last_error()) return Result<double>::err(last_error("get_double failed"));
        return Result<double>::ok(v);
    }

    Result<bool> get_bool(std::string_view path) const {
        ::mdix_clear_error();
        bool v = ::mdix_get_bool(h_, std::string(path).c_str());
        if (::mdix_get_last_error()) return Result<bool>::err(last_error("get_bool failed"));
        return Result<bool>::ok(v);
    }

    Result<std::string> get_json(std::string_view path) const {
        ::mdix_clear_error();
        OwnedString s{::mdix_get_json(h_, std::string(path).c_str())};
        if (!s) return Result<std::string>::err(last_error("get_json failed"));
        return Result<std::string>::ok(s.str());
    }

    Result<std::string> get_enum_name(std::string_view path) const {
        ::mdix_clear_error();
        OwnedString s{::mdix_get_enum_name(h_, std::string(path).c_str())};
        if (!s) return Result<std::string>::err(last_error("get_enum_name failed"));
        return Result<std::string>::ok(s.str());
    }

    Result<std::string> get_enum_field(std::string_view path) const {
        ::mdix_clear_error();
        OwnedString s{::mdix_get_enum_field(h_, std::string(path).c_str())};
        if (!s) return Result<std::string>::err(last_error("get_enum_field failed"));
        return Result<std::string>::ok(s.str());
    }

    /* ── Keys ──────────────────────────────────────────────────────────── */

    std::vector<std::string> get_keys(std::string_view prefix = "") const {
        if (!h_) return {};
        int32_t count = 0;
        char** arr = ::mdix_get_keys(h_, std::string(prefix).c_str(), &count);
        std::vector<std::string> keys;
        if (arr && count > 0) {
            keys.reserve(static_cast<size_t>(count));
            for (int32_t i = 0; i < count; ++i)
                keys.emplace_back(arr[i] ? arr[i] : "");
            ::mdix_free_string_array(arr, count);
        }
        return keys;
    }

    /** Every key in the entire flattened data set (recursive), not just direct children of a prefix. */
    std::vector<std::string> get_all_keys() const {
        if (!h_) return {};
        int32_t count = 0;
        char** arr = ::mdix_get_all_keys(h_, &count);
        std::vector<std::string> keys;
        if (arr && count > 0) {
            keys.reserve(static_cast<size_t>(count));
            for (int32_t i = 0; i < count; ++i)
                keys.emplace_back(arr[i] ? arr[i] : "");
            ::mdix_free_string_array(arr, count);
        }
        return keys;
    }

    /* ── Metadata ──────────────────────────────────────────────────────── */

    /** Reads a key from the loaded @CONFIG section (e.g. "version", "author", "debug_mode"). */
    Result<std::string> get_config_value(std::string_view key) const {
        ::mdix_clear_error();
        OwnedString s{::mdix_get_config_value(h_, std::string(key).c_str())};
        if (!s) return Result<std::string>::err(last_error("get_config_value failed"));
        return Result<std::string>::ok(s.str());
    }

    /** Runtime version string recorded in the loaded data itself. */
    Result<std::string> get_loaded_version() const {
        ::mdix_clear_error();
        OwnedString s{::mdix_get_loaded_version(h_)};
        if (!s) return Result<std::string>::err(last_error("get_loaded_version failed"));
        return Result<std::string>::ok(s.str());
    }

    bool is_compressed() const noexcept { return h_ && ::mdix_is_compressed(h_); }
    bool is_encrypted()  const noexcept { return h_ && ::mdix_is_encrypted(h_); }

    /* ── Query ─────────────────────────────────────────────────────────── */

    /**
     * Sibling-path glob query (whole-segment `*` only, e.g. "levels.*.enemies")
     * — every value matching the pattern across paths that share structure.
     * Returns the matches as a JSON array. For a single fixed path, get_json()
     * already covers it — this is specifically for the wildcarded, multi-path case.
     */
    Result<std::string> query_many(std::string_view pattern) const {
        ::mdix_clear_error();
        OwnedString s{::mdix_select_many_as_json(h_, std::string(pattern).c_str())};
        if (!s) return Result<std::string>::err(last_error("query_many failed"));
        return Result<std::string>::ok(s.str());
    }

    /* ── Export ────────────────────────────────────────────────────────── */

    Result<std::string> to_json(bool indented = true) const {
        ::mdix_clear_error();
        OwnedString s{::mdix_to_json(h_, indented)};
        if (!s) return Result<std::string>::err(last_error("to_json failed"));
        return Result<std::string>::ok(s.str());
    }

    Result<std::string> to_toml() const {
        ::mdix_clear_error();
        OwnedString s{::mdix_to_toml(h_)};
        if (!s) return Result<std::string>::err(last_error("to_toml failed"));
        return Result<std::string>::ok(s.str());
    }

    Result<std::string> to_mdix(MdixFormatMode mode = MDIX_FORMAT_DEFAULT) const {
        ::mdix_clear_error();
        /* mdix_to_mdix returns void* (char* recast) — safe to cast back */
        OwnedString s{static_cast<char*>(::mdix_to_mdix(h_, mode))};
        if (!s) return Result<std::string>::err(last_error("to_mdix failed"));
        return Result<std::string>::ok(s.str());
    }

private:
    explicit Database(void* h) noexcept : h_(h) {}
    void* h_;
};

/* ── Builder ──────────────────────────────────────────────────────────── */

class Builder {
public:
    Builder() : h_(::mdix_builder_new()) {
        if (!h_) throw std::runtime_error("mdix_builder_new failed");
    }
    ~Builder() { reset(); }

    Builder(Builder&& o) noexcept : h_(o.h_) { o.h_ = nullptr; }
    Builder& operator=(Builder&& o) noexcept {
        if (this != &o) { reset(); h_ = o.h_; o.h_ = nullptr; }
        return *this;
    }
    Builder(const Builder&)            = delete;
    Builder& operator=(const Builder&) = delete;

    /** Pre-populates the builder with `db`'s root-level values — for round-trip editing of an
     *  already-loaded file (load -> modify a few keys -> save) rather than rebuilding one from
     *  scratch. Synthetic indexed children (tags[0], server.host, ...) are already stripped;
     *  only aggregate/root values that map back to valid .mdix identifiers carry over. */
    static Result<Builder> from_handle(const Database& db) {
        ::mdix_clear_error();
        void* h = ::mdix_builder_from_handle(db.raw());
        if (!h) return Result<Builder>::err(last_error("mdix_builder_from_handle failed"));
        return Result<Builder>::ok(Builder{h});
    }

    void    reset()        noexcept { if (h_) { ::mdix_builder_free(h_); h_ = nullptr; } }
    bool    valid()  const noexcept { return h_ != nullptr; }
    int32_t entry_count() const noexcept { return h_ ? ::mdix_builder_entry_count(h_) : -1; }
    bool    clear()        noexcept { return h_ && ::mdix_builder_clear(h_); }

    /* ── Write (fluent) ────────────────────────────────────────────────── */

    Builder& set_string(std::string_view path, std::string_view value) {
        ::mdix_builder_set_string(h_, std::string(path).c_str(), std::string(value).c_str());
        return *this;
    }
    Builder& set_int(std::string_view path, int32_t value) {
        ::mdix_builder_set_int(h_, std::string(path).c_str(), value);
        return *this;
    }
    Builder& set_long(std::string_view path, int64_t value) {
        ::mdix_builder_set_long(h_, std::string(path).c_str(), value);
        return *this;
    }
    Builder& set_float(std::string_view path, float value) {
        ::mdix_builder_set_float(h_, std::string(path).c_str(), value);
        return *this;
    }
    Builder& set_double(std::string_view path, double value) {
        ::mdix_builder_set_double(h_, std::string(path).c_str(), value);
        return *this;
    }
    Builder& set_bool(std::string_view path, bool value) {
        ::mdix_builder_set_bool(h_, std::string(path).c_str(), value);
        return *this;
    }
    bool remove(std::string_view path) noexcept {
        return h_ && ::mdix_builder_remove(h_, std::string(path).c_str());
    }

    /* ── Read back ─────────────────────────────────────────────────────── */

    bool has_key(std::string_view path) const noexcept {
        return h_ && ::mdix_builder_has_key(h_, std::string(path).c_str());
    }

    std::optional<std::string> get_string(std::string_view path) const {
        OwnedString s{::mdix_builder_get_string(h_, std::string(path).c_str())};
        return s ? std::optional<std::string>{s.str()} : std::nullopt;
    }
    std::optional<int32_t> get_int(std::string_view path) const {
        if (!has_key(path)) return std::nullopt;
        return ::mdix_builder_get_int(h_, std::string(path).c_str());
    }
    std::optional<int64_t> get_long(std::string_view path) const {
        if (!has_key(path)) return std::nullopt;
        return ::mdix_builder_get_long(h_, std::string(path).c_str());
    }
    std::optional<float> get_float(std::string_view path) const {
        if (!has_key(path)) return std::nullopt;
        return ::mdix_builder_get_float(h_, std::string(path).c_str());
    }
    std::optional<double> get_double(std::string_view path) const {
        if (!has_key(path)) return std::nullopt;
        return ::mdix_builder_get_double(h_, std::string(path).c_str());
    }
    std::optional<bool> get_bool(std::string_view path) const {
        if (!has_key(path)) return std::nullopt;
        return ::mdix_builder_get_bool(h_, std::string(path).c_str());
    }

    /* ── Persistence ───────────────────────────────────────────────────── */

    Result<std::string> to_string() const {
        ::mdix_clear_error();
        OwnedString s{::mdix_builder_to_string(h_)};
        if (!s) return Result<std::string>::err(last_error("builder_to_string failed"));
        return Result<std::string>::ok(s.str());
    }

    bool save(std::string_view path) const noexcept {
        return h_ && ::mdix_builder_save(h_, std::string(path).c_str());
    }

    Result<Database> to_database() const {
        auto src = to_string();
        if (!src) return Result<Database>::err(src.error());
        return Database::load_str(src.value());
    }

private:
    explicit Builder(void* h) noexcept : h_(h) {}
    void* h_;
};

/* ── Watcher — poll-based file watching ───────────────────────────────── */

/**
 * Watches a single plaintext .mdix path via dixscript::Runtime::HotReloadWatcher
 * — a cheap stat()-based poll, not an OS filesystem-event subscription (see
 * hot_reload.rs's own doc comment: no notify/inotify/FSEvents dependency,
 * identical behavior on every platform this ships to). Cheap enough to call
 * check_and_reload() every frame of a game loop / timer tick.
 *
 *   mdix::Watcher watcher("config.mdix");
 *   while (running) {
 *       if (auto fresh = watcher.check_and_reload()) {
 *           apply_new_config(*fresh);
 *       }
 *       tick();
 *   }
 *
 * Encrypted .mdix files are NOT supported — force_reload() always reloads
 * through the plaintext loader path internally, a core Runtime limitation,
 * not something this binding adds on top.
 */
class Watcher {
public:
    explicit Watcher(std::string_view path) : h_(::mdix_watcher_new(std::string(path).c_str())) {
        if (!h_) throw std::runtime_error(last_error("mdix_watcher_new failed").message());
    }
    ~Watcher() { reset(); }

    Watcher(Watcher&& o) noexcept : h_(o.h_) { o.h_ = nullptr; }
    Watcher& operator=(Watcher&& o) noexcept {
        if (this != &o) { reset(); h_ = o.h_; o.h_ = nullptr; }
        return *this;
    }
    Watcher(const Watcher&)            = delete;
    Watcher& operator=(const Watcher&) = delete;

    void reset() noexcept { if (h_) { ::mdix_watcher_free(h_); h_ = nullptr; } }

    std::string path() const {
        OwnedString s{::mdix_watcher_path(h_)};
        return s ? s.str() : std::string{};
    }

    bool has_loaded() const noexcept { return h_ && ::mdix_watcher_has_loaded(h_); }

    /** Checks the file's modified-time without reloading. False means either "unchanged" or
     *  "error" (bad handle, file missing) — call mdix_get_last_error() to tell them apart. */
    bool has_changed() const noexcept { return h_ && ::mdix_watcher_has_changed(h_); }

    /** Reloads only if the file changed since the last successful reload (or since
     *  construction, on the first call). std::nullopt means either "unchanged" or "error" —
     *  call mdix_get_last_error() to tell them apart. On a reload failure the watcher's
     *  internal modified-time stamp is not updated, so the next call retries against the
     *  same file state rather than silently giving up on that change. */
    std::optional<Database> check_and_reload() {
        ::mdix_clear_error();
        void* h = ::mdix_watcher_check_and_reload(h_);
        if (!h) return std::nullopt;
        return Database::adopt(h);
    }

    /** Reloads unconditionally, regardless of whether the file has changed. */
    Result<Database> force_reload() {
        ::mdix_clear_error();
        void* h = ::mdix_watcher_force_reload(h_);
        if (!h) return Result<Database>::err(last_error("mdix_watcher_force_reload failed"));
        return Result<Database>::ok(Database::adopt(h));
    }

private:
    void* h_;
};

/* ── Merge — weighted AST-level merge of multiple sources ─────────────── */

/** One key that more than one merge source defined, and which source's value won. */
struct MergeConflict {
    std::string path;
    int32_t     winning_source;
    /* Parsed out of the conflicts JSON by merge_sources()/merge_sources_weighted() below —
       label lookup requires a small JSON scan, kept in mdix.hpp rather than pulled in as a
       dependency; see parse_conflicts_json() near the bottom of this section. */
    std::string winning_label;
};

/** The merged database plus a report of every key more than one source defined. */
struct MergeResult {
    Database                   database;
    std::vector<MergeConflict> conflicts;

    bool has_conflicts() const noexcept { return !conflicts.empty(); }
};

namespace detail {

/** Tiny scanner for the flat `[{"path":..,"winningSource":..,"winningLabel":..}, ...]`
 *  shape mdix_merge_sources* writes to out_conflicts_json — not a general JSON parser,
 *  just enough structure to pull three known scalar fields out of a known one-level-deep
 *  array-of-objects shape (no nested objects/arrays inside a conflict entry). */
inline std::vector<MergeConflict> parse_conflicts_json(const std::string& json) {
    std::vector<MergeConflict> out;
    size_t i = 0;
    auto skip_ws = [&] { while (i < json.size() && std::isspace(static_cast<unsigned char>(json[i]))) ++i; };
    auto read_string = [&]() -> std::string {
        std::string s;
        if (i >= json.size() || json[i] != '"') return s;
        ++i;
        while (i < json.size() && json[i] != '"') {
            if (json[i] == '\\' && i + 1 < json.size()) {
                ++i;
                switch (json[i]) {
                    case 'n': s += '\n'; break;
                    case 't': s += '\t'; break;
                    case '"': s += '"';  break;
                    case '\\': s += '\\'; break;
                    default: s += json[i];
                }
            } else {
                s += json[i];
            }
            ++i;
        }
        if (i < json.size()) ++i; /* closing quote */
        return s;
    };

    skip_ws();
    if (i >= json.size() || json[i] != '[') return out;
    ++i; /* consume '[' */

    while (true) {
        skip_ws();
        if (i >= json.size() || json[i] == ']') break;
        if (json[i] != '{') { ++i; continue; } /* defensive skip of stray separators */

        ++i; /* consume '{' */
        MergeConflict c{};
        while (true) {
            skip_ws();
            if (i >= json.size() || json[i] == '}') { if (i < json.size()) ++i; break; }
            if (json[i] == ',') { ++i; continue; }
            if (json[i] != '"') { ++i; continue; } /* defensive skip */

            std::string key = read_string();
            skip_ws();
            if (i < json.size() && json[i] == ':') ++i;
            skip_ws();

            if (key == "path") {
                c.path = read_string();
            } else if (key == "winningSource") {
                size_t start = i;
                while (i < json.size() && (std::isdigit(static_cast<unsigned char>(json[i])) || json[i] == '-')) ++i;
                c.winning_source = start < i ? std::stoi(json.substr(start, i - start)) : -1;
            } else if (key == "winningLabel") {
                if (i < json.size() && json[i] == 'n') { i += 4; /* null */ }
                else { c.winning_label = read_string(); }
            } else if (i < json.size() && json[i] == '"') {
                read_string(); /* unknown key — skip its string value */
            } else {
                while (i < json.size() && json[i] != ',' && json[i] != '}') ++i; /* skip bare token */
            }
        }
        out.push_back(std::move(c));

        skip_ws();
        if (i < json.size() && json[i] == ',') ++i;
    }
    return out;
}

} /* namespace detail */

/**
 * Merges `sources` with auto-descending weights (source 0 highest) and the given strategies.
 * Real AST-level merge (dixscript::Runtime::MdixMerger) — weighted-priority conflict
 * resolution, per-source conflict reporting, full type fidelity — not a shallow JSON-object
 * merge.
 */
inline Result<MergeResult> merge_sources(
    const std::vector<std::string>& sources,
    MdixMergeStrategy strategy = MDIX_MERGE_WEIGHTED_PRIORITY,
    MdixArrayMergeStrategy array_strategy = MDIX_ARRAY_MERGE_REPLACE)
{
    ::mdix_clear_error();
    if (sources.empty()) return Result<MergeResult>::err(Error{"merge_sources: at least one source is required"});

    std::vector<const char*> raw;
    raw.reserve(sources.size());
    for (const auto& s : sources) raw.push_back(s.c_str());

    char* conflicts_raw = nullptr;
    void* h = ::mdix_merge_sources(
        raw.data(), static_cast<int32_t>(raw.size()), strategy, array_strategy, &conflicts_raw);
    OwnedString conflicts_json{conflicts_raw};
    if (!h) return Result<MergeResult>::err(last_error("mdix_merge_sources failed"));

    return Result<MergeResult>::ok(MergeResult{
        Database::adopt(h),
        detail::parse_conflicts_json(conflicts_json.str())
    });
}

/** As merge_sources, but with explicit per-source weights — `weights` must have exactly
 *  `sources.size()` entries, one per source, in the same order. */
inline Result<MergeResult> merge_sources_weighted(
    const std::vector<std::string>& sources,
    const std::vector<double>& weights,
    MdixMergeStrategy strategy = MDIX_MERGE_WEIGHTED_PRIORITY,
    MdixArrayMergeStrategy array_strategy = MDIX_ARRAY_MERGE_REPLACE)
{
    ::mdix_clear_error();
    if (sources.empty()) return Result<MergeResult>::err(Error{"merge_sources_weighted: at least one source is required"});
    if (weights.size() != sources.size()) {
        return Result<MergeResult>::err(Error{"merge_sources_weighted: weights.size() must equal sources.size()"});
    }

    std::vector<const char*> raw;
    raw.reserve(sources.size());
    for (const auto& s : sources) raw.push_back(s.c_str());

    char* conflicts_raw = nullptr;
    void* h = ::mdix_merge_sources_weighted(
        raw.data(), weights.data(), static_cast<int32_t>(raw.size()),
        strategy, array_strategy, &conflicts_raw);
    OwnedString conflicts_json{conflicts_raw};
    if (!h) return Result<MergeResult>::err(last_error("mdix_merge_sources_weighted failed"));

    return Result<MergeResult>::ok(MergeResult{
        Database::adopt(h),
        detail::parse_conflicts_json(conflicts_json.str())
    });
}

/* ── Free functions ───────────────────────────────────────────────────── */

inline std::string version() noexcept { return ::mdix_version(); }

inline Result<std::string> format_source(
    std::string_view src,
    MdixFormatMode mode = MDIX_FORMAT_DEFAULT)
{
    ::mdix_clear_error();
    OwnedString s{::mdix_format_source(std::string(src).c_str(), mode)};
    if (!s) return Result<std::string>::err(last_error("format_source failed"));
    return Result<std::string>::ok(s.str());
}

inline Result<std::string> minify_source(std::string_view src) {
    ::mdix_clear_error();
    OwnedString s{::mdix_minify_source(std::string(src).c_str())};
    if (!s) return Result<std::string>::err(last_error("minify_source failed"));
    return Result<std::string>::ok(s.str());
}

/** Removes blank/redundant whitespace without touching comments or overall structure — see minify_source() for the more aggressive pass. */
inline Result<std::string> compact_source(std::string_view src) {
    ::mdix_clear_error();
    OwnedString s{::mdix_compact_source(std::string(src).c_str())};
    if (!s) return Result<std::string>::err(last_error("compact_source failed"));
    return Result<std::string>::ok(s.str());
}

/** Strips line and block comments, leaving formatting otherwise untouched. */
inline Result<std::string> strip_comments(std::string_view src) {
    ::mdix_clear_error();
    OwnedString s{::mdix_strip_comments(std::string(src).c_str())};
    if (!s) return Result<std::string>::err(last_error("strip_comments failed"));
    return Result<std::string>::ok(s.str());
}

/** Parses `source` and reports only whether it's syntactically valid DixScript — this is NOT
 *  schema validation against expected fields/types, just "does it parse". False on either a
 *  real parse failure or an empty source — call mdix_get_last_error() to tell them apart. */
inline bool validate(std::string_view source) noexcept {
    return ::mdix_validate(std::string(source).c_str());
}

} /* namespace mdix */
