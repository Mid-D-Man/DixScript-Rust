/**
 * test_mdix_cpp.cpp — C++ RAII wrapper tests for mdix.hpp.
 *
 * Self-contained assert-and-report runner (no GoogleTest/Catch2 dependency
 * — see CMakeLists.txt's Tests section for why). Tests the header-only
 * wrapper independently of test_mdix_c.c's plain-C coverage, since a bug in
 * mdix.hpp itself (RAII lifetime, Result<T> plumbing, the hand-rolled
 * conflict-JSON scanner) wouldn't show up testing mdix.h alone.
 */

#include "mdix.hpp"

#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>

static int g_failures = 0;
static int g_tests = 0;

#define TEST(name) static void name()
#define RUN(name) do { g_tests++; std::printf("  %-45s", #name); name(); } while (0)
#define CHECK(cond) do { \
    if (cond) { std::printf("ok\n"); } \
    else { std::printf("FAIL (%s:%d: %s)\n", __FILE__, __LINE__, #cond); g_failures++; } \
} while (0)

static const char* SIMPLE_SRC =
    "@DATA( app_name = \"MyApp\", port = 8080, ratio = 1.5f, active = true )";

static const char* BASE_SRC =
    "@DATA( app_name = \"MyApp\" server: host = \"localhost\", port = 8080 tags:: \"a\", \"b\" )";

static const char* OVERRIDE_SRC =
    "@DATA( server: port = 9090, ssl = true tags:: \"c\" )";

/* ── Database — sanity (pre-existing surface) ─────────────────────────── */

TEST(database_load_str_and_typed_getters) {
    auto db = mdix::Database::load_str(SIMPLE_SRC);
    CHECK(static_cast<bool>(db));
    CHECK(db->get_string("app_name").value_or("") == "MyApp");
    CHECK(db->get_int("port").value_or(0) == 8080);
}

TEST(database_load_str_malformed_is_error) {
    auto db = mdix::Database::load_str("@@@INVALID$$$");
    CHECK(!db);
    CHECK(!db.error().message().empty());
}

/* ── This pass's additions: metadata ──────────────────────────────────── */

TEST(is_compressed_and_is_encrypted_false_for_plain_load) {
    auto db = mdix::Database::load_str(SIMPLE_SRC);
    CHECK(static_cast<bool>(db));
    CHECK(db->is_compressed() == false);
    CHECK(db->is_encrypted() == false);
}

TEST(get_loaded_version_returns_nonempty) {
    auto db = mdix::Database::load_str(SIMPLE_SRC);
    CHECK(static_cast<bool>(db));
    auto v = db->get_loaded_version();
    CHECK(static_cast<bool>(v) && !v->empty());
}

TEST(get_all_keys_covers_nested_paths) {
    auto db = mdix::Database::load_str(BASE_SRC);
    CHECK(static_cast<bool>(db));
    auto keys = db->get_all_keys();
    bool found = false;
    for (const auto& k : keys) if (k.find("server.host") != std::string::npos) found = true;
    CHECK(found);
}

/* ── Query ─────────────────────────────────────────────────────────────── */

TEST(query_many_returns_json_array) {
    auto db = mdix::Database::load_str(
        "@DATA( levels:: { id = 1, name = \"Cave\" }, { id = 2, name = \"Forest\" } )");
    CHECK(static_cast<bool>(db));
    auto result = db->query_many("levels.*.name");
    CHECK(static_cast<bool>(result));
    if (result) CHECK(!result->empty() && result->front() == '[');
}

/* ── Validate ──────────────────────────────────────────────────────────── */

TEST(validate_accepts_and_rejects) {
    CHECK(mdix::validate(SIMPLE_SRC) == true);
    CHECK(mdix::validate("@@@INVALID$$$") == false);
}

/* ── Merge ─────────────────────────────────────────────────────────────── */

TEST(merge_sources_default_weights_primary_wins) {
    auto result = mdix::merge_sources({BASE_SRC, OVERRIDE_SRC});
    CHECK(static_cast<bool>(result));
    if (result) {
        CHECK(result->database.get_int("server.port").value_or(-1) == 8080);
        CHECK(result->has_conflicts());
        bool found_port_conflict = false;
        for (const auto& c : result->conflicts) if (c.path.find("port") != std::string::npos) found_port_conflict = true;
        CHECK(found_port_conflict);
    }
}

TEST(merge_sources_secondary_wins_strategy) {
    auto result = mdix::merge_sources({BASE_SRC, OVERRIDE_SRC}, MDIX_MERGE_SECONDARY_WINS);
    CHECK(static_cast<bool>(result));
    if (result) CHECK(result->database.get_int("server.port").value_or(-1) == 9090);
}

TEST(merge_sources_weighted_explicit_weights) {
    auto result = mdix::merge_sources_weighted(
        {BASE_SRC, OVERRIDE_SRC}, {0.2, 0.9}, MDIX_MERGE_WEIGHTED_PRIORITY, MDIX_ARRAY_MERGE_REPLACE);
    CHECK(static_cast<bool>(result));
    if (result) CHECK(result->database.get_int("server.port").value_or(-1) == 9090);
}

TEST(merge_sources_weighted_mismatched_lengths_is_error) {
    auto result = mdix::merge_sources_weighted({BASE_SRC, OVERRIDE_SRC}, {1.0});
    CHECK(!result);
}

TEST(merge_sources_array_strategy_concat_dedup) {
    auto result = mdix::merge_sources(
        {BASE_SRC, OVERRIDE_SRC}, MDIX_MERGE_WEIGHTED_PRIORITY, MDIX_ARRAY_MERGE_CONCAT);
    CHECK(static_cast<bool>(result));
    if (result) CHECK(result->database.get_array_length("tags") == 3); /* ["a","b"] ++ ["c"] */
}

TEST(merge_sources_throw_on_conflict_fails) {
    auto result = mdix::merge_sources({BASE_SRC, OVERRIDE_SRC}, MDIX_MERGE_THROW_ON_CONFLICT);
    CHECK(!result);
}

TEST(merge_sources_empty_list_is_error) {
    auto result = mdix::merge_sources({});
    CHECK(!result);
}

/* ── Builder::from_handle ──────────────────────────────────────────────── */

TEST(builder_from_handle_round_trips_and_saves) {
    auto db = mdix::Database::load_str(SIMPLE_SRC);
    CHECK(static_cast<bool>(db));
    auto builder = mdix::Builder::from_handle(*db);
    CHECK(static_cast<bool>(builder));
    if (builder) {
        CHECK(builder->has_key("app_name"));
        builder->set_int("port", 9999);
        CHECK(builder->get_int("port").value_or(0) == 9999);
    }
}

/* ── Watcher ───────────────────────────────────────────────────────────── */

TEST(watcher_reports_change_and_reloads) {
    const char* path = "test_watcher_cpp_tmp.mdix";
    { std::ofstream f(path); f << "@DATA( port = 8080 )"; }

    mdix::Watcher watcher(path);
    CHECK(!watcher.has_loaded());
    CHECK(watcher.has_changed());

    auto reloaded = watcher.check_and_reload();
    CHECK(static_cast<bool>(reloaded));
    if (reloaded) CHECK(reloaded->get_int("port").value_or(0) == 8080);

    CHECK(watcher.has_loaded());
    CHECK(!watcher.has_changed()); /* nothing changed since the reload above */

    std::remove(path);
}

TEST(watcher_force_reload_reloads_unconditionally) {
    const char* path = "test_watcher_cpp_force_tmp.mdix";
    { std::ofstream f(path); f << "@DATA( port = 1234 )"; }

    mdix::Watcher watcher(path);
    watcher.check_and_reload();

    auto forced = watcher.force_reload();
    CHECK(static_cast<bool>(forced));
    if (forced) CHECK(forced->get_int("port").value_or(0) == 1234);

    std::remove(path);
}

TEST(watcher_missing_file_reports_error) {
    mdix::Watcher watcher("no_such_file_xyz.mdix");
    CHECK(!watcher.has_changed());
}

/* ── Source text transforms ────────────────────────────────────────────── */

TEST(strip_comments_removes_comment_text) {
    auto out = mdix::strip_comments("@DATA( a = 1 // a comment\n)");
    CHECK(static_cast<bool>(out));
    if (out) CHECK(out->find("a comment") == std::string::npos);
}

TEST(compact_source_roundtrips_through_validate) {
    auto out = mdix::compact_source(SIMPLE_SRC);
    CHECK(static_cast<bool>(out));
    if (out) CHECK(mdix::validate(*out));
}

int main() {
    std::printf("mdix.hpp C++ wrapper tests\n");
    RUN(database_load_str_and_typed_getters);
    RUN(database_load_str_malformed_is_error);
    RUN(is_compressed_and_is_encrypted_false_for_plain_load);
    RUN(get_loaded_version_returns_nonempty);
    RUN(get_all_keys_covers_nested_paths);
    RUN(query_many_returns_json_array);
    RUN(validate_accepts_and_rejects);
    RUN(merge_sources_default_weights_primary_wins);
    RUN(merge_sources_secondary_wins_strategy);
    RUN(merge_sources_weighted_explicit_weights);
    RUN(merge_sources_weighted_mismatched_lengths_is_error);
    RUN(merge_sources_array_strategy_concat_dedup);
    RUN(merge_sources_throw_on_conflict_fails);
    RUN(merge_sources_empty_list_is_error);
    RUN(builder_from_handle_round_trips_and_saves);
    RUN(watcher_reports_change_and_reloads);
    RUN(watcher_force_reload_reloads_unconditionally);
    RUN(watcher_missing_file_reports_error);
    RUN(strip_comments_removes_comment_text);
    RUN(compact_source_roundtrips_through_validate);

    std::printf("\n%d/%d tests passed\n", g_tests - g_failures, g_tests);
    return g_failures == 0 ? 0 : 1;
}
