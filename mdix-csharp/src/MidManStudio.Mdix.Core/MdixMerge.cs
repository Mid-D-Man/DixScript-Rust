// csharp/src/MidManStudio.Mdix.Core/MdixMerge.cs
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using MidManStudio.DixScript.Native;
using NativeMergeStrategy = MidManStudio.DixScript.Native.MdixMergeStrategy;
using NativeArrayStrategy = MidManStudio.DixScript.Native.ArrayMergeStrategy;

namespace MidManStudio.Mdix.Core
{
    /// <summary>
    /// Controls how key conflicts are resolved when merging two or more databases.
    /// Mirrors dixscript::Runtime::MdixMergeStrategy exactly (see mdix-ffi's
    /// MdixMergeStrategy / merge.rs for the authoritative semantics) — kept as a
    /// separate local enum rather than exposing the generated native one directly
    /// so the value names read naturally from C#, same pattern as MdixFormatMode
    /// in MdixConverter.cs.
    /// </summary>
    public enum MdixMergeStrategy
    {
        /// <summary>
        /// Each source's weight decides the winner; equal weights fall back to
        /// the lower-indexed (primary) source. Default — matches the Rust core's
        /// own default. When no explicit weights are given (Merge / MergeAll /
        /// MergeSources), sources are auto-weighted in descending order: the
        /// first gets 1.0, the last gets the lowest.
        /// </summary>
        WeightedPriority = 0,
        /// <summary>The first (lowest-indexed) source always wins, regardless of weight.</summary>
        PrimaryWins = 1,
        /// <summary>The last (highest-indexed) source always wins, regardless of weight.</summary>
        SecondaryWins = 2,
        /// <summary>Any key defined by more than one source fails the whole merge.</summary>
        ThrowOnConflict = 3,
    }

    /// <summary>
    /// Controls how two array-valued entries (a GroupArray, or an array-valued
    /// simple property) that share a path across sources get combined.
    /// </summary>
    public enum MdixArrayMergeStrategy
    {
        /// <summary>The winning source's array entirely replaces the losing one's.</summary>
        Replace = 0,
        /// <summary>Both arrays are concatenated, winner's items first.</summary>
        Concat = 1,
        /// <summary>
        /// Concatenated (winner first), with exact-duplicate primitive values
        /// removed. Complex values (objects, nested arrays) are never deduped. Default.
        /// </summary>
        ConcatDedup = 2,
    }

    /// <summary>A single resolved key conflict from a merge.</summary>
    public sealed class MdixMergeConflict
    {
        /// <summary>Dotted path of the conflicting key, e.g. <c>"server.host"</c>.</summary>
        public string Path { get; }
        /// <summary>0-based index of the winning source, in the order passed to the merge call.</summary>
        public int WinningSource { get; }
        /// <summary>Label of the winning source, if one is available (see remarks on each merge method for how labels are assigned).</summary>
        public string? WinningLabel { get; }

        internal MdixMergeConflict(string path, int winningSource, string? winningLabel)
        {
            Path = path;
            WinningSource = winningSource;
            WinningLabel = winningLabel;
        }

        public override string ToString() =>
            WinningLabel is not null
                ? $"[Conflict] '{Path}' -> source[{WinningSource}] ('{WinningLabel}') won"
                : $"[Conflict] '{Path}' -> source[{WinningSource}] won";
    }

    /// <summary>
    /// The result of a successful merge: the combined database plus every
    /// conflict that was resolved along the way. Disposing the outcome disposes
    /// <see cref="Database"/>.
    /// </summary>
    public sealed class MdixMergeOutcome : IDisposable
    {
        /// <summary>The merged database. Caller owns it — dispose when done (or dispose this outcome).</summary>
        public MdixDatabase Database { get; }
        /// <summary>Every key conflict that was resolved during the merge. Empty if none were.</summary>
        public IReadOnlyList<MdixMergeConflict> Conflicts { get; }

        internal MdixMergeOutcome(MdixDatabase database, IReadOnlyList<MdixMergeConflict> conflicts)
        {
            Database = database;
            Conflicts = conflicts;
        }

        public void Dispose() => Database?.Dispose();
    }

    /// <summary>
    /// Merges two or more loaded DixScript databases into a single new database.
    ///
    /// This calls straight into the real AST-level merger
    /// (dixscript::Runtime::MdixMerger, via mdix-ffi's mdix_merge_sources /
    /// mdix_merge_sources_weighted) — no JSON anywhere in the merge path itself.
    /// Every DixScript value type survives exactly: Long, Float, Double,
    /// HexColor, Blob, Regex, Date, Timestamp, Enum. You also get a real
    /// per-key conflict report and weighted-priority resolution, neither of
    /// which the old implementation had.
    ///
    /// Before this, MdixMerge.cs's only way to reach dixscript at all was
    /// mdix_to_json -> hand-written recursive deep-merge in managed C# on the
    /// resulting flat hashmap -> hand-formatted back into an @DATA(...) source
    /// string -> MdixDatabase.LoadStr. mdix-ffi had no merge exports at all, so
    /// there was no other option at the time. That path lost type information
    /// on every round trip (mdix_to_json emits enums as plain integers, and
    /// HexColor / Blob / Regex / Date / Timestamp all flatten to JSON strings
    /// or numbers with no tag saying which DixScript type they came from — so
    /// a merged value would silently come back as the wrong type), and had no
    /// conflict reporting, array-merge strategies, or weighted priority.
    ///
    /// <see cref="MergeSources"/> / <see cref="MergeSourcesWeighted"/> take raw
    /// .mdix source strings and never touch JSON at all. <see cref="Merge"/> /
    /// <see cref="MergeAll"/> (already-loaded databases) get there through
    /// mdix_to_mdix — full-fidelity .mdix text, not JSON — mirroring
    /// mdix-wasm's MdixDatabase.mergeWith, which does the same to_mdix()
    /// round-trip internally for the identical reason (an already-loaded
    /// handle only retains the resolved data, not the AST MdixMerger needs).
    /// <see cref="MergeJson"/> is the one place JSON still appears, and that's
    /// inherent to what it does — importing an actual JSON blob — not a
    /// leftover implementation shortcut.
    /// </summary>
    public static unsafe class MdixMerge
    {
        // ── Public API — source strings, no JSON at all ─────────────────────────

        /// <summary>
        /// Merges two or more .mdix source strings using the real AST-level
        /// merger. Sources are auto-weighted in descending order — the first
        /// gets weight 1.0, the last gets the lowest (only matters under
        /// <see cref="MdixMergeStrategy.WeightedPriority"/>). Use
        /// <see cref="MergeSourcesWeighted"/> for explicit per-source weights.
        /// Conflict labels are auto-generated as <c>"source[i]"</c>.
        /// </summary>
        public static MdixResult<MdixMergeOutcome> MergeSources(
            IReadOnlyList<string> sources,
            MdixMergeStrategy strategy = MdixMergeStrategy.WeightedPriority,
            MdixArrayMergeStrategy arrayStrategy = MdixArrayMergeStrategy.ConcatDedup)
        {
            if (sources is null || sources.Count == 0)
                return MdixError.NativeError("MergeSources: sources cannot be null or empty.");

            MdixNative.mdix_clear_error();

            var arrayPtr = AllocSourceArray(sources, out var stringPtrs);
            byte* conflictsPtr = null;
            try
            {
                void* handle = MdixNative.mdix_merge_sources(
                    (byte**)arrayPtr,
                    sources.Count,
                    (NativeMergeStrategy)(int)strategy,
                    (NativeArrayStrategy)(int)arrayStrategy,
                    &conflictsPtr);

                if (handle == null)
                    return MdixError.NativeError(ReadLastError() ?? "MergeSources: merge failed.");

                var conflicts = ParseConflicts(conflictsPtr);
                return MdixResult<MdixMergeOutcome>.Ok(
                    new MdixMergeOutcome(MdixDatabase.FromRawHandle(handle), conflicts));
            }
            finally
            {
                FreeSourceArray(arrayPtr, stringPtrs);
                if (conflictsPtr != null) MdixNative.mdix_free_string(conflictsPtr);
            }
        }

        /// <summary>
        /// Merges .mdix source strings with explicit per-source weights. Higher
        /// weight wins under <see cref="MdixMergeStrategy.WeightedPriority"/>.
        /// Conflict labels are auto-generated as <c>"source[i]"</c>.
        /// </summary>
        public static MdixResult<MdixMergeOutcome> MergeSourcesWeighted(
            IReadOnlyList<(string source, double weight)> sources,
            MdixMergeStrategy strategy = MdixMergeStrategy.WeightedPriority,
            MdixArrayMergeStrategy arrayStrategy = MdixArrayMergeStrategy.ConcatDedup)
        {
            if (sources is null || sources.Count == 0)
                return MdixError.NativeError("MergeSourcesWeighted: sources cannot be null or empty.");

            MdixNative.mdix_clear_error();

            var texts = new string[sources.Count];
            var weights = new double[sources.Count];
            for (int i = 0; i < sources.Count; i++)
            {
                texts[i] = sources[i].source;
                weights[i] = sources[i].weight;
            }

            var arrayPtr = AllocSourceArray(texts, out var stringPtrs);
            byte* conflictsPtr = null;
            try
            {
                fixed (double* weightsPtr = weights)
                {
                    void* handle = MdixNative.mdix_merge_sources_weighted(
                        (byte**)arrayPtr,
                        weightsPtr,
                        texts.Length,
                        (NativeMergeStrategy)(int)strategy,
                        (NativeArrayStrategy)(int)arrayStrategy,
                        &conflictsPtr);

                    if (handle == null)
                        return MdixError.NativeError(ReadLastError() ?? "MergeSourcesWeighted: merge failed.");

                    var conflicts = ParseConflicts(conflictsPtr);
                    return MdixResult<MdixMergeOutcome>.Ok(
                        new MdixMergeOutcome(MdixDatabase.FromRawHandle(handle), conflicts));
                }
            }
            finally
            {
                FreeSourceArray(arrayPtr, stringPtrs);
                if (conflictsPtr != null) MdixNative.mdix_free_string(conflictsPtr);
            }
        }

        // ── Public API — already-loaded databases (via .mdix text, not JSON) ────

        /// <summary>
        /// Merges <paramref name="secondary"/> into <paramref name="primary"/>
        /// (weight 1.0 vs 0.5) and returns a new combined database. Neither
        /// input database is modified or disposed. Reaches the merger via
        /// mdix_to_mdix (full-fidelity .mdix text) on each input, not JSON.
        /// </summary>
        public static MdixResult<MdixMergeOutcome> Merge(
            MdixDatabase primary,
            MdixDatabase secondary,
            MdixMergeStrategy strategy = MdixMergeStrategy.WeightedPriority,
            MdixArrayMergeStrategy arrayStrategy = MdixArrayMergeStrategy.ConcatDedup)
        {
            if (primary is null) return MdixError.NativeError("Merge: primary cannot be null.");
            if (secondary is null) return MdixError.NativeError("Merge: secondary cannot be null.");

            var primarySrc = MdixConverter.ToMdix(primary);
            if (primarySrc.IsFailure) return MdixResult<MdixMergeOutcome>.Err(primarySrc.Error);

            var secondarySrc = MdixConverter.ToMdix(secondary);
            if (secondarySrc.IsFailure) return MdixResult<MdixMergeOutcome>.Err(secondarySrc.Error);

            return MergeSourcesWeighted(
                new (string, double)[]
                {
                    (primarySrc.SuccessResult, 1.0),
                    (secondarySrc.SuccessResult, 0.5),
                },
                strategy, arrayStrategy);
        }

        /// <summary>
        /// Merges all databases in <paramref name="databases"/> left-to-right,
        /// auto-weighted in descending order (matches <see cref="MergeSources"/>).
        /// None of the input databases are modified or disposed.
        /// </summary>
        public static MdixResult<MdixMergeOutcome> MergeAll(
            IEnumerable<MdixDatabase> databases,
            MdixMergeStrategy strategy = MdixMergeStrategy.WeightedPriority,
            MdixArrayMergeStrategy arrayStrategy = MdixArrayMergeStrategy.ConcatDedup)
        {
            if (databases is null)
                return MdixError.NativeError("MergeAll: databases cannot be null.");

            var sources = new List<string>();
            int index = 0;
            foreach (var db in databases)
            {
                if (db is null)
                    return MdixError.NativeError($"MergeAll: database at index {index} is null.");

                var srcResult = MdixConverter.ToMdix(db);
                if (srcResult.IsFailure) return MdixResult<MdixMergeOutcome>.Err(srcResult.Error);

                sources.Add(srcResult.SuccessResult);
                index++;
            }

            if (sources.Count == 0)
                return MdixError.NativeError("MergeAll: databases sequence was empty.");

            // A single source is a valid, well-defined case on the native side
            // (MdixMerger::merge_all returns it unchanged, is_success = true) —
            // no special-casing needed here unlike the old JSON-based version.
            return MergeSources(sources, strategy, arrayStrategy);
        }

        /// <summary>
        /// Merges a raw JSON object string into an existing database. This is
        /// the one merge method that still touches JSON — it has to, since the
        /// input already is JSON. <paramref name="secondaryJson"/> is parsed via
        /// the existing mdix_from_json path (MdixConverter.FromJson) and then
        /// merged against <paramref name="primary"/> the same way
        /// <see cref="Merge"/> does.
        /// </summary>
        public static MdixResult<MdixMergeOutcome> MergeJson(
            MdixDatabase primary,
            string secondaryJson,
            MdixMergeStrategy strategy = MdixMergeStrategy.WeightedPriority,
            MdixArrayMergeStrategy arrayStrategy = MdixArrayMergeStrategy.ConcatDedup)
        {
            if (primary is null)
                return MdixError.NativeError("MergeJson: primary cannot be null.");
            if (string.IsNullOrEmpty(secondaryJson))
                return MdixError.NativeError("MergeJson: secondaryJson cannot be null or empty.");

            var secondaryResult = MdixConverter.FromJson(secondaryJson);
            if (secondaryResult.IsFailure) return MdixResult<MdixMergeOutcome>.Err(secondaryResult.Error);

            using var secondary = secondaryResult.SuccessResult;
            return Merge(primary, secondary, strategy, arrayStrategy);
        }

        // ── Private — conflict report parsing ────────────────────────────────

        /// <summary>
        /// Parses the `[{"path":...,"winningSource":...,"winningLabel":...}, ...]`
        /// JSON mdix_merge_sources[_weighted] writes to out_conflicts_json.
        /// This JSON usage is unrelated to the type-fidelity problem the rest of
        /// this file's doc comment describes — it's a small diagnostic report
        /// (a string, an int, an optional string), not DixScript data, so none
        /// of JSON's type-flattening limitations apply here.
        /// </summary>
        private static List<MdixMergeConflict> ParseConflicts(byte* jsonPtr)
        {
            var conflicts = new List<MdixMergeConflict>();
            if (jsonPtr == null) return conflicts;

            var json = Marshal.PtrToStringUTF8((IntPtr)jsonPtr);
            if (string.IsNullOrEmpty(json)) return conflicts;

            using var doc = JsonDocument.Parse(json);
            foreach (var el in doc.RootElement.EnumerateArray())
            {
                var path = el.GetProperty("path").GetString() ?? string.Empty;
                var winningSource = el.GetProperty("winningSource").GetInt32();
                string? winningLabel = el.TryGetProperty("winningLabel", out var labelEl)
                    && labelEl.ValueKind != JsonValueKind.Null
                        ? labelEl.GetString()
                        : null;
                conflicts.Add(new MdixMergeConflict(path, winningSource, winningLabel));
            }
            return conflicts;
        }

        // ── Private — unmanaged string-array marshaling ──────────────────────
        //
        // mdix_merge_sources[_weighted] take `const char* const* sources` (an
        // array of null-terminated UTF-8 strings) — a dynamic, runtime-sized
        // array of pointers can't be pinned with a compile-time-fixed number of
        // `fixed` statements, so each string (and the array of pointers itself)
        // is copied into unmanaged memory instead and explicitly freed in a
        // finally block. Unlike MdixStringCache.GetUtf8Bytes, these are never
        // cached — merge sources can be arbitrarily large .mdix file contents,
        // not small reusable paths.

        private static IntPtr AllocSourceArray(IReadOnlyList<string> sources, out IntPtr[] stringPtrs)
        {
            stringPtrs = new IntPtr[sources.Count];
            int allocated = 0;
            try
            {
                for (int i = 0; i < sources.Count; i++)
                {
                    var bytes = Encoding.UTF8.GetBytes(sources[i]);
                    var ptr = Marshal.AllocHGlobal(bytes.Length + 1);
                    Marshal.Copy(bytes, 0, ptr, bytes.Length);
                    Marshal.WriteByte(ptr, bytes.Length, 0); // null terminator
                    stringPtrs[i] = ptr;
                    allocated++;
                }

                var arrayPtr = Marshal.AllocHGlobal(IntPtr.Size * sources.Count);
                Marshal.Copy(stringPtrs, 0, arrayPtr, sources.Count);
                return arrayPtr;
            }
            catch
            {
                // Something failed partway through (most likely OutOfMemoryException
                // from AllocHGlobal on a very large source set) -- free whatever we
                // did manage to allocate before this rethrows, rather than leaking it.
                for (int i = 0; i < allocated; i++) Marshal.FreeHGlobal(stringPtrs[i]);
                throw;
            }
        }

        private static void FreeSourceArray(IntPtr arrayPtr, IntPtr[] stringPtrs)
        {
            foreach (var p in stringPtrs)
                if (p != IntPtr.Zero) Marshal.FreeHGlobal(p);
            if (arrayPtr != IntPtr.Zero) Marshal.FreeHGlobal(arrayPtr);
        }

        // ── Private — error reading (same pattern as MdixDatabase.cs / MdixConverter.cs) ──

        private static string? ReadLastError()
        {
            var ptr = MdixNative.mdix_get_last_error();
            return ptr == null ? null : Marshal.PtrToStringUTF8((IntPtr)ptr);
        }
    }
}
