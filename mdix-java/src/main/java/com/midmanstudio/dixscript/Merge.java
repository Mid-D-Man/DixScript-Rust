// Merge.java
package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixJson;
import com.midmanstudio.dixscript.internal.MdixNative;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Weighted, AST-level merge of multiple DixScript sources into one {@link Database}.
 * <p>
 * Uses the real {@code dixscript::Runtime::MdixMerger} under the hood — weighted-priority
 * conflict resolution, per-source conflict reporting, a configurable array-merge strategy,
 * and full type fidelity for every DixScript value type. This is not a shallow JSON-object
 * merge; each source is freshly parsed and merged at the AST level.
 * <pre>{@code
 * Merge.Result result = Merge.sourcesWeighted(
 *     Arrays.asList(baseConfigSrc, overrideConfigSrc),
 *     new double[] { 1.0, 0.5 },
 *     Merge.Strategy.WEIGHTED_PRIORITY,
 *     Merge.ArrayStrategy.CONCAT_DEDUP);
 *
 * try (Database merged = result.database) {
 *     if (result.hasConflicts()) {
 *         for (Merge.Conflict c : result.conflicts) System.out.println(c);
 *     }
 *     int port = merged.getInt("server.port");
 * }
 * }</pre>
 */
public final class Merge {

    private Merge() {}

    /** How to resolve a key defined by more than one source. */
    public enum Strategy {
        /** Each source's weight decides the winner; equal weights fall back to the lower-indexed (primary) source. */
        WEIGHTED_PRIORITY(0),
        /** The lower-indexed source always wins, regardless of weight. */
        PRIMARY_WINS(1),
        /** The higher-indexed source always wins, regardless of weight. */
        SECONDARY_WINS(2),
        /** Any key defined by more than one source is a hard error — the merge fails outright. */
        THROW_ON_CONFLICT(3);

        final int code;
        Strategy(int code) { this.code = code; }
    }

    /** How to combine array-shaped values (plain {@code Array} or {@code GroupArray}) that share a path across sources. */
    public enum ArrayStrategy {
        /** The winning source's array entirely replaces the losing one's. */
        REPLACE(0),
        /** Both arrays are concatenated, winner's items first. */
        CONCAT(1),
        /** Concatenated (winner first), with exact-duplicate primitive values removed. Complex values are never deduped. */
        CONCAT_DEDUP(2);

        final int code;
        ArrayStrategy(int code) { this.code = code; }
    }

    /** One key that more than one source defined, and which source's value won. */
    public static final class Conflict {
        public final String path;
        public final int winningSource;
        /** May be {@code null} if the winning source had no label. */
        public final String winningLabel;

        Conflict(String path, int winningSource, String winningLabel) {
            this.path = path;
            this.winningSource = winningSource;
            this.winningLabel = winningLabel;
        }

        @Override
        public String toString() {
            return "'" + path + "' -> source[" + winningSource + "]" + (winningLabel != null ? " (" + winningLabel + ")" : "");
        }
    }

    /** The merged database plus a report of every key more than one source defined. */
    public static final class Result {
        public final Database database;
        public final List<Conflict> conflicts;

        Result(Database database, List<Conflict> conflicts) {
            this.database = database;
            this.conflicts = conflicts;
        }

        public boolean hasConflicts() { return !conflicts.isEmpty(); }
    }

    // ── Entry points — source strings ───────────────────────────────────────

    /** Merges {@code sources} with auto-descending weights (source 0 highest) and default strategies. */
    public static Result sources(String... sources) {
        return sourcesWeighted(Arrays.asList(sources), null, Strategy.WEIGHTED_PRIORITY, ArrayStrategy.REPLACE);
    }

    /**
     * Merges {@code sources} with explicit per-source {@code weights} (or {@code null} for
     * auto-descending), under the given conflict- and array-merge strategies.
     *
     * @throws MdixException if any source fails to parse, {@code weights} is non-null and
     *         not the same length as {@code sources}, or {@code strategy} is
     *         {@link Strategy#THROW_ON_CONFLICT} and a conflict actually occurs.
     */
    public static Result sourcesWeighted(
        List<String> sources, double[] weights, Strategy strategy, ArrayStrategy arrayStrategy
    ) {
        if (sources == null || sources.isEmpty()) {
            throw new MdixException("Merge: at least one source is required");
        }
        if (weights != null && weights.length != sources.size()) {
            throw new MdixException(
                "Merge: weights.length (" + weights.length + ") must equal sources.size() (" + sources.size() + ")");
        }
        String[] sourcesArr = sources.toArray(new String[0]);
        String[] weightsArr = null;
        if (weights != null) {
            weightsArr = new String[weights.length];
            for (int i = 0; i < weights.length; i++) weightsArr[i] = Double.toString(weights[i]);
        }
        String[] raw = MdixNative.mergeSources(sourcesArr, weightsArr, strategy.code, arrayStrategy.code);
        if (raw == null || raw.length != 2) {
            throw new MdixException("Merge: native call returned no result");
        }
        long handle = Long.parseLong(raw[0]);
        return new Result(new Database(handle), parseConflicts(raw[1]));
    }

    // ── Entry points — already-loaded databases ─────────────────────────────

    /**
     * Merges already-loaded databases with auto-descending weights and default strategies.
     * Each database is round-tripped back to source text via {@code toMdix()} first — an
     * already-loaded {@link Database} only retains resolved data, not the AST it came from,
     * which {@code MdixMerger} needs to re-derive conflict-free weighted output.
     */
    public static Result databases(Database... databases) {
        return databasesWeighted(Arrays.asList(databases), null, Strategy.WEIGHTED_PRIORITY, ArrayStrategy.REPLACE);
    }

    /** As {@link #sourcesWeighted}, but starting from already-loaded databases instead of source text. */
    public static Result databasesWeighted(
        List<Database> databases, double[] weights, Strategy strategy, ArrayStrategy arrayStrategy
    ) {
        if (databases == null || databases.isEmpty()) {
            throw new MdixException("Merge: at least one database is required");
        }
        Converter converter = new Converter();
        List<String> sources = new ArrayList<>(databases.size());
        for (Database db : databases) {
            sources.add(converter.toMdix(db, Converter.FormatMode.DEFAULT));
        }
        return sourcesWeighted(sources, weights, strategy, arrayStrategy);
    }

    private static List<Conflict> parseConflicts(String json) {
        List<MdixValue> arr = MdixJson.parse(json).asArray();
        List<Conflict> out = new ArrayList<>();
        if (arr == null) return out;
        for (MdixValue v : arr) {
            String path = v.field("path").asString();
            Long winningSource = v.field("winningSource").asLong();
            String winningLabel = v.field("winningLabel").asString();
            out.add(new Conflict(path, winningSource == null ? -1 : winningSource.intValue(), winningLabel));
        }
        return out;
    }
}
