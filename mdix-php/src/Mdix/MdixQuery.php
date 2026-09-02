<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/**
 * A chainable, LINQ/Stream-style query over decoded DixScript values.
 *
 * Build one from MdixDatabase::query() / MdixDatabase::queryMany(), then
 * chain filter/sort/group operations before a terminal read. Every
 * intermediate method returns a new MdixQuery rather than mutating in
 * place — safe to branch a query into two different chains from a shared
 * prefix.
 *
 * Deliberately built on plain PHP arrays (from json_decode), not a custom
 * value-wrapper type the way the Java/C++ bindings need one — PHP arrays
 * are already the dynamic, freely-indexable structure those bindings had
 * to build from scratch, so there's nothing to add here beyond the
 * fluent chaining itself.
 *
 *   $highPriority = $db->query('tasks')
 *       ->where(fn($t) => ($t['priority'] ?? 0) === 3)
 *       ->orderByDescending(fn($t) => $t['priority'])
 *       ->select(fn($t) => $t['name']);
 *
 * For anything this doesn't cover, toArray() drops back to a plain PHP
 * array for array_map/array_filter/usort etc.
 */
final class MdixQuery
{
    /** @param array<int,mixed> $items */
    public function __construct(private readonly array $items)
    {
    }

    // ── filtering / projection ───────────────────────────────────────────────

    /** Keeps only elements matching $predicate. @param callable(mixed): bool $predicate */
    public function where(callable $predicate): self
    {
        return new self(\array_values(\array_filter($this->items, $predicate)));
    }

    /** Keeps only elements whose $field equals $value (assumes array-shaped elements). */
    public function whereFieldEquals(string $field, mixed $value): self
    {
        return $this->where(
            fn (mixed $item): bool => \is_array($item) && (($item[$field] ?? null) === $value)
        );
    }

    /** Discards the first $n elements. */
    public function skip(int $n): self
    {
        return new self(\array_slice($this->items, \max($n, 0)));
    }

    /** Keeps only the first $n elements. */
    public function take(int $n): self
    {
        return new self(\array_slice($this->items, 0, \max($n, 0)));
    }

    /** Removes duplicate elements (compared by value, not identity), preserving first-seen order. */
    public function distinct(): self
    {
        $seen = [];
        $out  = [];
        foreach ($this->items as $item) {
            $key = \serialize($item);
            if (!isset($seen[$key])) {
                $seen[$key] = true;
                $out[]      = $item;
            }
        }
        return new self($out);
    }

    /**
     * Projects each element to a new form.
     * @param callable(mixed): mixed $map
     * @return array<int,mixed>
     */
    public function select(callable $map): array
    {
        return \array_map($map, $this->items);
    }

    /**
     * Projects each element through a named field (assumes array-shaped elements).
     * @return array<int,mixed>
     */
    public function selectField(string $field): array
    {
        return $this->select(fn (mixed $item): mixed => \is_array($item) ? ($item[$field] ?? null) : null);
    }

    // ── ordering ──────────────────────────────────────────────────────────────

    /** Sorts ascending by a derived key. Stable (PHP's usort is stable since 8.0). @param callable(mixed): mixed $key */
    public function orderBy(callable $key): self
    {
        $items = $this->items;
        \usort($items, fn (mixed $a, mixed $b): int => $key($a) <=> $key($b));
        return new self($items);
    }

    /** Sorts descending by a derived key. @param callable(mixed): mixed $key */
    public function orderByDescending(callable $key): self
    {
        $items = $this->items;
        \usort($items, fn (mixed $a, mixed $b): int => $key($b) <=> $key($a));
        return new self($items);
    }

    // ── grouping ──────────────────────────────────────────────────────────────

    /**
     * Groups elements by a derived key, preserving first-seen key order.
     * @param callable(mixed): array-key $key
     * @return array<array-key, array<int,mixed>>
     */
    public function groupBy(callable $key): array
    {
        $groups = [];
        foreach ($this->items as $item) {
            $groups[$key($item)][] = $item;
        }
        return $groups;
    }

    // ── terminal predicates / aggregates ────────────────────────────────────

    /** @param callable(mixed): bool $predicate */
    public function any(callable $predicate): bool
    {
        foreach ($this->items as $item) {
            if ($predicate($item)) {
                return true;
            }
        }
        return false;
    }

    /** @param callable(mixed): bool $predicate */
    public function all(callable $predicate): bool
    {
        foreach ($this->items as $item) {
            if (!$predicate($item)) {
                return false;
            }
        }
        return true;
    }

    public function count(): int
    {
        return \count($this->items);
    }

    public function isEmpty(): bool
    {
        return $this->items === [];
    }

    public function first(): mixed
    {
        return $this->items[0] ?? null;
    }

    public function firstOr(mixed $fallback): mixed
    {
        return $this->items === [] ? $fallback : $this->items[0];
    }

    public function last(): mixed
    {
        return $this->items === [] ? null : $this->items[\count($this->items) - 1];
    }

    public function nth(int $index): mixed
    {
        return $this->items[$index] ?? null;
    }

    /** Sum of every numeric element, widened to int. Non-numeric elements contribute nothing. */
    public function sumInt(): int
    {
        return (int) \array_sum(\array_filter($this->items, \is_numeric(...)));
    }

    /** Sum of every numeric element, widened to float. Non-numeric elements contribute nothing. */
    public function sumFloat(): float
    {
        return (float) \array_sum(\array_filter($this->items, \is_numeric(...)));
    }

    /** Average of every numeric element. Null on an empty query or one with no numeric elements. */
    public function avgFloat(): ?float
    {
        $numeric = \array_filter($this->items, \is_numeric(...));
        return $numeric === [] ? null : (float) \array_sum($numeric) / \count($numeric);
    }

    /** @param callable(mixed): mixed $key */
    public function minByKey(callable $key): mixed
    {
        $best = null;
        $bestKey = null;
        $found = false;
        foreach ($this->items as $item) {
            $k = $key($item);
            if (!$found || $k < $bestKey) {
                $best = $item;
                $bestKey = $k;
                $found = true;
            }
        }
        return $best;
    }

    /** @param callable(mixed): mixed $key */
    public function maxByKey(callable $key): mixed
    {
        $best = null;
        $bestKey = null;
        $found = false;
        foreach ($this->items as $item) {
            $k = $key($item);
            if (!$found || $k > $bestKey) {
                $best = $item;
                $bestKey = $k;
                $found = true;
            }
        }
        return $best;
    }

    /** @return array<int,mixed> */
    public function toArray(): array
    {
        return $this->items;
    }
}
