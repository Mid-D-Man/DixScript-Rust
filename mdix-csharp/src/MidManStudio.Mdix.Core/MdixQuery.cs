using System;
using System.Collections.Generic;
using System.Linq;

namespace MidManStudio.Mdix.Core
{
    /// <summary>
    /// LINQ-style query extensions over DixScript array and collection data.
    /// All methods load the full typed list internally then apply operations in managed code.
    /// Usage:
    /// <code>
    /// var boss    = db.QueryFirst&lt;Enemy&gt;("enemies", e => e.AiType == "BOSS").OrThrow();
    /// var heavies = db.QueryWhere&lt;Enemy&gt;("enemies", e => e.Hp &gt; 500).OrThrow();
    /// var names   = db.QuerySelect&lt;Enemy, string&gt;("enemies", e => e.Name).OrThrow();
    /// int count   = db.QueryCount&lt;Enemy&gt;("enemies", e => e.AiType == "BOSS").OrThrow();
    /// bool any    = db.QueryAny&lt;Enemy&gt;("enemies", e => e.Hp &gt; 9000).OrThrow();
    /// var sorted  = db.QueryOrderBy&lt;Enemy, int&gt;("enemies", e => e.Hp).OrThrow();
    /// </code>
    /// For anything more complex, call <see cref="MdixDatabase.GetArray{T}"/> and use
    /// standard LINQ on the returned <c>List&lt;T&gt;</c> directly.
    /// </summary>
    public static class MdixQueryExtensions
    {
        // ── QueryFirst ────────────────────────────────────────────────────────

        /// <summary>
        /// Returns the first item in the array at <paramref name="path"/>.
        /// Returns a <see cref="MdixErrorKind.NotFound"/> error if the array is empty.
        /// </summary>
        public static MdixResult<T> QueryFirst<T>(this MdixDatabase db, string path)
        {
            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<T>.Err(r.Error);

            var list = r.SuccessResult;
            if (list.Count == 0)
                return MdixError.NotFound($"{path}[0]");

            return MdixResult<T>.Ok(list[0]);
        }

        /// <summary>
        /// Returns the first item in the array at <paramref name="path"/> that satisfies
        /// <paramref name="predicate"/>.
        /// Returns a <see cref="MdixErrorKind.NotFound"/> error if no item matches.
        /// </summary>
        public static MdixResult<T> QueryFirst<T>(
            this MdixDatabase db,
            string path,
            Func<T, bool> predicate)
        {
            if (predicate is null) throw new ArgumentNullException(nameof(predicate));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<T>.Err(r.Error);

            foreach (var item in r.SuccessResult)
            {
                if (predicate(item))
                    return MdixResult<T>.Ok(item);
            }

            return MdixError.NotFound($"{path}[first matching predicate]");
        }

        // ── QueryLast ─────────────────────────────────────────────────────────

        /// <summary>
        /// Returns the last item in the array at <paramref name="path"/>.
        /// Returns a <see cref="MdixErrorKind.NotFound"/> error if the array is empty.
        /// </summary>
        public static MdixResult<T> QueryLast<T>(this MdixDatabase db, string path)
        {
            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<T>.Err(r.Error);

            var list = r.SuccessResult;
            if (list.Count == 0)
                return MdixError.NotFound($"{path}[last]");

            return MdixResult<T>.Ok(list[list.Count - 1]);
        }

        /// <summary>
        /// Returns the last item in the array at <paramref name="path"/> that satisfies
        /// <paramref name="predicate"/>.
        /// Returns a <see cref="MdixErrorKind.NotFound"/> error if no item matches.
        /// </summary>
        public static MdixResult<T> QueryLast<T>(
            this MdixDatabase db,
            string path,
            Func<T, bool> predicate)
        {
            if (predicate is null) throw new ArgumentNullException(nameof(predicate));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<T>.Err(r.Error);

            T? found = default;
            bool any = false;

            foreach (var item in r.SuccessResult)
            {
                if (predicate(item)) { found = item; any = true; }
            }

            return any
                ? MdixResult<T>.Ok(found!)
                : MdixError.NotFound($"{path}[last matching predicate]");
        }

        // ── QuerySingle ───────────────────────────────────────────────────────

        /// <summary>
        /// Returns the single item in the array that satisfies <paramref name="predicate"/>.
        /// Returns an error if zero or more than one item matches.
        /// </summary>
        public static MdixResult<T> QuerySingle<T>(
            this MdixDatabase db,
            string path,
            Func<T, bool> predicate)
        {
            if (predicate is null) throw new ArgumentNullException(nameof(predicate));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<T>.Err(r.Error);

            T? match = default;
            int count = 0;

            foreach (var item in r.SuccessResult)
            {
                if (!predicate(item)) continue;
                match = item;
                count++;
                if (count > 1)
                    return MdixError.NativeError(
                        $"QuerySingle on '{path}': more than one item matched the predicate.");
            }

            return count == 1
                ? MdixResult<T>.Ok(match!)
                : MdixError.NotFound($"{path}[single matching predicate]");
        }

        // ── QueryWhere ────────────────────────────────────────────────────────

        /// <summary>
        /// Returns all items in the array at <paramref name="path"/> that satisfy
        /// <paramref name="predicate"/>. Returns an empty list (not an error) when no items match.
        /// </summary>
        public static MdixResult<List<T>> QueryWhere<T>(
            this MdixDatabase db,
            string path,
            Func<T, bool> predicate)
        {
            if (predicate is null) throw new ArgumentNullException(nameof(predicate));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<List<T>>.Err(r.Error);

            var result = new List<T>();
            foreach (var item in r.SuccessResult)
            {
                if (predicate(item)) result.Add(item);
            }

            return MdixResult<List<T>>.Ok(result);
        }

        // ── QuerySelect ───────────────────────────────────────────────────────

        /// <summary>
        /// Projects each item in the array at <paramref name="path"/> to a new form using
        /// <paramref name="selector"/>.
        /// </summary>
        public static MdixResult<List<TResult>> QuerySelect<T, TResult>(
            this MdixDatabase db,
            string path,
            Func<T, TResult> selector)
        {
            if (selector is null) throw new ArgumentNullException(nameof(selector));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<List<TResult>>.Err(r.Error);

            var list   = r.SuccessResult;
            var result = new List<TResult>(list.Count);
            foreach (var item in list)
                result.Add(selector(item));

            return MdixResult<List<TResult>>.Ok(result);
        }

        // ── QueryCount ────────────────────────────────────────────────────────

        /// <summary>
        /// Returns the number of items in the array at <paramref name="path"/>.
        /// When <paramref name="predicate"/> is provided, counts only matching items.
        /// </summary>
        public static MdixResult<int> QueryCount<T>(
            this MdixDatabase db,
            string path,
            Func<T, bool>? predicate = null)
        {
            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<int>.Err(r.Error);

            int count = predicate == null
                ? r.SuccessResult.Count
                : r.SuccessResult.Count(predicate);

            return MdixResult<int>.Ok(count);
        }

        // ── QueryAny ─────────────────────────────────────────────────────────

        /// <summary>
        /// Returns true if any item in the array at <paramref name="path"/> satisfies
        /// <paramref name="predicate"/>.
        /// </summary>
        public static MdixResult<bool> QueryAny<T>(
            this MdixDatabase db,
            string path,
            Func<T, bool> predicate)
        {
            if (predicate is null) throw new ArgumentNullException(nameof(predicate));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<bool>.Err(r.Error);

            foreach (var item in r.SuccessResult)
            {
                if (predicate(item))
                    return MdixResult<bool>.Ok(true);
            }

            return MdixResult<bool>.Ok(false);
        }

        // ── QueryAll ──────────────────────────────────────────────────────────

        /// <summary>
        /// Returns true if every item in the array at <paramref name="path"/> satisfies
        /// <paramref name="predicate"/>, or if the array is empty.
        /// </summary>
        public static MdixResult<bool> QueryAll<T>(
            this MdixDatabase db,
            string path,
            Func<T, bool> predicate)
        {
            if (predicate is null) throw new ArgumentNullException(nameof(predicate));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<bool>.Err(r.Error);

            foreach (var item in r.SuccessResult)
            {
                if (!predicate(item))
                    return MdixResult<bool>.Ok(false);
            }

            return MdixResult<bool>.Ok(true);
        }

        // ── QueryOrderBy ──────────────────────────────────────────────────────

        /// <summary>
        /// Returns the array at <paramref name="path"/> sorted ascending by
        /// <paramref name="keySelector"/>. Does not modify the original data.
        /// </summary>
        public static MdixResult<List<T>> QueryOrderBy<T, TKey>(
            this MdixDatabase db,
            string path,
            Func<T, TKey> keySelector)
        {
            if (keySelector is null) throw new ArgumentNullException(nameof(keySelector));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<List<T>>.Err(r.Error);

            return MdixResult<List<T>>.Ok(r.SuccessResult.OrderBy(keySelector).ToList());
        }

        /// <summary>
        /// Returns the array at <paramref name="path"/> sorted descending by
        /// <paramref name="keySelector"/>. Does not modify the original data.
        /// </summary>
        public static MdixResult<List<T>> QueryOrderByDescending<T, TKey>(
            this MdixDatabase db,
            string path,
            Func<T, TKey> keySelector)
        {
            if (keySelector is null) throw new ArgumentNullException(nameof(keySelector));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<List<T>>.Err(r.Error);

            return MdixResult<List<T>>.Ok(r.SuccessResult.OrderByDescending(keySelector).ToList());
        }

        // ── QueryDistinct ─────────────────────────────────────────────────────

        /// <summary>
        /// Returns the array at <paramref name="path"/> with duplicate items removed,
        /// using the default equality comparer for <typeparamref name="T"/>.
        /// </summary>
        public static MdixResult<List<T>> QueryDistinct<T>(this MdixDatabase db, string path)
        {
            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<List<T>>.Err(r.Error);

            return MdixResult<List<T>>.Ok(r.SuccessResult.Distinct().ToList());
        }

        // ── QueryTake / QuerySkip ──────────────────────────────────────────────

        /// <summary>Returns the first <paramref name="count"/> items from the array.</summary>
        public static MdixResult<List<T>> QueryTake<T>(
            this MdixDatabase db, string path, int count)
        {
            if (count < 0) throw new ArgumentOutOfRangeException(nameof(count));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<List<T>>.Err(r.Error);

            return MdixResult<List<T>>.Ok(r.SuccessResult.Take(count).ToList());
        }

        /// <summary>Returns items after skipping the first <paramref name="count"/> items.</summary>
        public static MdixResult<List<T>> QuerySkip<T>(
            this MdixDatabase db, string path, int count)
        {
            if (count < 0) throw new ArgumentOutOfRangeException(nameof(count));

            var r = db.GetArray<T>(path);
            if (r.IsFailure) return MdixResult<List<T>>.Err(r.Error);

            return MdixResult<List<T>>.Ok(r.SuccessResult.Skip(count).ToList());
        }
    }
}
