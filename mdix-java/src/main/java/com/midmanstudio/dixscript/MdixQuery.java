// MdixQuery.java
package com.midmanstudio.dixscript;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Function;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import java.util.stream.Stream;

/**
 * A chainable, LINQ-style query over a set of {@link MdixValue}s. Mirrors
 * Rust's {@code dixscript::Runtime::DixQuery} method for method — build one
 * from {@link Database#query(String)} / {@link Database#queryMany(String)},
 * then chain filter/sort/group operations before a terminal read.
 * <p>
 * Every intermediate method (other than the terminal reads at the bottom)
 * returns a new {@code MdixQuery} rather than mutating in place, same as
 * the Rust original — safe to branch a query into two different chains
 * from a shared prefix.
 * <p>
 * For anything this doesn't cover, {@link #stream()} drops down to a plain
 * {@code Stream<MdixValue>} for standard Java stream operations.
 * <pre>{@code
 * List<String> highPriorityNames = db.query("tasks")
 *     .where_(v -> v.field("priority").asLong() != null && v.field("priority").asLong() == 3)
 *     .select(v -> v.field("name").asString());
 *
 * long activeCount = db.query("events")
 *     .where_(v -> "ACTIVE".equals(v.field("kind").asString()))
 *     .count();
 * }</pre>
 */
public final class MdixQuery {

    private final List<MdixValue> items;

    MdixQuery(List<MdixValue> items) {
        this.items = items;
    }

    // ── filtering / projection ───────────────────────────────────────────────

    /** Keeps only elements matching {@code predicate}. (LINQ {@code Where}) */
    public MdixQuery where_(Predicate<MdixValue> predicate) {
        return new MdixQuery(items.stream().filter(predicate).collect(Collectors.toList()));
    }

    /** Keeps only elements whose {@code field} equals {@code value}. Shorthand for the common equality-filter case. */
    public MdixQuery whereFieldEquals(String field, MdixValue value) {
        return where_(v -> v.field(field).equals(value));
    }

    /** Discards the first {@code n} elements. (LINQ {@code Skip}) */
    public MdixQuery skip(int n) {
        if (n >= items.size()) return new MdixQuery(new ArrayList<>());
        return new MdixQuery(new ArrayList<>(items.subList(Math.max(n, 0), items.size())));
    }

    /** Keeps only the first {@code n} elements. (LINQ {@code Take}) */
    public MdixQuery take(int n) {
        return new MdixQuery(new ArrayList<>(items.subList(0, Math.max(0, Math.min(n, items.size())))));
    }

    /** Removes duplicate elements (by {@link MdixValue#equals}), preserving first-seen order. */
    public MdixQuery distinct() {
        List<MdixValue> out = new ArrayList<>();
        for (MdixValue v : items) {
            if (!out.contains(v)) out.add(v);
        }
        return new MdixQuery(out);
    }

    /** Projects each element to a new form. (LINQ {@code Select}) */
    public <T> List<T> select(Function<MdixValue, T> map) {
        return items.stream().map(map).collect(Collectors.toList());
    }

    /** Projects each element through a named field. Shorthand for {@code select(v -> v.field(name))}. */
    public List<MdixValue> selectField(String name) {
        return select(v -> v.field(name));
    }

    // ── ordering ──────────────────────────────────────────────────────────────

    /** Sorts ascending by a derived key. (LINQ {@code OrderBy}) Stable sort. */
    public <K extends Comparable<K>> MdixQuery orderBy(Function<MdixValue, K> key) {
        List<MdixValue> sorted = new ArrayList<>(items);
        sorted.sort(Comparator.comparing(key));
        return new MdixQuery(sorted);
    }

    /** Sorts descending by a derived key. (LINQ {@code OrderByDescending}) Stable sort. */
    public <K extends Comparable<K>> MdixQuery orderByDescending(Function<MdixValue, K> key) {
        List<MdixValue> sorted = new ArrayList<>(items);
        sorted.sort(Comparator.comparing(key).reversed());
        return new MdixQuery(sorted);
    }

    // ── grouping ──────────────────────────────────────────────────────────────

    /** Groups elements by a derived key, preserving first-seen key order. (LINQ {@code GroupBy}) */
    public <K> Map<K, List<MdixValue>> groupBy(Function<MdixValue, K> key) {
        Map<K, List<MdixValue>> groups = new LinkedHashMap<>();
        for (MdixValue v : items) {
            groups.computeIfAbsent(key.apply(v), k -> new ArrayList<>()).add(v);
        }
        return groups;
    }

    // ── terminal predicates / aggregates ────────────────────────────────────

    public boolean any(Predicate<MdixValue> predicate) { return items.stream().anyMatch(predicate); }

    public boolean all(Predicate<MdixValue> predicate) { return items.stream().allMatch(predicate); }

    public int count() { return items.size(); }

    public boolean isEmpty() { return items.isEmpty(); }

    public MdixValue first() { return items.isEmpty() ? null : items.get(0); }

    public MdixValue firstOr(MdixValue fallback) { return items.isEmpty() ? fallback : items.get(0); }

    public MdixValue last() { return items.isEmpty() ? null : items.get(items.size() - 1); }

    public MdixValue nth(int index) { return (index < 0 || index >= items.size()) ? null : items.get(index); }

    /** Sum of every element's numeric value, widened to {@code long}. Non-numeric elements contribute nothing. */
    public long sumInt() {
        long sum = 0;
        for (MdixValue v : items) {
            Long l = v.asLong();
            if (l != null) sum += l;
        }
        return sum;
    }

    /** Sum of every element's numeric value, widened to {@code double}. Non-numeric elements contribute nothing. */
    public double sumFloat() {
        double sum = 0;
        for (MdixValue v : items) {
            Double d = v.asDouble();
            if (d != null) sum += d;
        }
        return sum;
    }

    /** Average of every numeric element's {@code double} value. {@code null} on an empty query or one with no numeric elements. */
    public Double avgFloat() {
        double sum = 0;
        int n = 0;
        for (MdixValue v : items) {
            Double d = v.asDouble();
            if (d != null) { sum += d; n++; }
        }
        return n == 0 ? null : sum / n;
    }

    public <K extends Comparable<K>> MdixValue minByKey(Function<MdixValue, K> key) {
        return items.stream().min(Comparator.comparing(key)).orElse(null);
    }

    public <K extends Comparable<K>> MdixValue maxByKey(Function<MdixValue, K> key) {
        return items.stream().max(Comparator.comparing(key)).orElse(null);
    }

    /** The current result set as a new, independent {@code List}. */
    public List<MdixValue> toList() { return new ArrayList<>(items); }

    /** Drops down to a plain {@code Stream<MdixValue>} for anything not covered above. */
    public Stream<MdixValue> stream() { return items.stream(); }
}
