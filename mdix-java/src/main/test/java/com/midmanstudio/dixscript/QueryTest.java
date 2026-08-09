package com.midmanstudio.dixscript;

import org.junit.jupiter.api.*;
import static org.assertj.core.api.Assertions.*;

import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

/**
 * Integration tests for {@link Database#query}, {@link Database#queryMany}, and {@link MdixQuery}.
 * Requires the native lib to be on java.library.path (set by build.gradle.kts).
 */
class QueryTest {

    private static final String ENEMIES_SRC =
        "@DATA( " +
        "  world = \"overworld\" " +
        "  tags:: \"alpha\", \"beta\", \"alpha\", \"gamma\" " +
        "  enemies:: " +
        "    { name = \"Goblin\", health = 50, aiType = \"AGGRESSIVE\" }, " +
        "    { name = \"Orc\", health = 100, aiType = \"AGGRESSIVE\" }, " +
        "    { name = \"Dragon\", health = 1000, aiType = \"BOSS\" }, " +
        "    { name = \"Slime\", health = 20, aiType = \"PASSIVE\" } " +
        "  levels:: " +
        "    { id = 1, enemies:: { name = \"Rat\", health = 5, aiType = \"PASSIVE\" } }, " +
        "    { id = 2, enemies:: { name = \"Bat\", health = 8, aiType = \"PASSIVE\" } } " +
        ")";

    private Database db;

    @BeforeEach
    void setUp() {
        db = DixScript.loadStr(ENEMIES_SRC);
    }

    @AfterEach
    void tearDown() {
        db.close();
    }

    // ── query(path) ───────────────────────────────────────────────────────────

    @Test void query_returnsAllElements() {
        assertThat(db.query("enemies").count()).isEqualTo(4);
    }

    @Test void query_where_filtersByField() {
        List<String> aggressive = db.query("enemies")
            .where_(e -> "AGGRESSIVE".equals(e.field("aiType").asString()))
            .select(e -> e.field("name").asString());
        assertThat(aggressive).containsExactly("Goblin", "Orc");
    }

    @Test void query_whereFieldEquals_shorthand() {
        assertThat(db.query("enemies").whereFieldEquals("aiType", MdixValue.ofString("BOSS")).count())
            .isEqualTo(1);
    }

    @Test void query_select_projectsField() {
        List<String> names = db.query("enemies").selectField("name").stream()
            .map(MdixValue::asString)
            .collect(Collectors.toList());
        assertThat(names).containsExactly("Goblin", "Orc", "Dragon", "Slime");
    }

    @Test void query_orderBy_sortsAscending() {
        List<Long> healths = db.query("enemies")
            .orderBy(e -> e.field("health").asLong())
            .select(e -> e.field("health").asLong());
        assertThat(healths).containsExactly(20L, 50L, 100L, 1000L);
    }

    @Test void query_orderByDescending_sortsDescending() {
        MdixValue first = db.query("enemies").orderByDescending(e -> e.field("health").asLong()).first();
        assertThat(first.field("name").asString()).isEqualTo("Dragon");
    }

    @Test void query_take_limitsResults() {
        assertThat(db.query("enemies").take(2).count()).isEqualTo(2);
    }

    @Test void query_skip_dropsLeadingResults() {
        assertThat(db.query("enemies").skip(3).count()).isEqualTo(1);
    }

    @Test void query_any_true() {
        assertThat(db.query("enemies").any(e -> "BOSS".equals(e.field("aiType").asString()))).isTrue();
    }

    @Test void query_all_false() {
        assertThat(db.query("enemies").all(e -> "AGGRESSIVE".equals(e.field("aiType").asString()))).isFalse();
    }

    @Test void query_count_withFilter() {
        long aggressiveCount = db.query("enemies")
            .where_(e -> "AGGRESSIVE".equals(e.field("aiType").asString()))
            .count();
        assertThat(aggressiveCount).isEqualTo(2);
    }

    @Test void query_isEmpty_falseWhenPopulated() {
        assertThat(db.query("enemies").isEmpty()).isFalse();
    }

    @Test void query_first_and_last() {
        assertThat(db.query("enemies").first().field("name").asString()).isEqualTo("Goblin");
        assertThat(db.query("enemies").last().field("name").asString()).isEqualTo("Slime");
    }

    @Test void query_nth_returnsElementAtIndex() {
        assertThat(db.query("enemies").nth(2).field("name").asString()).isEqualTo("Dragon");
    }

    @Test void query_nth_outOfRange_returnsNull() {
        assertThat(db.query("enemies").nth(99)).isNull();
    }

    @Test void query_sumInt_sumsHealth() {
        assertThat(db.query("enemies").select(e -> e.field("health").asLong())
            .stream().mapToLong(Long::longValue).sum()).isEqualTo(1170L);
    }

    @Test void query_groupBy_groupsByField() {
        Map<String, List<MdixValue>> byAi = db.query("enemies").groupBy(e -> e.field("aiType").asString());
        assertThat(byAi.get("AGGRESSIVE")).hasSize(2);
        assertThat(byAi.get("BOSS")).hasSize(1);
        assertThat(byAi.get("PASSIVE")).hasSize(1);
    }

    @Test void query_distinct_removesDuplicateScalars() {
        assertThat(db.query("tags").count()).isEqualTo(4);
        assertThat(db.query("tags").distinct().count()).isEqualTo(3);
    }

    // ── queryMany(pattern) ───────────────────────────────────────────────────

    @Test void queryMany_nonMatchingPattern_returnsEmpty() {
        assertThat(db.queryMany("no.such.path.*").isEmpty()).isTrue();
    }

    @Test void queryMany_globPattern_doesNotThrow() {
        // Exact glob-matcher semantics live in dixscript::Runtime::DixData::select_many;
        // this is a smoke test that the native call round-trips cleanly end to end.
        assertThatCode(() -> db.queryMany("levels.*.enemies")).doesNotThrowAnyException();
    }

    // ── stream() escape hatch ────────────────────────────────────────────────

    @Test void query_stream_dropsToJavaStream() {
        long count = db.query("enemies").stream()
            .filter(e -> e.field("health").asLong() != null && e.field("health").asLong() > 30)
            .count();
        assertThat(count).isEqualTo(3);
    }
}
