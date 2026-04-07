using System;
using System.Dynamic;

namespace MidManStudio.Mdix.Core
{
    /// <summary>
    /// <see cref="DynamicObject"/> wrapper over a loaded <see cref="MdixDatabase"/>.
    /// Enables path navigation without string literals at call sites:
    /// <code>
    /// dynamic cfg = db.AsDynamic();
    /// int port    = cfg.server.port;
    /// string name = cfg.enemies[0].name;
    /// </code>
    /// Intermediate node accesses (objects, arrays) return a new scoped
    /// <see cref="MdixDynamic"/> — leaf accesses resolve the value from the
    /// database using the value type returned by <see cref="MdixDatabase.GetValueType"/>.
    /// <para/>
    /// <b>Thread safety:</b> The underlying <see cref="MdixDatabase"/> must remain
    /// undisposed while any <see cref="MdixDynamic"/> that references it is in use.
    /// <para/>
    /// <b>Error handling:</b> Failed reads return <c>null</c> rather than throwing,
    /// so callers can do null-checks without try/catch in hot paths.
    /// </summary>
    public sealed class MdixDynamic : DynamicObject
    {
        #region Fields

        private readonly MdixDatabase _db;
        private readonly string       _prefix; // dotted path to the current node, e.g. "server" or "enemies[0]"

        #endregion

        #region Construction

        /// <summary>
        /// Creates a dynamic view rooted at the top level of <paramref name="db"/>.
        /// Prefer <see cref="MdixDatabase.AsDynamic"/> over calling this directly.
        /// </summary>
        public MdixDynamic(MdixDatabase db) : this(db, string.Empty) { }

        internal MdixDynamic(MdixDatabase db, string prefix)
        {
            _db     = db     ?? throw new ArgumentNullException(nameof(db));
            _prefix = prefix ?? string.Empty;
        }

        #endregion

        #region DynamicObject Overrides

        /// <summary>
        /// Handles <c>obj.PropertyName</c> access.
        /// Returns a scoped <see cref="MdixDynamic"/> for intermediate nodes,
        /// or the resolved scalar value at leaf nodes.
        /// </summary>
        public override bool TryGetMember(GetMemberBinder binder, out object? result)
        {
            var path = BuildPath(binder.Name);
            result = Resolve(path);
            // Always return true — null result means "path not found" at the call site.
            return true;
        }

        /// <summary>
        /// Handles <c>obj[index]</c> access where <paramref name="indexes"/> is a single int.
        /// Appends <c>[N]</c> to the current path prefix.
        /// </summary>
        public override bool TryGetIndex(GetIndexBinder binder, object[] indexes, out object? result)
        {
            result = null;
            if (indexes == null || indexes.Length != 1) return false;

            string indexStr = indexes[0] switch
            {
                int    i => i.ToString(),
                string s => s,
                _        => indexes[0]?.ToString() ?? string.Empty,
            };

            var path = string.IsNullOrEmpty(_prefix)
                ? $"[{indexStr}]"
                : $"{_prefix}[{indexStr}]";

            result = Resolve(path);
            return true;
        }

        #endregion

        #region Path Resolution

        /// <summary>
        /// Builds a dotted path from the current prefix and a member name.
        /// </summary>
        private string BuildPath(string memberName) =>
            string.IsNullOrEmpty(_prefix)
                ? memberName
                : $"{_prefix}.{memberName}";

        /// <summary>
        /// Inspects the value type at <paramref name="path"/> and returns:
        /// <list type="bullet">
        ///   <item>A scalar C# value (<c>string</c>, <c>int</c>, <c>float</c>, <c>double</c>, <c>bool</c>) for leaf nodes.</item>
        ///   <item>A new <see cref="MdixDynamic"/> scoped to that path for object/array nodes.</item>
        ///   <item><c>null</c> if the path does not exist or the database is disposed.</item>
        /// </list>
        /// </summary>
        private object? Resolve(string path)
        {
            if (!_db.IsValid) return null;

            var valueType = _db.GetValueType(path);

            switch (valueType)
            {
                case MdixValueType.String:
                case MdixValueType.Date:
                case MdixValueType.Timestamp:
                case MdixValueType.HexColor:
                case MdixValueType.Blob:
                case MdixValueType.Regex:
                    return _db.GetString(path).UnwrapOr(null!);

                case MdixValueType.Int:
                case MdixValueType.Enum:
                    return _db.GetInt(path).UnwrapOr(0);

                case MdixValueType.Float:
                    return _db.GetFloat(path).UnwrapOr(0f);

                case MdixValueType.Double:
                    return _db.GetDouble(path).UnwrapOr(0d);

                case MdixValueType.Bool:
                    return _db.GetBool(path).UnwrapOr(false);

                case MdixValueType.Null:
                    return null;

                case MdixValueType.Object:
                case MdixValueType.Array:
                case MdixValueType.Tuple:
                    // Intermediate node — return a scoped dynamic for further navigation.
                    return new MdixDynamic(_db, path);

                case MdixValueType.Unknown:
                default:
                    // Path does not exist.
                    return null;
            }
        }

        #endregion

        #region Utility

        /// <summary>The full dotted path prefix this dynamic node is scoped to.</summary>
        public string CurrentPath => _prefix;

        /// <summary>Returns the raw JSON for the current node path — useful for debugging.</summary>
        public MdixResult<string> ToJson() =>
            string.IsNullOrEmpty(_prefix)
                ? MdixError.InvalidPath("Cannot get JSON for root dynamic node without a path.")
                : _db.GetJson(_prefix);

        public override string ToString() =>
            string.IsNullOrEmpty(_prefix)
                ? $"MdixDynamic(root)"
                : $"MdixDynamic(path: '{_prefix}')";

        #endregion
    }
}
