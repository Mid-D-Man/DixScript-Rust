package com.midmanstudio.dixscript;

import org.junit.jupiter.api.*;
import static org.assertj.core.api.Assertions.*;

/**
 * Integration tests for {@link Merge}.
 * Requires the native lib to be on java.library.path (set by build.gradle.kts).
 */
class MergeTest {

    private static final String BASE =
        "@DATA( app_name = \"MyApp\" server: host = \"localhost\", port = 8080 tags:: \"a\", \"b\" )";

    private static final String OVERRIDE =
        "@DATA( server: port = 9090, ssl = true tags:: \"c\" )";

    // ── sources() — defaults ─────────────────────────────────────────────────

    @Test void sources_defaultWeights_primaryWinsSharedKey() {
        Merge.Result result = Merge.sources(BASE, OVERRIDE);
        try (Database merged = result.database) {
            // Auto-descending weights: source[0] outweighs source[1] on the shared "port" key.
            assertThat(merged.getInt("server.port")).isEqualTo(8080);
            assertThat(merged.getString("server.host")).isEqualTo("localhost");
            assertThat(merged.getBool("server.ssl")).isTrue(); // not a conflict — only OVERRIDE defines it
        }
    }

    @Test void sources_reportsConflictOnSharedKey() {
        Merge.Result result = Merge.sources(BASE, OVERRIDE);
        try (Database ignored = result.database) {
            assertThat(result.hasConflicts()).isTrue();
            assertThat(result.conflicts).anyMatch(c -> c.path.contains("port"));
        }
    }

    // ── sourcesWeighted() — strategies ───────────────────────────────────────

    @Test void sourcesWeighted_secondaryWins_overrideTakesPriority() {
        Merge.Result result = Merge.sourcesWeighted(
            java.util.Arrays.asList(BASE, OVERRIDE), null, Merge.Strategy.SECONDARY_WINS, Merge.ArrayStrategy.REPLACE);
        try (Database merged = result.database) {
            assertThat(merged.getInt("server.port")).isEqualTo(9090);
        }
    }

    @Test void sourcesWeighted_primaryWins_baseTakesPriority() {
        Merge.Result result = Merge.sourcesWeighted(
            java.util.Arrays.asList(BASE, OVERRIDE), null, Merge.Strategy.PRIMARY_WINS, Merge.ArrayStrategy.REPLACE);
        try (Database merged = result.database) {
            assertThat(merged.getInt("server.port")).isEqualTo(8080);
        }
    }

    @Test void sourcesWeighted_explicitWeights_higherWeightWins() {
        Merge.Result result = Merge.sourcesWeighted(
            java.util.Arrays.asList(BASE, OVERRIDE),
            new double[] { 0.2, 0.9 },
            Merge.Strategy.WEIGHTED_PRIORITY,
            Merge.ArrayStrategy.REPLACE);
        try (Database merged = result.database) {
            assertThat(merged.getInt("server.port")).isEqualTo(9090);
        }
    }

    @Test void sourcesWeighted_throwOnConflict_throwsWhenSharedKeyExists() {
        assertThatThrownBy(() -> Merge.sourcesWeighted(
                java.util.Arrays.asList(BASE, OVERRIDE), null, Merge.Strategy.THROW_ON_CONFLICT, Merge.ArrayStrategy.REPLACE))
            .isInstanceOf(MdixException.class);
    }

    @Test void sourcesWeighted_mismatchedWeightsLength_throws() {
        assertThatThrownBy(() -> Merge.sourcesWeighted(
                java.util.Arrays.asList(BASE, OVERRIDE), new double[] { 1.0 }, Merge.Strategy.WEIGHTED_PRIORITY, Merge.ArrayStrategy.REPLACE))
            .isInstanceOf(MdixException.class);
    }

    // ── array strategies ─────────────────────────────────────────────────────

    @Test void arrayStrategy_replace_winnerReplacesArray() {
        Merge.Result result = Merge.sourcesWeighted(
            java.util.Arrays.asList(BASE, OVERRIDE), null, Merge.Strategy.WEIGHTED_PRIORITY, Merge.ArrayStrategy.REPLACE);
        try (Database merged = result.database) {
            assertThat(merged.arrayLength("tags")).isEqualTo(2); // BASE's ["a","b"] wins outright
        }
    }

    @Test void arrayStrategy_concat_combinesBothArrays() {
        Merge.Result result = Merge.sourcesWeighted(
            java.util.Arrays.asList(BASE, OVERRIDE), null, Merge.Strategy.WEIGHTED_PRIORITY, Merge.ArrayStrategy.CONCAT);
        try (Database merged = result.database) {
            assertThat(merged.arrayLength("tags")).isEqualTo(3); // ["a","b"] ++ ["c"]
        }
    }

    // ── databases() — already-loaded ─────────────────────────────────────────

    @Test void databases_roundTripsThroughSourceText() {
        try (Database base = DixScript.loadStr(BASE);
             Database override = DixScript.loadStr(OVERRIDE)) {
            Merge.Result result = Merge.databases(base, override);
            try (Database merged = result.database) {
                assertThat(merged.getString("app_name")).isEqualTo("MyApp");
            }
        }
    }

    // ── validation ────────────────────────────────────────────────────────────

    @Test void sources_emptyArray_throws() {
        assertThatThrownBy(Merge::sources).isInstanceOf(MdixException.class);
    }

    @Test void sources_malformedSource_throws() {
        assertThatThrownBy(() -> Merge.sources(BASE, "@@@INVALID$$$")).isInstanceOf(MdixException.class);
    }
}
