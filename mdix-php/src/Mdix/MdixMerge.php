<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

use MidManStudio\Mdix\Internal\NativeLoader;

/**
 * Weighted, AST-level merge of multiple DixScript sources into one MdixDatabase.
 *
 * Uses the real dixscript::Runtime::MdixMerger under the hood — weighted-priority
 * conflict resolution, per-source conflict reporting, a configurable array-merge
 * strategy, and full type fidelity for every DixScript value type. This is not a
 * shallow JSON-object merge; each source is freshly parsed and merged at the AST
 * level.
 *
 *   $result = MdixMerge::sourcesWeighted(
 *       [$baseConfigSrc, $overrideConfigSrc],
 *       [1.0, 0.5],
 *       MergeStrategy::WeightedPriority,
 *       ArrayMergeStrategy::ConcatDedup,
 *   );
 *
 *   try {
 *       if ($result->hasConflicts()) {
 *           foreach ($result->conflicts as $c) { echo $c, "\n"; }
 *       }
 *       $port = $result->database->getInt('server.port');
 *   } finally {
 *       $result->database->close();
 *   }
 *
 * All methods are static — no instantiation required, matching MdixConverter's
 * shape.
 */
final class MdixMerge
{
    private function __construct()
    {
    }

    /** Merges $sources with auto-descending weights (source 0 highest) and default strategies. */
    public static function sources(string ...$sources): MdixMergeResult
    {
        return self::sourcesWeighted($sources, null, MergeStrategy::WeightedPriority, ArrayMergeStrategy::Replace);
    }

    /**
     * Merges $sources with explicit per-source $weights (or null for
     * auto-descending), under the given conflict- and array-merge strategies.
     *
     * @param string[] $sources
     * @param float[]|null $weights must be the same length as $sources when given.
     *
     * @throws MdixError if any source fails to parse, $weights is non-null and
     *         not the same length as $sources, or $strategy is
     *         MergeStrategy::ThrowOnConflict and a conflict actually occurs.
     */
    public static function sourcesWeighted(
        array $sources,
        ?array $weights = null,
        MergeStrategy $strategy = MergeStrategy::WeightedPriority,
        ArrayMergeStrategy $arrayStrategy = ArrayMergeStrategy::Replace,
    ): MdixMergeResult {
        $sources = \array_values($sources);
        $count   = \count($sources);

        if ($count === 0) {
            throw new MdixError('MdixMerge: at least one source is required', ErrorKind::InvalidPath);
        }
        if ($weights !== null && \count($weights) !== $count) {
            throw new MdixError(
                \sprintf('MdixMerge: weights count (%d) must equal sources count (%d)', \count($weights), $count),
                ErrorKind::InvalidPath,
            );
        }

        $ffi = NativeLoader::get();

        // Every native char[] buffer below must stay referenced by a PHP
        // variable for the whole native call — a char* array element only
        // holds the pointer value, not a reference keeping the pointee
        // alive, so an unreferenced buffer is free to be garbage collected
        // (and the C call would then read freed memory) before mdix_merge_
        // sources[_weighted] runs.
        $buffers    = [];
        $sourcesArr = $ffi->new("const char*[{$count}]");
        foreach ($sources as $i => $source) {
            $len = \strlen($source);
            $buf = $ffi->new('char[' . ($len + 1) . ']');
            \FFI::memcpy($buf, $source, $len);
            $buffers[]      = $buf;
            $sourcesArr[$i] = $ffi->cast('const char*', \FFI::addr($buf));
        }

        $conflictsOut = $ffi->new('char*');

        if ($weights === null) {
            $handle = $ffi->mdix_merge_sources(
                $sourcesArr,
                $count,
                $strategy->value,
                $arrayStrategy->value,
                \FFI::addr($conflictsOut),
            );
        } else {
            $weightsArr = $ffi->new("double[{$count}]");
            foreach (\array_values($weights) as $i => $w) {
                $weightsArr[$i] = (float) $w;
            }
            $handle = $ffi->mdix_merge_sources_weighted(
                $sourcesArr,
                $weightsArr,
                $count,
                $strategy->value,
                $arrayStrategy->value,
                \FFI::addr($conflictsOut),
            );
        }

        $conflictsJson = '[]';
        if (!\FFI::isNull($conflictsOut)) {
            $conflictsJson = \FFI::string($conflictsOut);
            $ffi->mdix_free_string($conflictsOut);
        }

        if ($handle === null) {
            $ptr = $ffi->mdix_get_last_error();
            $msg = $ptr !== null ? $ptr : 'unknown native error';
            throw MdixError::fromMessage("[mdix:mergeSources] {$msg}");
        }

        return new MdixMergeResult(MdixDatabase::adopt($handle), self::parseConflicts($conflictsJson));
    }

    /**
     * Merges already-loaded databases with auto-descending weights and default
     * strategies. Each database is round-tripped back to source text via
     * toMdix() first — an already-loaded MdixDatabase only retains resolved
     * data, not the AST it came from, which MdixMerger needs to re-derive
     * conflict-free weighted output.
     */
    public static function databases(MdixDatabase ...$databases): MdixMergeResult
    {
        return self::databasesWeighted($databases, null, MergeStrategy::WeightedPriority, ArrayMergeStrategy::Replace);
    }

    /**
     * As sourcesWeighted(), but starting from already-loaded databases
     * instead of source text.
     *
     * @param MdixDatabase[] $databases
     * @param float[]|null $weights
     */
    public static function databasesWeighted(
        array $databases,
        ?array $weights = null,
        MergeStrategy $strategy = MergeStrategy::WeightedPriority,
        ArrayMergeStrategy $arrayStrategy = ArrayMergeStrategy::Replace,
    ): MdixMergeResult {
        $databases = \array_values($databases);
        if ($databases === []) {
            throw new MdixError('MdixMerge: at least one database is required', ErrorKind::InvalidPath);
        }

        $sources = \array_map(
            static fn (MdixDatabase $db): string => $db->toMdix(FormatMode::Default),
            $databases,
        );

        return self::sourcesWeighted($sources, $weights, $strategy, $arrayStrategy);
    }

    /** @return MdixMergeConflict[] */
    private static function parseConflicts(string $json): array
    {
        $decoded = \json_decode($json, associative: true);
        if (!\is_array($decoded)) {
            return [];
        }

        $out = [];
        foreach ($decoded as $entry) {
            if (!\is_array($entry)) {
                continue;
            }
            $out[] = new MdixMergeConflict(
                (string) ($entry['path'] ?? ''),
                (int) ($entry['winningSource'] ?? -1),
                isset($entry['winningLabel']) ? (string) $entry['winningLabel'] : null,
            );
        }
        return $out;
    }
}
