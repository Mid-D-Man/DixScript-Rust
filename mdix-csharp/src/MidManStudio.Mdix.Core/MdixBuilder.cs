using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace MidManStudio.Mdix.Core
{
    // ══════════════════════════════════════════════════════════════════════════
    // Internal value-entry types used by DataBuilder and the serializer.
    // ══════════════════════════════════════════════════════════════════════════

    internal sealed record DixHexEntry(string Hex);
    internal sealed record DixDateEntry(string Formatted);
    internal sealed record DixTimestampEntry(string Formatted);
    internal sealed record DixEnumEntry(string EnumName, string FieldName);
    internal sealed record DixBlobEntry(string Base64);
    internal sealed record DixRegexEntry(string Pattern);
    internal sealed record DixTupleEntry(IReadOnlyList<object> Values);

    // ══════════════════════════════════════════════════════════════════════════
    // MdixBuilder — top-level fluent builder
    // ══════════════════════════════════════════════════════════════════════════

    public sealed class MdixBuilder : IDisposable
    {
        #region Fields

        internal readonly MdixConfigBuilder      _config = new MdixConfigBuilder();
        internal readonly MdixEnumsBuilder       _enums  = new MdixEnumsBuilder();
        internal readonly MdixDataSectionBuilder _data   = new MdixDataSectionBuilder();

        private volatile int _disposed;

        #endregion

        #region Construction

        private MdixBuilder() { }

        public static MdixBuilder Create() => new MdixBuilder();

        /// <summary>
        /// Creates a builder pre-populated with entries copied from a loaded database.
        /// Long values are preserved without truncation.
        /// </summary>
        public static MdixResult<MdixBuilder> FromDatabase(MdixDatabase db)
        {
            if (db is null) return MdixError.NativeError("FromDatabase: db cannot be null.");

            var builder    = Create();
            var keysResult = db.GetKeys();
            if (keysResult.IsFailure) return MdixResult<MdixBuilder>.Err(keysResult.Error);

            var err = CopyKeysIntoData(db, builder._data, keysResult.SuccessResult);
            if (err.HasValue) return MdixResult<MdixBuilder>.Err(err.Value);

            return MdixResult<MdixBuilder>.Ok(builder);
        }

        #endregion

        #region IDisposable

        public void Dispose() => Interlocked.Exchange(ref _disposed, 1);

        private void ThrowIfDisposed()
        {
            if (_disposed == 1)
                throw new ObjectDisposedException(nameof(MdixBuilder));
        }

        #endregion

        #region Section configuration

        public MdixBuilder Config(Action<MdixConfigBuilder> configure)
        {
            ThrowIfDisposed();
            configure(_config);
            return this;
        }

        public MdixBuilder Enums(Action<MdixEnumsBuilder> configure)
        {
            ThrowIfDisposed();
            configure(_enums);
            return this;
        }

        public MdixBuilder Data(Action<MdixDataSectionBuilder> configure)
        {
            ThrowIfDisposed();
            configure(_data);
            return this;
        }

        #endregion

        #region POCO serialization

        public MdixResult<Unit> Serialize<T>(T obj, string? prefix = null)
        {
            ThrowIfDisposed();
            if (obj == null) return MdixError.NativeError("Cannot serialize a null object.");
            var serializer = new MdixSerializer();
            return serializer.Serialize(obj, _data, prefix);
        }

        public MdixResult<MdixDatabase> ToDatabase()
        {
            ThrowIfDisposed();
            var ser = Serialize();
            if (ser.IsFailure) return MdixResult<MdixDatabase>.Err(ser.Error);
            return MdixDatabase.LoadStr(ser.SuccessResult);
        }

        #endregion

        #region Serialization and persistence

        public MdixResult<string> Serialize()
        {
            ThrowIfDisposed();
            try
            {
                return MdixResult<string>.Ok(
                    MdixFileSerializer.Serialize(_config, _enums, _data));
            }
            catch (Exception ex)
            {
                return MdixError.NativeError($"Serialize failed: {ex.Message}");
            }
        }

        public MdixResult<Unit> Save(string path)
        {
            ThrowIfDisposed();

            if (string.IsNullOrEmpty(path))
                return MdixError.InvalidPath(path);

            if (!path.EndsWith(".mdix", StringComparison.OrdinalIgnoreCase))
                path += ".mdix";

            var ser = Serialize();
            if (ser.IsFailure) return ser.Error;

            try
            {
                var dir = Path.GetDirectoryName(path);
                if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
                File.WriteAllText(path, ser.SuccessResult,
                    new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
                return MdixResult<Unit>.Ok(Unit.Value);
            }
            catch (Exception ex)
            {
                return MdixError.IoError($"Failed to save to '{path}': {ex.Message}", ex);
            }
        }

        public Task<MdixResult<Unit>> SaveAsync(
            string path, CancellationToken ct = default) =>
            Task.Run(() => Save(path), ct);

        public MdixResult<Unit> SaveToDirectory(string directory, string fileName)
        {
            if (string.IsNullOrEmpty(directory)) return MdixError.InvalidPath(directory);
            if (string.IsNullOrEmpty(fileName))  return MdixError.InvalidPath(fileName);
            fileName = Path.GetFileNameWithoutExtension(fileName);
            return Save(Path.Combine(directory, fileName));
        }

        #endregion

        #region Private helpers

        private static MdixError? CopyKeysIntoData(
            MdixDatabase           db,
            MdixDataSectionBuilder data,
            string[]               keys)
        {
            foreach (var key in keys)
            {
                switch (db.GetValueType(key))
                {
                    case MdixValueType.String:
                    case MdixValueType.Date:
                    case MdixValueType.Timestamp:
                    case MdixValueType.HexColor:
                    case MdixValueType.Blob:
                    case MdixValueType.Regex:
                    {
                        var r = db.GetString(key);
                        if (r.IsFailure) return r.Error;
                        data.WithString(key, r.SuccessResult);
                        break;
                    }
                    case MdixValueType.Int:
                    case MdixValueType.Enum:
                    {
                        var r = db.GetInt(key);
                        if (r.IsFailure) return r.Error;
                        data.WithInt(key, r.SuccessResult);
                        break;
                    }
                    case MdixValueType.Long:
                    {
                        var r = db.GetLong(key);
                        if (r.IsFailure) return r.Error;
                        data.WithLong(key, r.SuccessResult);
                        break;
                    }
                    case MdixValueType.Float:
                    {
                        var r = db.GetFloat(key);
                        if (r.IsFailure) return r.Error;
                        data.WithFloat(key, r.SuccessResult);
                        break;
                    }
                    case MdixValueType.Double:
                    {
                        var r = db.GetDouble(key);
                        if (r.IsFailure) return r.Error;
                        data.WithDouble(key, r.SuccessResult);
                        break;
                    }
                    case MdixValueType.Bool:
                    {
                        var r = db.GetBool(key);
                        if (r.IsFailure) return r.Error;
                        data.WithBool(key, r.SuccessResult);
                        break;
                    }
                    case MdixValueType.Object:
                    case MdixValueType.Array:
                    {
                        var children = db.GetKeys(key);
                        if (children.IsFailure) return children.Error;
                        if (children.SuccessResult.Length > 0)
                        {
                            var childErr = CopyKeysIntoData(db, data, children.SuccessResult);
                            if (childErr.HasValue) return childErr;
                        }
                        break;
                    }
                    default:
                    {
                        var r = db.GetJson(key);
                        if (r.IsSuccess) data.WithString(key, r.SuccessResult);
                        break;
                    }
                }
            }

            return null;
        }

        #endregion
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixConfigBuilder — @CONFIG section
    // ══════════════════════════════════════════════════════════════════════════

    public sealed class MdixConfigBuilder
    {
        internal readonly Dictionary<string, object> _entries = new Dictionary<string, object>();

        public MdixConfigBuilder WithVersion(string version)        => Set("version",           version);
        public MdixConfigBuilder WithAuthor(string author)          => Set("author",             author);
        public MdixConfigBuilder WithEncoding(string encoding)      => Set("encoding",           encoding);
        public MdixConfigBuilder WithFeatures(string features)      => Set("features",           features);
        public MdixConfigBuilder WithDebugMode(string debugMode)    => Set("debug_mode",         debugMode);
        public MdixConfigBuilder WithErrorHandling(string eh)       => Set("error_handling",     eh);
        public MdixConfigBuilder WithCompatibilityMode(string mode) => Set("compatibility_mode", mode);

        public MdixConfigBuilder WithCreated(DateTime created) =>
            Set("created", created.ToString("yyyy-MM-ddTHH:mm:ssZ", CultureInfo.InvariantCulture));

        public MdixConfigBuilder WithCustom(string key, string value) => Set(key, value);

        private MdixConfigBuilder Set(string key, object value)
        {
            _entries[key] = value;
            return this;
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixEnumsBuilder — @ENUMS section
    // ══════════════════════════════════════════════════════════════════════════

    public sealed class MdixEnumsBuilder
    {
        internal readonly List<(string Name, List<(string Field, int? Value)> Fields)> _enums
            = new List<(string, List<(string, int?)>)>();

        public MdixEnumsBuilder WithEnum(string enumName)
        {
            throw new ArgumentException("Enum must have at least one field.", nameof(enumName));
        }

        public MdixEnumsBuilder WithEnum(string enumName, params string[] fieldNames)
        {
            if (string.IsNullOrEmpty(enumName))
                throw new ArgumentException("Enum name cannot be empty.", nameof(enumName));
            if (fieldNames == null || fieldNames.Length == 0)
                throw new ArgumentException("Enum must have at least one field.", nameof(fieldNames));

            var fields = fieldNames.Select(n => (n, (int?)null)).ToList();
            _enums.Add((enumName, fields));
            return this;
        }

        public MdixEnumsBuilder WithEnum(string enumName, params (string Field, int Value)[] fields)
        {
            if (string.IsNullOrEmpty(enumName))
                throw new ArgumentException("Enum name cannot be empty.", nameof(enumName));
            if (fields == null || fields.Length == 0)
                throw new ArgumentException("Enum must have at least one field.", nameof(fields));

            var fieldList = fields.Select(f => (f.Field, (int?)f.Value)).ToList();
            _enums.Add((enumName, fieldList));
            return this;
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixDataSectionBuilder — @DATA section with two-tier ordering
    // ══════════════════════════════════════════════════════════════════════════

    public sealed class MdixDataSectionBuilder
    {
        internal readonly List<KeyValuePair<string, object>> _flatProperties
            = new List<KeyValuePair<string, object>>();

        internal readonly List<MdixTableEntry> _tableProperties = new List<MdixTableEntry>();
        internal readonly List<MdixGroupEntry> _groupArrays     = new List<MdixGroupEntry>();

        private bool _hasSeenGroupedData;

        // ── Flat properties ───────────────────────────────────────────────────

        public MdixDataSectionBuilder WithInt(string name, int value)       => AddFlat(name, value);
        public MdixDataSectionBuilder WithLong(string name, long value)      => AddFlat(name, value);
        public MdixDataSectionBuilder WithFloat(string name, float value)   => AddFlat(name, value);
        public MdixDataSectionBuilder WithDouble(string name, double value) => AddFlat(name, value);
        public MdixDataSectionBuilder WithString(string name, string value) => AddFlat(name, value);
        public MdixDataSectionBuilder WithBool(string name, bool value)     => AddFlat(name, value);

        public MdixDataSectionBuilder WithDate(string name, DateTime value) =>
            AddFlat(name, new DixDateEntry(
                value.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture)));

        public MdixDataSectionBuilder WithTimestamp(string name, DateTime value) =>
            AddFlat(name, new DixTimestampEntry(
                value.ToUniversalTime()
                     .ToString("yyyy-MM-ddTHH:mm:ss.fffZ", CultureInfo.InvariantCulture)));

        public MdixDataSectionBuilder WithHexColor(string name, string hexValue)
        {
            if (string.IsNullOrEmpty(hexValue) || !hexValue.StartsWith("#"))
                throw new ArgumentException("Hex color must start with '#'.", nameof(hexValue));
            return AddFlat(name, new DixHexEntry(hexValue));
        }

        public MdixDataSectionBuilder WithEnum(string name, string enumName, string fieldName) =>
            AddFlat(name, new DixEnumEntry(enumName, fieldName));

        public MdixDataSectionBuilder WithBlob(string name, string base64Data)
        {
            try { Convert.FromBase64String(base64Data); }
            catch (FormatException ex)
            {
                throw new ArgumentException(
                    $"Invalid base64 blob data: {ex.Message}", nameof(base64Data), ex);
            }
            return AddFlat(name, new DixBlobEntry(base64Data));
        }

        public MdixDataSectionBuilder WithRegex(string name, string pattern)
        {
            try { _ = new System.Text.RegularExpressions.Regex(pattern); }
            catch (ArgumentException ex)
            {
                throw new ArgumentException(
                    $"Invalid regex pattern: {ex.Message}", nameof(pattern), ex);
            }
            return AddFlat(name, new DixRegexEntry(pattern));
        }

        public MdixDataSectionBuilder WithArray<T>(string name, IEnumerable<T> items) =>
            AddFlat(name, items.Cast<object>().ToList());

        public MdixDataSectionBuilder WithTuple(string name, params object[] values)
        {
            if (values.Length > 6)
                throw new ArgumentException("Tuples may have at most 6 elements.", nameof(values));
            return AddFlat(name, new DixTupleEntry(values.ToList()));
        }

        public MdixDataSectionBuilder WithObject(string name, Action<MdixObjectBuilder> configure)
        {
            var builder = new MdixObjectBuilder();
            configure(builder);
            return AddFlat(name, builder._properties);
        }

        // ── Grouped data ──────────────────────────────────────────────────────

        public MdixDataSectionBuilder WithTableProperties(
            string path, Action<MdixTablePropertiesBuilder> configure)
        {
            _hasSeenGroupedData = true;
            var builder = new MdixTablePropertiesBuilder();
            configure(builder);
            _tableProperties.Add(new MdixTableEntry(path, builder._properties));
            return this;
        }

        public MdixDataSectionBuilder WithGroupArray<T>(string path, IEnumerable<T> items)
        {
            _hasSeenGroupedData = true;
            _groupArrays.Add(new MdixGroupEntry(path, items.Cast<object>().ToList()));
            return this;
        }

        public MdixDataSectionBuilder WithGroupArray(
            string path, Action<MdixGroupArrayBuilder> configure)
        {
            _hasSeenGroupedData = true;
            var builder = new MdixGroupArrayBuilder();
            configure(builder);
            _groupArrays.Add(new MdixGroupEntry(path, builder._items));
            return this;
        }

        // ── Two-tier enforcement ──────────────────────────────────────────────

        private MdixDataSectionBuilder AddFlat(string name, object value)
        {
            if (_hasSeenGroupedData)
                throw new InvalidOperationException(
                    $"Cannot add flat property '{name}' after table properties or group arrays. " +
                    "Flat properties must come first (two-tier structure).");

            _flatProperties.Add(new KeyValuePair<string, object>(name, value));
            return this;
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixTablePropertiesBuilder — properties inside a table block
    // ══════════════════════════════════════════════════════════════════════════

    public sealed class MdixTablePropertiesBuilder
    {
        internal readonly List<KeyValuePair<string, object>> _properties
            = new List<KeyValuePair<string, object>>();

        public MdixTablePropertiesBuilder WithInt(string name, int value)       => Add(name, value);
        public MdixTablePropertiesBuilder WithLong(string name, long value)      => Add(name, value);
        public MdixTablePropertiesBuilder WithFloat(string name, float value)   => Add(name, value);
        public MdixTablePropertiesBuilder WithDouble(string name, double value) => Add(name, value);
        public MdixTablePropertiesBuilder WithString(string name, string value) => Add(name, value);
        public MdixTablePropertiesBuilder WithBool(string name, bool value)     => Add(name, value);

        public MdixTablePropertiesBuilder WithHexColor(string name, string hex)
        {
            if (!hex.StartsWith("#"))
                throw new ArgumentException("Hex color must start with '#'.");
            return Add(name, new DixHexEntry(hex));
        }

        // FIX: WithBlob/WithRegex existed on MdixDataSectionBuilder but were missing
        // here, so a blob/regex property nested inside a table block had no way to
        // serialize correctly -- validation mirrors MdixDataSectionBuilder exactly.
        public MdixTablePropertiesBuilder WithBlob(string name, string base64Data)
        {
            try { Convert.FromBase64String(base64Data); }
            catch (FormatException ex)
            {
                throw new ArgumentException(
                    $"Invalid base64 blob data: {ex.Message}", nameof(base64Data), ex);
            }
            return Add(name, new DixBlobEntry(base64Data));
        }

        public MdixTablePropertiesBuilder WithRegex(string name, string pattern)
        {
            try { _ = new System.Text.RegularExpressions.Regex(pattern); }
            catch (ArgumentException ex)
            {
                throw new ArgumentException(
                    $"Invalid regex pattern: {ex.Message}", nameof(pattern), ex);
            }
            return Add(name, new DixRegexEntry(pattern));
        }

        public MdixTablePropertiesBuilder WithEnum(string name, string enumName, string fieldName) =>
            Add(name, new DixEnumEntry(enumName, fieldName));

        public MdixTablePropertiesBuilder WithDate(string name, DateTime value) =>
            Add(name, new DixDateEntry(
                value.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture)));

        public MdixTablePropertiesBuilder WithTimestamp(string name, DateTime value) =>
            Add(name, new DixTimestampEntry(
                value.ToUniversalTime()
                     .ToString("yyyy-MM-ddTHH:mm:ss.fffZ", CultureInfo.InvariantCulture)));

        private MdixTablePropertiesBuilder Add(string name, object value)
        {
            _properties.Add(new KeyValuePair<string, object>(name, value));
            return this;
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixGroupArrayBuilder — items inside a group array block
    // ══════════════════════════════════════════════════════════════════════════

    public sealed class MdixGroupArrayBuilder
    {
        internal readonly List<object> _items = new List<object>();

        public MdixGroupArrayBuilder AddString(string value) { _items.Add(value); return this; }
        public MdixGroupArrayBuilder AddInt(int value)       { _items.Add(value); return this; }
        public MdixGroupArrayBuilder AddLong(long value)      { _items.Add(value); return this; }
        public MdixGroupArrayBuilder AddFloat(float value)   { _items.Add(value); return this; }
        public MdixGroupArrayBuilder AddDouble(double value) { _items.Add(value); return this; }
        public MdixGroupArrayBuilder AddBool(bool value)     { _items.Add(value); return this; }

        public MdixGroupArrayBuilder AddEnum(string enumName, string fieldName)
        {
            _items.Add(new DixEnumEntry(enumName, fieldName));
            return this;
        }

        public MdixGroupArrayBuilder AddObject(Action<MdixObjectBuilder> configure)
        {
            var builder = new MdixObjectBuilder();
            configure(builder);
            _items.Add(builder._properties);
            return this;
        }

        public MdixGroupArrayBuilder AddValue(object value) { _items.Add(value); return this; }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixObjectBuilder — for nested object literals
    // ══════════════════════════════════════════════════════════════════════════

    public sealed class MdixObjectBuilder
    {
        internal readonly Dictionary<string, object> _properties =
            new Dictionary<string, object>();

        public MdixObjectBuilder WithInt(string name, int value)       { _properties[name] = value; return this; }
        public MdixObjectBuilder WithLong(string name, long value)      { _properties[name] = value; return this; }
        public MdixObjectBuilder WithFloat(string name, float value)   { _properties[name] = value; return this; }
        public MdixObjectBuilder WithDouble(string name, double value) { _properties[name] = value; return this; }
        public MdixObjectBuilder WithString(string name, string value) { _properties[name] = value; return this; }
        public MdixObjectBuilder WithBool(string name, bool value)     { _properties[name] = value; return this; }

        public MdixObjectBuilder WithEnum(string name, string enumName, string fieldName)
        {
            _properties[name] = new DixEnumEntry(enumName, fieldName);
            return this;
        }

        public MdixObjectBuilder WithDate(string name, DateTime value)
        {
            _properties[name] = new DixDateEntry(
                value.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture));
            return this;
        }

        public MdixObjectBuilder WithArray<T>(string name, IEnumerable<T> items)
        {
            _properties[name] = items.Cast<object>().ToList();
            return this;
        }

        public MdixObjectBuilder WithObject(string name, Action<MdixObjectBuilder> configure)
        {
            var b = new MdixObjectBuilder();
            configure(b);
            _properties[name] = b._properties;
            return this;
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Internal data containers
    // ══════════════════════════════════════════════════════════════════════════

    internal sealed class MdixTableEntry
    {
        internal string Path { get; }
        internal IReadOnlyList<KeyValuePair<string, object>> Properties { get; }

        internal MdixTableEntry(string path, IReadOnlyList<KeyValuePair<string, object>> props)
        {
            Path       = path;
            Properties = props;
        }
    }

    internal sealed class MdixGroupEntry
    {
        internal string Path { get; }
        internal IReadOnlyList<object> Items { get; }

        internal MdixGroupEntry(string path, IReadOnlyList<object> items)
        {
            Path  = path;
            Items = items;
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // MdixFileSerializer — converts section builders to a .mdix string
    // ══════════════════════════════════════════════════════════════════════════

    internal static class MdixFileSerializer
    {
        internal static string Serialize(
            MdixConfigBuilder      config,
            MdixEnumsBuilder       enums,
            MdixDataSectionBuilder data)
        {
            var sb = new StringBuilder();
            AppendConfig(sb, config);
            AppendEnums(sb, enums);
            AppendData(sb, data);
            return sb.ToString().TrimEnd();
        }

        // ── @CONFIG ───────────────────────────────────────────────────────────

        private static void AppendConfig(StringBuilder sb, MdixConfigBuilder config)
        {
            if (config._entries.Count == 0) return;
            sb.AppendLine("@CONFIG(");
            foreach (var kvp in config._entries)
                sb.AppendLine($"  {kvp.Key} -> {FormatConfigValue(kvp.Value)}");
            sb.AppendLine(")");
            sb.AppendLine();
        }

        private static string FormatConfigValue(object value) => value switch
        {
            string s => $"\"{Escape(s)}\"",
            int    i => i.ToString(),
            long   l => l.ToString(CultureInfo.InvariantCulture) + "L",
            bool   b => b ? "true" : "false",
            _        => $"\"{Escape(value?.ToString() ?? "")}\"",
        };

        // ── @ENUMS ────────────────────────────────────────────────────────────

        private static void AppendEnums(StringBuilder sb, MdixEnumsBuilder enums)
        {
            if (enums._enums.Count == 0) return;
            sb.AppendLine("@ENUMS(");
            foreach (var (name, fields) in enums._enums)
            {
                var parts = fields.Select(f =>
                    f.Value.HasValue ? $"{f.Field} = {f.Value}" : f.Field);
                sb.AppendLine($"  {name} {{ {string.Join(", ", parts)} }}");
            }
            sb.AppendLine(")");
            sb.AppendLine();
        }

        // ── @DATA ─────────────────────────────────────────────────────────────

        private static void AppendData(StringBuilder sb, MdixDataSectionBuilder data)
        {
            bool hasFlat   = data._flatProperties.Count > 0;
            bool hasGroups = data._tableProperties.Count > 0 || data._groupArrays.Count > 0;
            if (!hasFlat && !hasGroups) return;

            sb.AppendLine("@DATA(");

            foreach (var kv in data._flatProperties)
                sb.AppendLine($"  {kv.Key} = {FormatValue(kv.Value)}");

            if (hasFlat && hasGroups) sb.AppendLine();

            foreach (var table in data._tableProperties)
            {
                var props = string.Join(", ",
                    table.Properties.Select(p => $"{p.Key} = {FormatValue(p.Value)}"));
                sb.AppendLine($"  {table.Path}: {props}");
            }

            foreach (var arr in data._groupArrays)
            {
                if (arr.Items.Count == 0)
                {
                    sb.AppendLine($"  {arr.Path}:: ");
                    continue;
                }

                bool isComplex = arr.Items.Any(i => i is Dictionary<string, object>);
                if (isComplex)
                {
                    sb.AppendLine($"  {arr.Path}::");
                    for (int i = 0; i < arr.Items.Count; i++)
                    {
                        var comma = i < arr.Items.Count - 1 ? "," : "";
                        sb.AppendLine($"    {FormatValue(arr.Items[i])}{comma}");
                    }
                }
                else
                {
                    var items = string.Join(", ", arr.Items.Select(FormatValue));
                    sb.AppendLine($"  {arr.Path}:: {items}");
                }
            }

            sb.AppendLine(")");
        }

        // ── Value formatter ───────────────────────────────────────────────────

        internal static string FormatValue(object value) => value switch
        {
            null                           => "null",
            bool b                         => b ? "true" : "false",
            int i                          => i.ToString(),
            long l                         => l.ToString(CultureInfo.InvariantCulture) + "L",
            float f                        => f.ToString("G", CultureInfo.InvariantCulture) + "f",
            double d                       => d.ToString("G", CultureInfo.InvariantCulture),
            string s                       => $"\"{Escape(s)}\"",
            DixHexEntry h                  => h.Hex,
            DixDateEntry dt                => dt.Formatted,
            DixTimestampEntry ts           => ts.Formatted,
            DixEnumEntry e                 => $"{e.EnumName}.{e.FieldName}",
            DixBlobEntry b                 => $"b:(\"{b.Base64}\")",
            DixRegexEntry r                => $"r:(\"{Escape(r.Pattern)}\")",
            DixTupleEntry t                => $"t:({string.Join(", ", t.Values.Select(FormatValue))})",
            List<object> arr               => $"[{string.Join(", ", arr.Select(FormatValue))}]",
            Dictionary<string, object> obj => FormatObject(obj),
            _                              => $"\"{Escape(value.ToString() ?? "")}\"",
        };

        private static string FormatObject(Dictionary<string, object> obj)
        {
            var props = string.Join(", ",
                obj.Select(kv => $"{kv.Key} = {FormatValue(kv.Value)}"));
            return $"{{ {props} }}";
        }

        private static string Escape(string s) =>
            s.Replace("\\", "\\\\")
             .Replace("\"", "\\\"")
             .Replace("\n", "\\n")
             .Replace("\r", "\\r")
             .Replace("\t", "\\t");
    }
}
