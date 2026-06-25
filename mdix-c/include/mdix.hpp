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
    void* h_;
};

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

} /* namespace mdix */
