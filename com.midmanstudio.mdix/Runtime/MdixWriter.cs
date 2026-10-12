using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// What a save did not store, and why. Nothing here is an error: the rest of the object was saved.
    /// </summary>
    public sealed class MdixWriteReport
    {
        /// <summary>Members or values left out, each as "Type.member: reason".</summary>
        public readonly List<string> NotStored = new List<string>();

        /// <summary>Small changes made to stay valid, for example characters DixScript text cannot hold.</summary>
        public readonly List<string> Notes = new List<string>();

        /// <summary>How many values were written.</summary>
        public int WrittenValues;

        /// <summary>True when everything was stored as it was.</summary>
        public bool IsClean => NotStored.Count == 0 && Notes.Count == 0;

        /// <summary>The report in a few lines; each group lists at most <paramref name="maxPerGroup"/> entries.</summary>
        public string Describe(int maxPerGroup = 5)
        {
            var lines = new List<string>();
            AppendGroup(lines, "Not stored", NotStored, maxPerGroup);
            AppendGroup(lines, "Changed", Notes, maxPerGroup);
            return string.Join("\n", lines);
        }

        private static void AppendGroup(List<string> lines, string heading, List<string> items, int maxPerGroup)
        {
            if (items.Count == 0) return;
            var limit = maxPerGroup > 0 ? maxPerGroup : int.MaxValue;

            var sb = new StringBuilder();
            sb.Append(heading).Append(": ");
            for (int i = 0; i < items.Count && i < limit; i++)
            {
                if (i > 0) sb.Append("; ");
                sb.Append(items[i]);
            }
            if (items.Count > limit) sb.Append("; and ").Append(items.Count - limit).Append(" more");
            lines.Add(sb.ToString());
        }
    }

    /// <summary>
    /// Turns any class or struct into DixScript text, the counterpart of <see cref="MdixBinder"/>.
    ///
    /// Stored: public fields, [SerializeField] fields and properties with a public getter and any setter
    /// ({ get; private set; } included); strings, bools, chars, every integer type, floats, decimals, enums
    /// (by name), Guid, DateTime (as round-trip text, so its Kind is kept), DateTimeOffset, TimeSpan; arrays, lists, sets and dictionaries of anything
    /// storable; nested classes, structs and records; Unity's Vector2/3/4, Vector2Int/3Int, Quaternion,
    /// Matrix4x4, Rect, RectInt, Bounds, BoundsInt, Color and Color32.
    ///
    /// Not stored (reported in <see cref="MdixWriteReport.NotStored"/>): references to Unity objects,
    /// delegates, members typed object or as an interface or abstract class, multi-dimensional arrays.
    /// Mark a field [NonSerialized] or a property [MdixIgnore] to leave it out without a report.
    /// A member is stored as its declared type, so a derived instance held in a base-typed member keeps
    /// only the base members.
    /// </summary>
    public static class MdixWriter
    {
        private const int MaxDepth    = 32;
        private const int InlineLimit = 100;
        private const int VerifyLimit = 2000;

        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        /// <summary>Engine structs that have no public fields (or whose fields are not the whole story), by the members to store.</summary>
        private static readonly Dictionary<string, string[]> EngineMembers = new Dictionary<string, string[]>
        {
            { "UnityEngine.Vector2Int", new[] { "x", "y" } },
            { "UnityEngine.Vector3Int", new[] { "x", "y", "z" } },
            { "UnityEngine.Rect",       new[] { "x", "y", "width", "height" } },
            { "UnityEngine.RectInt",    new[] { "x", "y", "width", "height" } },
            { "UnityEngine.Bounds",     new[] { "center", "size" } },
            { "UnityEngine.BoundsInt",  new[] { "position", "size" } },
        };

        // ── Public entry points ───────────────────────────────────────────────

        /// <summary>The DixScript text for <paramref name="value"/>. <paramref name="prefix"/> nests it under that key (dots nest further).</summary>
        public static MdixResult<string> ToText<T>(T value, string prefix = null)
        {
            MdixWriteReport report;
            return ToText(value, typeof(T), prefix, out report);
        }

        /// <inheritdoc cref="ToText{T}(T, string)"/>
        public static MdixResult<string> ToText<T>(T value, string prefix, out MdixWriteReport report) =>
            ToText(value, typeof(T), prefix, out report);

        /// <inheritdoc cref="ToText{T}(T, string)"/>
        public static MdixResult<string> ToText(object value, Type declared, string prefix, out MdixWriteReport report)
        {
            report = new MdixWriteReport();
            if (value == null)
                return MdixResult<string>.Err(MdixError.SchemaError("Nothing to save: the value is null."));

            var type = declared;
            if (type == null || type.IsAbstract || type.IsInterface || type == typeof(object)) type = value.GetType();
            type = Nullable.GetUnderlyingType(type) ?? type;

            if (!MdixBinder.IsRootObject(type))
                return MdixResult<string>.Err(MdixError.SchemaError(
                    type.Name + " cannot be the root of a save: use a class or struct (put a list or dictionary inside one)."));

            var effective = MdixBinder.EffectivePrefix(type, prefix);
            string[] parts = null;
            if (!string.IsNullOrEmpty(effective))
            {
                parts = effective.Split('.');
                foreach (var part in parts)
                    if (!MdixBinder.IsPlainKey(part))
                        return MdixResult<string>.Err(MdixError.SchemaError(
                            "The prefix \"" + effective + "\" is not a valid name: use letters, digits and underscores, separated by dots."));
            }

            var ctx = new Ctx(report);
            Node node;
            try { node = ObjectNode(ctx, value, type, type.Name, 0); }
            catch (Exception ex)
            {
                return MdixResult<string>.Err(MdixError.NativeError(
                    "MdixWriter: " + type.Name + " could not be written (" + ex.Message + ")"));
            }
            if (node == null)
                return MdixResult<string>.Err(MdixError.SchemaError("Nothing in " + type.Name + " can be stored.\n" + report.Describe(5)));

            if (parts != null)
            {
                for (int i = parts.Length - 1; i >= 0; i--)
                {
                    var wrap = new Obj();
                    wrap.Members.Add(new KeyValuePair<string, Node>(parts[i], node));
                    node = wrap;
                }
            }

            var root = (Obj)node;
            var sb = new StringBuilder(1024);
            sb.Append("@DATA(\n");
            foreach (var member in root.Members)
            {
                sb.Append("  ").Append(member.Key).Append(" = ");
                Emit(sb, member.Value, 2);
                sb.Append('\n');
            }
            sb.Append(")\n");
            var text = sb.ToString();

            // Prove the engine reads back what was written before anything is saved over a good file.
            var check = Verify(text, root);
            if (check != null)
                return MdixResult<string>.Err(MdixError.SchemaError(check));

            return MdixResult<string>.Ok(text);
        }

        private static string Verify(string text, Obj root)
        {
            var loaded = Dix.LoadStr(text);
            if (loaded.IsFailure)
                return "The save data is not valid DixScript, so nothing was written: " + loaded.Error.Message;

            using (var db = loaded.SuccessResult)
            {
                var budget = VerifyLimit;
                var missing = FindMissing(db, root, string.Empty, ref budget);
                if (missing != null)
                    return "DixScript did not accept the key \"" + missing + "\" (a reserved word?), so nothing was written.";
            }
            return null;
        }

        private static string FindMissing(MdixDatabase db, Node node, string path, ref int budget)
        {
            var obj = node as Obj;
            if (obj != null)
            {
                foreach (var member in obj.Members)
                {
                    if (budget-- <= 0) return null;
                    var p = path.Length == 0 ? member.Key : path + "." + member.Key;
                    if (db.GetValueType(p) == MdixValueType.Unknown) return p;
                    var inner = FindMissing(db, member.Value, p, ref budget);
                    if (inner != null) return inner;
                }
                return null;
            }

            var arr = node as Arr;
            if (arr != null)
            {
                for (int i = 0; i < arr.Items.Count && i < 2; i++)
                {
                    var inner = FindMissing(db, arr.Items[i], path + "[" + i + "]", ref budget);
                    if (inner != null) return inner;
                }
            }
            return null;
        }

        // ── Tree ──────────────────────────────────────────────────────────────

        private abstract class Node { }

        private sealed class Scalar : Node
        {
            public readonly string Text;
            public Scalar(string text) { Text = text; }
        }

        private sealed class Arr : Node
        {
            public readonly List<Node> Items = new List<Node>();
        }

        private sealed class Obj : Node
        {
            public readonly List<KeyValuePair<string, Node>> Members = new List<KeyValuePair<string, Node>>();
        }

        private sealed class RefComparer : IEqualityComparer<object>
        {
            public static readonly RefComparer Instance = new RefComparer();
            bool IEqualityComparer<object>.Equals(object x, object y) => ReferenceEquals(x, y);
            int IEqualityComparer<object>.GetHashCode(object obj) => RuntimeHelpers.GetHashCode(obj);
        }

        private sealed class Ctx
        {
            public readonly MdixWriteReport Report;
            public readonly HashSet<object> Active = new HashSet<object>(RefComparer.Instance);
            private int _replaced;

            public Ctx(MdixWriteReport report) { Report = report; }

            public Node Raw(string text)  { Report.WrittenValues++; return new Scalar(text); }
            public Node Text(string text) { Report.WrittenValues++; return new Scalar(Quote(text)); }

            public void NotStored(string path, string reason)
            {
                var entry = path + ": " + reason;
                if (!Report.NotStored.Contains(entry)) Report.NotStored.Add(entry);
            }

            private string Quote(string s)
            {
                var sb = new StringBuilder(s.Length + 2);
                sb.Append('"');
                foreach (var c in s)
                {
                    switch (c)
                    {
                        case '\\': sb.Append("\\\\"); break;
                        case '"':  sb.Append("\\\""); break;
                        case '\n': sb.Append("\\n");  break;
                        case '\r': sb.Append("\\r");  break;
                        case '\t': sb.Append("\\t");  break;
                        case '\0': case '\u000B': case '\u000C': case '\u0085': case '\u2028': case '\u2029':
                            sb.Append(' '); _replaced++; break;
                        default: sb.Append(c); break;
                    }
                }
                sb.Append('"');

                if (_replaced > 0)
                {
                    var note = "text: null and line-separator characters cannot be stored and were saved as spaces";
                    if (!Report.Notes.Contains(note)) Report.Notes.Add(note);
                }
                return sb.ToString();
            }
        }

        // ── Values ────────────────────────────────────────────────────────────

        private static Node ToNode(Ctx ctx, object value, Type declared, string path, int depth, bool textNumbers)
        {
            if (depth > MaxDepth)
            {
                ctx.NotStored(path, "nested deeper than " + MaxDepth + " levels");
                return null;
            }

            var type = Nullable.GetUnderlyingType(declared) ?? declared;

            var why = NotStorable(type);
            if (why != null)
            {
                ctx.NotStored(path, why);
                return null;
            }

            if (value == null) return ctx.Raw("null");

            Node scalar;
            if (TryScalar(ctx, value, type, textNumbers, out scalar)) return scalar;

            Type keyType, valueType;
            if (MdixBinder.TryGetDictionary(type, out keyType, out valueType))
                return DictionaryNode(ctx, value, keyType, valueType, path, depth);

            Type element; bool isArray, isSet;
            if (MdixBinder.TryGetCollection(type, out element, out isArray, out isSet))
                return ArrayNode(ctx, value, element, path, depth);

            return ObjectNode(ctx, value, type, path, depth);
        }

        /// <summary>Null when the type can be stored, otherwise the reason it cannot.</summary>
        private static string NotStorable(Type type)
        {
            if (type == typeof(string) || type.IsPrimitive || type.IsEnum || type == typeof(decimal) || IsKnownScalar(type))
                return null;

            Type k, v, e; bool arr, set;
            if (MdixBinder.TryGetDictionary(type, out k, out v)) return NotStorableInner(k) ?? NotStorableInner(v);
            if (MdixBinder.TryGetCollection(type, out e, out arr, out set)) return NotStorableInner(e);
            if (type.IsArray) return "multi-dimensional arrays cannot be stored (use an array of arrays or a list)";

            if (MdixBinder.DerivesFromUnityObject(type))
                return "a reference to a Unity object cannot be stored (store its name or an id instead; [NonSerialized] hides this message)";
            if (typeof(Delegate).IsAssignableFrom(type)) return "delegates and events cannot be stored";
            if (type == typeof(object)) return "its type is object, so it could not be rebuilt on load";
            if (type == typeof(Type) || type == typeof(IntPtr) || type == typeof(UIntPtr) || type.IsPointer ||
                typeof(MemberInfo).IsAssignableFrom(type))
                return "this kind of value cannot be stored";
            if (type.IsInterface || type.IsAbstract)
                return "an interface or abstract type cannot be created on load (use a concrete type)";
            if (type.IsGenericTypeDefinition) return "an open generic type cannot be stored";
            if (MdixBinder.IsUnityType(type) && !type.IsValueType)
                return "the engine class " + type.Name + " cannot be stored";
            return null;
        }

        private static string NotStorableInner(Type type) => NotStorable(Nullable.GetUnderlyingType(type) ?? type);

        private static bool IsKnownScalar(Type t) =>
            t == typeof(Guid) || t == typeof(DateTime) || t == typeof(DateTimeOffset) || t == typeof(TimeSpan) ||
            t == typeof(MdixHexColor) || t == typeof(MdixDate) || t == typeof(MdixTimestamp) ||
            t == typeof(UnityEngine.Color) || t == typeof(UnityEngine.Color32) || t == typeof(byte[]);

        private static bool TryScalar(Ctx ctx, object value, Type type, bool textNumbers, out Node node)
        {
            node = null;

            if (type.IsEnum) { node = ctx.Text(value.ToString()); return true; }

            if (type == typeof(Guid))           { node = ctx.Text(((Guid)value).ToString("D")); return true; }
            if (type == typeof(TimeSpan))       { node = ctx.Raw(((TimeSpan)value).Ticks.ToString(Inv) + "L"); return true; }
            if (type == typeof(DateTime))       { node = ctx.Text(((DateTime)value).ToString("o", Inv)); return true; }
            if (type == typeof(DateTimeOffset)) { node = ctx.Raw(StampText((DateTimeOffset)value)); return true; }
            if (type == typeof(byte[]))         { node = ctx.Text(Convert.ToBase64String((byte[])value)); return true; }
            if (type == typeof(MdixHexColor))
            {
                var c = (MdixHexColor)value;
                node = ctx.Raw(HexText(c.R, c.G, c.B, c.A));
                return true;
            }
            if (type == typeof(MdixDate))      { node = ctx.Raw(DateLiteral(((MdixDate)value).Value)); return true; }
            if (type == typeof(MdixTimestamp)) { node = ctx.Raw(StampLiteral(((MdixTimestamp)value).Value)); return true; }
            if (type == typeof(UnityEngine.Color32))
            {
                var c = (UnityEngine.Color32)value;
                node = ctx.Raw("#" + c.r.ToString("X2") + c.g.ToString("X2") + c.b.ToString("X2") + c.a.ToString("X2"));
                return true;
            }
            if (type == typeof(UnityEngine.Color))
            {
                var c = (UnityEngine.Color)value;
                var o = new Obj();
                o.Members.Add(new KeyValuePair<string, Node>("r", FloatNode(ctx, c.r, textNumbers)));
                o.Members.Add(new KeyValuePair<string, Node>("g", FloatNode(ctx, c.g, textNumbers)));
                o.Members.Add(new KeyValuePair<string, Node>("b", FloatNode(ctx, c.b, textNumbers)));
                o.Members.Add(new KeyValuePair<string, Node>("a", FloatNode(ctx, c.a, textNumbers)));
                node = o;
                return true;
            }

            switch (Type.GetTypeCode(type))
            {
                case TypeCode.Boolean: node = ctx.Raw((bool)value ? "true" : "false"); return true;
                case TypeCode.Char:    node = ctx.Text(((char)value) == '\0' ? string.Empty : ((char)value).ToString()); return true;
                case TypeCode.String:  node = ctx.Text((string)value);                  return true;

                case TypeCode.SByte: case TypeCode.Byte: case TypeCode.Int16: case TypeCode.UInt16: case TypeCode.Int32:
                    node = ctx.Raw(Convert.ToInt64(value, Inv).ToString(Inv));
                    return true;

                case TypeCode.UInt32: case TypeCode.Int64:
                    node = ctx.Raw(Convert.ToInt64(value, Inv).ToString(Inv) + "L");
                    return true;

                case TypeCode.UInt64:
                {
                    var u = (ulong)value;
                    node = (u <= long.MaxValue && !textNumbers) ? ctx.Raw(u.ToString(Inv) + "L") : ctx.Text(u.ToString(Inv));
                    return true;
                }

                case TypeCode.Single:  node = FloatNode(ctx, (float)value, textNumbers); return true;
                case TypeCode.Double:  node = DoubleNode(ctx, (double)value, textNumbers); return true;
                case TypeCode.Decimal: node = ctx.Text(((decimal)value).ToString(Inv)); return true;
            }

            return false;
        }

        // ── Number and date text ──────────────────────────────────────────────

        private static Node FloatNode(Ctx ctx, float f, bool asText)
        {
            if (asText || float.IsNaN(f) || float.IsInfinity(f)) return ctx.Text(f.ToString("R", Inv));

            var s = f.ToString("R", Inv);
            float back;
            if (!float.TryParse(s, NumberStyles.Float, Inv, out back) || back != f) s = f.ToString("G9", Inv);
            return ctx.Raw(AddPoint(s) + "f");
        }

        private static Node DoubleNode(Ctx ctx, double d, bool asText)
        {
            if (asText || double.IsNaN(d) || double.IsInfinity(d)) return ctx.Text(d.ToString("R", Inv));

            var s = d.ToString("R", Inv);
            double back;
            if (!double.TryParse(s, NumberStyles.Float, Inv, out back) || back != d) s = d.ToString("G17", Inv);
            return ctx.Raw(AddPoint(s));
        }

        private static string AddPoint(string s) =>
            s.IndexOf('.') < 0 && s.IndexOf('E') < 0 && s.IndexOf('e') < 0 ? s + ".0" : s;

        private static string HexText(float r, float g, float b, float a) =>
            "#" + ToByte(r).ToString("X2") + ToByte(g).ToString("X2") + ToByte(b).ToString("X2") + ToByte(a).ToString("X2");

        private static byte ToByte(float channel) =>
            (byte)Math.Max(0, Math.Min(255, (int)Math.Round(channel * 255f)));

        private static string StampText(DateTimeOffset value) =>
            value.ToString("yyyy-MM-dd'T'HH:mm:ss.fffffffzzz", Inv);

        private static string DateLiteral(object raw)
        {
            if (raw is DateTime)       return ((DateTime)raw).ToString("yyyy-MM-dd", Inv);
            if (raw is DateTimeOffset) return ((DateTimeOffset)raw).ToString("yyyy-MM-dd", Inv);
            return raw.ToString();
        }

        private static string StampLiteral(object raw)
        {
            if (raw is DateTimeOffset) return StampText((DateTimeOffset)raw);
            if (raw is DateTime)
            {
                var dt = (DateTime)raw;
                return dt.Kind == DateTimeKind.Utc
                    ? dt.ToString("yyyy-MM-dd'T'HH:mm:ss.fffffff'Z'", Inv)
                    : StampText(new DateTimeOffset(dt));
            }
            return raw.ToString();
        }

        // ── Containers ────────────────────────────────────────────────────────

        private static Node ArrayNode(Ctx ctx, object value, Type element, string path, int depth)
        {
            var source = value as IEnumerable;
            if (source == null)
            {
                ctx.NotStored(path, "the value is not a collection");
                return null;
            }

            var items = new List<object>();
            foreach (var item in source) items.Add(item);

            // One non-finite float (or a ulong too big for a long) writes the whole list as text, so its
            // elements stay one kind; the loader reads numbers from text.
            var plain = Nullable.GetUnderlyingType(element) ?? element;
            var textNumbers = false;
            if (plain == typeof(float) || plain == typeof(double))
            {
                foreach (var item in items)
                {
                    if (item == null) continue;
                    var d = Convert.ToDouble(item, Inv);
                    if (double.IsNaN(d) || double.IsInfinity(d)) { textNumbers = true; break; }
                }
            }
            else if (plain == typeof(ulong))
            {
                foreach (var item in items)
                    if (item != null && (ulong)item > long.MaxValue) { textNumbers = true; break; }
            }

            var arr = new Arr();
            for (int i = 0; i < items.Count; i++)
            {
                var child = ToNode(ctx, items[i], element, path + "[" + i + "]", depth + 1, textNumbers);
                arr.Items.Add(child ?? ctx.Raw("null"));
            }
            return arr;
        }

        private static Node DictionaryNode(Ctx ctx, object value, Type keyType, Type valueType, string path, int depth)
        {
            var dict = value as IDictionary;
            if (dict == null)
            {
                ctx.NotStored(path, "the dictionary could not be enumerated");
                return null;
            }

            var arr = new Arr();
            var index = 0;
            foreach (DictionaryEntry entry in dict)
            {
                var entryPath = path + "[" + index++ + "]";
                var key   = ToNode(ctx, entry.Key,   keyType,   entryPath + ".key",   depth + 1, false);
                var val   = ToNode(ctx, entry.Value, valueType, entryPath + ".value", depth + 1, false);
                if (key == null || val == null) continue;

                var pair = new Obj();
                pair.Members.Add(new KeyValuePair<string, Node>("key", key));
                pair.Members.Add(new KeyValuePair<string, Node>("value", val));
                arr.Items.Add(pair);
            }
            return arr;
        }

        private static Node ObjectNode(Ctx ctx, object value, Type type, string path, int depth)
        {
            var isReference = !type.IsValueType;
            if (isReference && !ctx.Active.Add(value))
            {
                ctx.NotStored(path, "it refers back to an object that is already being saved (a cycle)");
                return ctx.Raw("null");
            }

            try
            {
                var obj  = new Obj();
                var keys = new HashSet<string>(StringComparer.Ordinal);

                var engine = MdixBinder.IsUnityType(type);
                string[] only = null;
                if (engine) EngineMembers.TryGetValue(type.FullName, out only);

                foreach (var m in MdixBinder.GetMembers(type))
                {
                    if (!m.CanGet) continue;

                    if (engine)
                    {
                        if (only != null) { if (Array.IndexOf(only, m.Name) < 0) continue; }
                        else if (m.Field == null) continue;       // Quaternion.eulerAngles and friends are computed
                    }

                    var memberPath = path + "." + m.Name;
                    if (!MdixBinder.IsPlainKey(m.Key))
                    {
                        ctx.NotStored(memberPath, "its name cannot be used as a key (" + m.Key + ")");
                        continue;
                    }
                    if (!keys.Add(m.Key))
                    {
                        ctx.NotStored(memberPath, "another member is already stored as \"" + m.Key + "\"");
                        continue;
                    }

                    object member;
                    try { member = m.Get(value); }
                    catch (Exception ex)
                    {
                        ctx.NotStored(memberPath, "reading it threw " + ex.GetType().Name + ": " + ex.Message);
                        continue;
                    }

                    var child = ToNode(ctx, member, m.Type, memberPath, depth + 1, false);
                    if (child != null) obj.Members.Add(new KeyValuePair<string, Node>(m.Key, child));
                }

                if (engine && only == null && obj.Members.Count == 0)
                {
                    ctx.NotStored(path, "the engine type " + type.Name + " has no public fields to store");
                    return null;
                }
                return obj;
            }
            finally
            {
                if (isReference) ctx.Active.Remove(value);
            }
        }

        // ── Text layout ───────────────────────────────────────────────────────

        /// <summary>One-line text for a node whose parts are all simple and short, otherwise null.</summary>
        private static string TryFlat(Node node)
        {
            var scalar = node as Scalar;
            if (scalar != null) return scalar.Text;

            var arr = node as Arr;
            if (arr != null)
            {
                var sb = new StringBuilder("[");
                for (int i = 0; i < arr.Items.Count; i++)
                {
                    var part = TryFlat(arr.Items[i]);
                    if (part == null) return null;
                    if (i > 0) sb.Append(", ");
                    sb.Append(part);
                    if (sb.Length > InlineLimit) return null;
                }
                return sb.Append(']').ToString();
            }

            var obj = (Obj)node;
            if (obj.Members.Count == 0) return "{}";

            var text = new StringBuilder("{ ");
            for (int i = 0; i < obj.Members.Count; i++)
            {
                var part = TryFlat(obj.Members[i].Value);
                if (part == null) return null;
                if (i > 0) text.Append(", ");
                text.Append(obj.Members[i].Key).Append(" = ").Append(part);
                if (text.Length > InlineLimit) return null;
            }
            return text.Append(" }").ToString();
        }

        private static void Pad(StringBuilder sb, int indent) => sb.Append(' ', indent);

        private static void Emit(StringBuilder sb, Node node, int indent)
        {
            var flat = TryFlat(node);
            if (flat != null) { sb.Append(flat); return; }

            var arr = node as Arr;
            if (arr != null)
            {
                var allScalar = true;
                foreach (var item in arr.Items) if (!(item is Scalar)) { allScalar = false; break; }

                sb.Append("[\n");
                if (allScalar)
                {
                    // Long lists of simple values, twelve to a line.
                    const int perLine = 12;
                    for (int i = 0; i < arr.Items.Count; i++)
                    {
                        if (i % perLine == 0) Pad(sb, indent + 2);
                        sb.Append(((Scalar)arr.Items[i]).Text);
                        var last = i == arr.Items.Count - 1;
                        if (!last) sb.Append(',');
                        if (last || (i + 1) % perLine == 0) sb.Append('\n'); else sb.Append(' ');
                    }
                }
                else
                {
                    for (int i = 0; i < arr.Items.Count; i++)
                    {
                        Pad(sb, indent + 2);
                        Emit(sb, arr.Items[i], indent + 2);
                        if (i < arr.Items.Count - 1) sb.Append(',');
                        sb.Append('\n');
                    }
                }
                Pad(sb, indent);
                sb.Append(']');
                return;
            }

            var obj = (Obj)node;
            sb.Append("{\n");
            for (int i = 0; i < obj.Members.Count; i++)
            {
                Pad(sb, indent + 2);
                sb.Append(obj.Members[i].Key).Append(" = ");
                Emit(sb, obj.Members[i].Value, indent + 2);
                if (i < obj.Members.Count - 1) sb.Append(',');
                sb.Append('\n');
            }
            Pad(sb, indent);
            sb.Append('}');
        }
    }
}
