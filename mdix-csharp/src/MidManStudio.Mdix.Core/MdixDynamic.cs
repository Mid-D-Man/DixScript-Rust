using System;
using System.Dynamic;

namespace MidManStudio.Mdix.Core
{
    /// <summary>
    /// <see cref="DynamicObject"/> wrapper over a loaded <see cref="MdixDatabase"/>.
    /// Enables path navigation without string literals at call sites:
    /// <code>
    /// dynamic cfg  = db.AsDynamic();
    /// int    port  = cfg.server.port;
    /// long   count = cfg.stats.total_users;  // Long values returned as long
    /// string name  = cfg.enemies[0].name;
    /// </code>
    /// Intermediate node accesses (objects, arrays) return a new scoped
    /// <see cref="MdixDynamic"/>. Leaf accesses resolve the value using the
    /// type returned by <see cref="MdixDatabase.GetValueType"/>.
    /// <para/>
    /// <b>Error handling:</b> Failed reads return <c>null</c> rather than throwing.
    /// </summary>
    public sealed class MdixDynamic : DynamicObject
    {
        #region Fields

        private readonly MdixDatabase _db;
        private readonly string       _prefix;

        #endregion

        #region Construction

        public MdixDynamic(MdixDatabase db) : this(db, string.Empty) { }

        internal MdixDynamic(MdixDatabase db, string prefix)
        {
            _db     = db     ?? throw new ArgumentNullException(nameof(db));
            _prefix = prefix ?? string.Empty;
        }

        #endregion

        #region DynamicObject Overrides

        public override bool TryGetMember(GetMemberBinder binder, out object? result)
        {
            var path = BuildPath(binder.Name);
            result = Resolve(path);
            return true;
        }

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

        private string BuildPath(string memberName) =>
            string.IsNullOrEmpty(_prefix)
                ? memberName
                : $"{_prefix}.{memberName}";

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

                // Long values are returned as System.Int64 (long).
                // dynamic callers receive a boxed long and can assign to
                // long, decimal, or double variables without a cast.
                case MdixValueType.Long:
                    return _db.GetLong(path).UnwrapOr(0L);

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
                    return new MdixDynamic(_db, path);

                case MdixValueType.Unknown:
                default:
                    return null;
            }
        }

        #endregion

        #region Utility

        public string CurrentPath => _prefix;

        public MdixResult<string> ToJson() =>
            string.IsNullOrEmpty(_prefix)
                ? MdixError.InvalidPath("Cannot get JSON for root dynamic node without a path.")
                : _db.GetJson(_prefix);

        public override string ToString() =>
            string.IsNullOrEmpty(_prefix)
                ? "MdixDynamic(root)"
                : $"MdixDynamic(path: '{_prefix}')";

        #endregion
    }
}
