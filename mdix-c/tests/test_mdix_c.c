/**
 * test_mdix_c.c — plain C API tests for mdix.h.
 *
 * Self-contained assert-and-report runner (no external test framework —
 * see CMakeLists.txt's Tests section for why). Each TEST() prints PASS/FAIL
 * and the binary exits non-zero if anything failed, so `ctest` sees it the
 * same way it would see a GoogleTest/Catch2 binary.
 *
 * Focused on this pass's additions (merge, query_many, validate, hot
 * reload, the smaller metadata/conversion gaps) plus enough of the
 * pre-existing surface (load, get/set, builder) to confirm nothing broke.
 */

#include "mdix.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int g_failures = 0;
static int g_tests = 0;

#define TEST(name) static void name(void)
#define RUN(name) do { g_tests++; printf("  %-45s", #name); name(); } while (0)
#define CHECK(cond) do { \
    if (cond) { printf("ok\n"); } \
    else { printf("FAIL (%s:%d: %s)\n", __FILE__, __LINE__, #cond); g_failures++; } \
} while (0)
#define CHECK_STR_EQ(actual, expected) do { \
    const char* _a = (actual); const char* _e = (expected); \
    if (_a && strcmp(_a, _e) == 0) { printf("ok\n"); } \
    else { printf("FAIL (%s:%d: got \"%s\", want \"%s\")\n", __FILE__, __LINE__, _a ? _a : "(null)", _e); g_failures++; } \
} while (0)

static const char* SIMPLE_SRC =
    "@DATA( app_name = \"MyApp\", port = 8080, ratio = 1.5f, active = true )";

static const char* BASE_SRC =
    "@DATA( app_name = \"MyApp\" server: host = \"localhost\", port = 8080 tags:: \"a\", \"b\" )";

static const char* OVERRIDE_SRC =
    "@DATA( server: port = 9090, ssl = true tags:: \"c\" )";

/* ── Load / basic getters (sanity — pre-existing surface) ────────────── */

TEST(load_str_and_get_values) {
    void* h = mdix_load_str(SIMPLE_SRC);
    CHECK(h != NULL);
    char* app_name = mdix_get_string(h, "app_name");
    CHECK_STR_EQ(app_name, "MyApp");
    mdix_free_string(app_name);
    mdix_free(h);
}

TEST(load_str_malformed_returns_null) {
    void* h = mdix_load_str("@@@INVALID$$$");
    CHECK(h == NULL);
    CHECK(mdix_get_last_error() != NULL);
    mdix_clear_error();
}

/* ── This pass's additions: metadata ──────────────────────────────────── */

TEST(is_compressed_and_is_encrypted_false_for_plain_load) {
    void* h = mdix_load_str(SIMPLE_SRC);
    CHECK(h != NULL);
    CHECK(mdix_is_compressed(h) == false);
    CHECK(mdix_is_encrypted(h) == false);
    mdix_free(h);
}

TEST(get_loaded_version_returns_nonempty_string) {
    void* h = mdix_load_str(SIMPLE_SRC);
    CHECK(h != NULL);
    char* v = mdix_get_loaded_version(h);
    CHECK(v != NULL && v[0] != '\0');
    mdix_free_string(v);
    mdix_free(h);
}

TEST(get_all_keys_covers_nested_paths) {
    void* h = mdix_load_str(BASE_SRC);
    CHECK(h != NULL);
    int32_t count = 0;
    char** keys = mdix_get_all_keys(h, &count);
    CHECK(keys != NULL && count > 0);
    int found_nested = 0;
    for (int32_t i = 0; i < count; i++) {
        if (keys[i] && strstr(keys[i], "server.host") != NULL) found_nested = 1;
    }
    CHECK(found_nested == 1);
    mdix_free_string_array(keys, count);
    mdix_free(h);
}

/* ── Query ─────────────────────────────────────────────────────────────── */

TEST(select_many_as_json_returns_valid_json_array) {
    void* h = mdix_load_str(
        "@DATA( levels:: { id = 1, name = \"Cave\" }, { id = 2, name = \"Forest\" } )");
    CHECK(h != NULL);
    char* json = mdix_select_many_as_json(h, "levels.*.name");
    CHECK(json != NULL);
    if (json) { CHECK(json[0] == '['); mdix_free_string(json); }
    mdix_free(h);
}

TEST(select_many_as_json_no_match_returns_empty_array) {
    void* h = mdix_load_str(SIMPLE_SRC);
    CHECK(h != NULL);
    char* json = mdix_select_many_as_json(h, "no.such.path.*");
    CHECK(json != NULL);
    if (json) { CHECK(strcmp(json, "[]") == 0); mdix_free_string(json); }
    mdix_free(h);
}

/* ── Validate ──────────────────────────────────────────────────────────── */

TEST(validate_accepts_wellformed_source) {
    CHECK(mdix_validate(SIMPLE_SRC) == true);
}

TEST(validate_rejects_malformed_source) {
    CHECK(mdix_validate("@@@INVALID$$$") == false);
    mdix_clear_error();
}

/* ── Merge ─────────────────────────────────────────────────────────────── */

TEST(merge_sources_default_weights_primary_wins) {
    const char* sources[2] = { BASE_SRC, OVERRIDE_SRC };
    char* conflicts_json = NULL;
    void* merged = mdix_merge_sources(
        sources, 2, MDIX_MERGE_WEIGHTED_PRIORITY, MDIX_ARRAY_MERGE_REPLACE, &conflicts_json);
    CHECK(merged != NULL);
    if (merged) {
        CHECK(mdix_get_int(merged, "server.port") == 8080); /* source 0 outweighs source 1 */
        CHECK(mdix_get_bool(merged, "server.ssl") == true); /* not a conflict — only source 1 defines it */
        mdix_free(merged);
    }
    CHECK(conflicts_json != NULL);
    if (conflicts_json) {
        CHECK(strstr(conflicts_json, "port") != NULL);
        mdix_free_string(conflicts_json);
    }
}

TEST(merge_sources_secondary_wins_strategy) {
    const char* sources[2] = { BASE_SRC, OVERRIDE_SRC };
    char* conflicts_json = NULL;
    void* merged = mdix_merge_sources(
        sources, 2, MDIX_MERGE_SECONDARY_WINS, MDIX_ARRAY_MERGE_REPLACE, &conflicts_json);
    CHECK(merged != NULL);
    if (merged) {
        CHECK(mdix_get_int(merged, "server.port") == 9090);
        mdix_free(merged);
    }
    if (conflicts_json) mdix_free_string(conflicts_json);
}

TEST(merge_sources_weighted_explicit_weights) {
    const char* sources[2] = { BASE_SRC, OVERRIDE_SRC };
    double weights[2] = { 0.2, 0.9 };
    char* conflicts_json = NULL;
    void* merged = mdix_merge_sources_weighted(
        sources, weights, 2, MDIX_MERGE_WEIGHTED_PRIORITY, MDIX_ARRAY_MERGE_REPLACE, &conflicts_json);
    CHECK(merged != NULL);
    if (merged) {
        CHECK(mdix_get_int(merged, "server.port") == 9090); /* higher weight wins */
        mdix_free(merged);
    }
    if (conflicts_json) mdix_free_string(conflicts_json);
}

TEST(merge_sources_array_strategy_concat) {
    const char* sources[2] = { BASE_SRC, OVERRIDE_SRC };
    char* conflicts_json = NULL;
    void* merged = mdix_merge_sources(
        sources, 2, MDIX_MERGE_WEIGHTED_PRIORITY, MDIX_ARRAY_MERGE_CONCAT, &conflicts_json);
    CHECK(merged != NULL);
    if (merged) {
        CHECK(mdix_get_array_length(merged, "tags") == 3); /* ["a","b"] ++ ["c"] */
        mdix_free(merged);
    }
    if (conflicts_json) mdix_free_string(conflicts_json);
}

TEST(merge_sources_throw_on_conflict_fails) {
    const char* sources[2] = { BASE_SRC, OVERRIDE_SRC };
    char* conflicts_json = NULL;
    void* merged = mdix_merge_sources(
        sources, 2, MDIX_MERGE_THROW_ON_CONFLICT, MDIX_ARRAY_MERGE_REPLACE, &conflicts_json);
    CHECK(merged == NULL);
    CHECK(mdix_get_last_error() != NULL);
    mdix_clear_error();
    if (conflicts_json) mdix_free_string(conflicts_json);
}

TEST(merge_sources_empty_count_fails) {
    char* conflicts_json = NULL;
    void* merged = mdix_merge_sources(
        NULL, 0, MDIX_MERGE_WEIGHTED_PRIORITY, MDIX_ARRAY_MERGE_REPLACE, &conflicts_json);
    CHECK(merged == NULL);
    mdix_clear_error();
    if (conflicts_json) mdix_free_string(conflicts_json);
}

/* ── Builder round-trip via mdix_builder_from_handle ──────────────────── */

TEST(builder_from_handle_carries_over_root_values) {
    void* h = mdix_load_str(SIMPLE_SRC);
    CHECK(h != NULL);
    void* b = mdix_builder_from_handle(h);
    CHECK(b != NULL);
    if (b) {
        CHECK(mdix_builder_has_key(b, "app_name") == true);
        mdix_builder_set_int(b, "port", 9999);
        CHECK(mdix_builder_get_int(b, "port") == 9999);
        mdix_builder_free(b);
    }
    mdix_free(h);
}

/* ── Hot reload ────────────────────────────────────────────────────────── */

TEST(watcher_reports_change_on_first_check) {
    const char* path = "test_watcher_c_tmp.mdix";
    FILE* f = fopen(path, "w");
    CHECK(f != NULL);
    if (f) { fputs("@DATA( port = 8080 )", f); fclose(f); }

    void* w = mdix_watcher_new(path);
    CHECK(w != NULL);
    if (w) {
        CHECK(mdix_watcher_has_loaded(w) == false);
        CHECK(mdix_watcher_has_changed(w) == true);

        void* reloaded = mdix_watcher_check_and_reload(w);
        CHECK(reloaded != NULL);
        if (reloaded) { CHECK(mdix_get_int(reloaded, "port") == 8080); mdix_free(reloaded); }

        CHECK(mdix_watcher_has_loaded(w) == true);
        CHECK(mdix_watcher_has_changed(w) == false); /* nothing changed since the reload above */

        mdix_watcher_free(w);
    }
    remove(path);
}

TEST(watcher_missing_file_reports_error) {
    void* w = mdix_watcher_new("no_such_file_xyz.mdix");
    CHECK(w != NULL); /* construction itself doesn't touch the filesystem */
    if (w) {
        CHECK(mdix_watcher_has_changed(w) == false);
        CHECK(mdix_get_last_error() != NULL);
        mdix_clear_error();
        mdix_watcher_free(w);
    }
}

/* ── Source text transforms ───────────────────────────────────────────── */

TEST(strip_comments_removes_comment_text) {
    char* out = mdix_strip_comments("@DATA( a = 1 // a comment\n)");
    CHECK(out != NULL);
    if (out) { CHECK(strstr(out, "a comment") == NULL); mdix_free_string(out); }
}

TEST(compact_source_roundtrips_through_validate) {
    char* out = mdix_compact_source(SIMPLE_SRC);
    CHECK(out != NULL);
    if (out) { CHECK(mdix_validate(out) == true); mdix_free_string(out); }
}

int main(void) {
    printf("mdix.h C API tests\n");
    RUN(load_str_and_get_values);
    RUN(load_str_malformed_returns_null);
    RUN(is_compressed_and_is_encrypted_false_for_plain_load);
    RUN(get_loaded_version_returns_nonempty_string);
    RUN(get_all_keys_covers_nested_paths);
    RUN(select_many_as_json_returns_valid_json_array);
    RUN(select_many_as_json_no_match_returns_empty_array);
    RUN(validate_accepts_wellformed_source);
    RUN(validate_rejects_malformed_source);
    RUN(merge_sources_default_weights_primary_wins);
    RUN(merge_sources_secondary_wins_strategy);
    RUN(merge_sources_weighted_explicit_weights);
    RUN(merge_sources_array_strategy_concat);
    RUN(merge_sources_throw_on_conflict_fails);
    RUN(merge_sources_empty_count_fails);
    RUN(builder_from_handle_carries_over_root_values);
    RUN(watcher_reports_change_on_first_check);
    RUN(watcher_missing_file_reports_error);
    RUN(strip_comments_removes_comment_text);
    RUN(compact_source_roundtrips_through_validate);

    printf("\n%d/%d tests passed\n", g_tests - g_failures, g_tests);
    return g_failures == 0 ? 0 : 1;
}
