using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// What a bind did, and what it could not do. The bake wizard shows this after every bake
    /// so a half-empty asset never happens silently, and runtime code can log it the same way.
    /// </summary>
    public sealed class MdixBindReport
    {
        /// <summary>Values and containers that were read from the data.</summary>
        public int BoundValues;

        /// <summary>Members of the C# type that were looked at, and how many of them found data.</summary>
        public int MembersSeen;
        public int MembersMatched;

        /// <summary>Members that found no data, as "Type.member (looked for: a, b)".</summary>
        public readonly List<string> MissingMembers = new List<string>();

        /// <summary>Keys in the data that no member of the C# type reads.</summary>
        public readonly List<string> UnusedKeys = new List<string>();

        /// <summary>Data that exists but could not be converted, as "path: reason".</summary>
        public readonly List<string> Problems = new List<string>();

        public bool IsClean => MissingMembers.Count == 0 && UnusedKeys.Count == 0 && Problems.Count == 0;

        /// <summary>True when the data fills nothing in this type, so baking would produce an empty asset.</summary>
        public bool NothingMatched => MembersMatched == 0;

        /// <summary>
        /// The report in a few lines: what could not be converted, which members found no value and
        /// which keys nothing reads. Each group lists at most <paramref name="maxPerGroup"/> entries
        /// (zero or less lists them all). Empty when the bind was clean.
        /// </summary>
        public string Describe(int maxPerGroup = 5)
        {
            var lines = new List<string>();
            var limit = maxPerGroup > 0 ? maxPerGroup : int.MaxValue;
            AppendGroup(lines, "Cannot convert", Problems, limit);
            AppendGroup(lines, "No value in the data for", MissingMembers, limit);
            AppendGroup(lines, "In the data but read by nothing", UnusedKeys, limit);
            return string.Join("\n", lines);
        }

        private static void AppendGroup(List<string> lines, string heading, List<string> items, int limit)
        {
            if (items.Count == 0) return;

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
    /// Fills a C# object from an <see cref="MdixDatabase"/> by walking the object's own members.
    ///
    /// Why this exists instead of <c>MdixDatabase.Deserialize&lt;T&gt;</c>: the serializer maps
    /// public *properties* only and cannot fill list or array members, so a ScriptableObject with
    /// an <c>enemies</c> list always came out empty, and it builds the target with <c>new</c>, which
    /// ScriptableObject does not allow. The binder works on the instance it is given, reads public
    /// fields and [SerializeField] fields (what Unity serializes) as well as writable properties,
    /// and handles nesting, lists, arrays and enums to any depth.
    ///
    /// Data keys are matched to members by trying, in order: an explicit [MdixProperty] / [MdixAlias]
    /// path, the member name as written, the name without a leading underscore or "m_", snake_case
    /// (both the library's rule and an acronym-aware one), lower camel case and lower case. So
    /// <c>SpawnCap</c>, <c>spawnCap</c> and <c>spawn_cap</c> all read the key <c>spawn_cap</c>.
    /// Enums are matched by their number, which is what the data stores.
    ///
    /// Used by the bake wizard in the Editor and available at runtime through
    /// <see cref="Create{T}(MdixDatabase, string)"/> and <see cref="MdixAsset.Bind{T}"/>. It works by
    /// reflection, so on IL2CPP keep the data classes from being stripped ([Preserve] or a link.xml entry).
    /// </summary>
    public static class MdixBinder
    {
        private const int MaxDepth        = 32;
        private const int ProbeItemLimit  = 3;

        private static readonly Dictionary<Type, List<BindMember>> MemberCache =
            new Dictionary<Type, List<BindMember>>();

        private static readonly Regex IndexPattern = new Regex(@"\[\d+\]", RegexOptions.Compiled);

        // ── Public entry points ───────────────────────────────────────────────

        /// <summary>Fills <paramref name="target"/> from the data at <paramref name="dataPath"/> (empty = root).</summary>
        public static MdixBindReport Bind(MdixDatabase db, object target, string dataPath)
        {
            if (db == null)     throw new ArgumentNullException(nameof(db));
            if (target == null) throw new ArgumentNullException(nameof(target));

            var ctx = new Ctx(db, new MdixBindReport(), apply: true);
            BindObject(ctx, target, target.GetType(), dataPath ?? string.Empty, 0);
            return ctx.Report;
        }

        /// <summary>
        /// Builds a new <typeparamref name="T"/> and fills it from the data at <paramref name="dataPath"/>
        /// (empty = root), including lists, arrays and nested classes. Fails only when data exists but
        /// cannot be converted (a text where a number is expected, a number that does not fit, ...);
        /// members without data keep the values their constructor gave them. Use the overload with a
        /// report to see what was missing or unused.
        /// Not for ScriptableObject types: make those with ScriptableObject.CreateInstance and pass
        /// the instance to <see cref="Bind"/>.
        /// </summary>
        public static MdixResult<T> Create<T>(MdixDatabase db, string dataPath = "") where T : new()
        {
            MdixBindReport report;
            return Create<T>(db, dataPath, out report);
        }

        /// <inheritdoc cref="Create{T}(MdixDatabase, string)"/>
        public static MdixResult<T> Create<T>(MdixDatabase db, string dataPath, out MdixBindReport report)
            where T : new()
        {
            report = null;
            if (db == null) return MdixResult<T>.Err(MdixError.NullHandle());

            object boxed;
            try { boxed = new T(); }
            catch (Exception ex)
            {
                return MdixResult<T>.Err(MdixError.NativeError(
                    "MdixBinder.Create: could not create " + typeof(T).Name + " (" + ex.Message + ")"));
            }

            try { report = Bind(db, boxed, dataPath); }
            catch (Exception ex)
            {
                return MdixResult<T>.Err(MdixError.NativeError(
                    "MdixBinder.Create: " + typeof(T).Name + " failed (" + ex.Message + ")"));
            }

            if (report.Problems.Count > 0)
                return MdixResult<T>.Err(MdixError.SchemaError(
                    "Data does not fit " + typeof(T).Name + ".\n" + report.Describe(5)));

            // Boxed so that a struct T is filled too, not a copy of it.
            return MdixResult<T>.Ok((T)boxed);
        }

        /// <summary>
        /// Same walk without creating or assigning anything, to learn how well a type fits some data.
        /// Safe to call on a ScriptableObject type, since no instance is needed.
        /// </summary>
        public static MdixBindReport Probe(MdixDatabase db, Type type, string dataPath)
        {
            if (db == null)   throw new ArgumentNullException(nameof(db));
            if (type == null) throw new ArgumentNullException(nameof(type));

            var ctx = new Ctx(db, new MdixBindReport(), apply: false);
            BindObject(ctx, null, type, dataPath ?? string.Empty, 0);
            return ctx.Report;
        }

        /// <summary>
        /// Things Unity's own serializer will silently drop even though the binder filled them:
        /// properties without a serialized backing field, and nested types that are not [Serializable].
        /// </summary>
        public static List<string> FindSerializationGaps(Type root)
        {
            var gaps    = new List<string>();
            var visited = new HashSet<Type>();
            CollectGaps(root, gaps, visited, 0);
            return gaps;
        }

        // ── Members ───────────────────────────────────────────────────────────

        private sealed class BindMember
        {
            public string     Name;
            public Type       Type;
            public FieldInfo  Field;
            public PropertyInfo Property;
            public string[]   Candidates;

            public void Set(object target, object value)
            {
                if (Field != null) Field.SetValue(target, value);
                else               Property.SetValue(target, value);
            }
        }

        private static List<BindMember> GetMembers(Type type)
        {
            List<BindMember> cached;
            lock (MemberCache)
            {
                if (MemberCache.TryGetValue(type, out cached)) return cached;
            }

            var list  = new List<BindMember>();
            var names = new HashSet<string>(StringComparer.Ordinal);
            const BindingFlags flags =
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

            for (var t = type; t != null && t != typeof(object) && !IsUnityType(t); t = t.BaseType)
            {
                foreach (var f in t.GetFields(flags))
                {
                    if (f.IsStatic || f.IsInitOnly || f.IsLiteral) continue;
                    if (f.Name.IndexOf('<') >= 0) continue;                       // compiler-generated backing field
                    if (f.IsDefined(typeof(NonSerializedAttribute), true)) continue;
                    if (!f.IsPublic && !HasAttributeNamed(f, "SerializeField")) continue;
                    if (!names.Add(f.Name)) continue;

                    list.Add(new BindMember
                    {
                        Name       = f.Name,
                        Type       = f.FieldType,
                        Field      = f,
                        Candidates = BuildCandidates(f, f.Name),
                    });
                }

                foreach (var p in t.GetProperties(flags))
                {
                    if (p.GetIndexParameters().Length > 0) continue;
                    if (p.GetSetMethod(false) == null) continue;                  // public setter only
                    if (p.IsDefined(typeof(MdixIgnoreAttribute), true)) continue;
                    if (!names.Add(p.Name)) continue;

                    list.Add(new BindMember
                    {
                        Name       = p.Name,
                        Type       = p.PropertyType,
                        Property   = p,
                        Candidates = BuildCandidates(p, p.Name),
                    });
                }
            }

            lock (MemberCache) { MemberCache[type] = list; }
            return list;
        }

        private static bool HasAttributeNamed(MemberInfo member, string attributeName)
        {
            foreach (var a in member.GetCustomAttributes(true))
                if (a.GetType().Name == attributeName) return true;
            return false;
        }

        private static bool IsUnityType(Type t)
        {
            var ns = t.Namespace;
            return ns != null && (ns == "UnityEngine" || ns.StartsWith("UnityEngine.", StringComparison.Ordinal) ||
                                  ns == "UnityEditor" || ns.StartsWith("UnityEditor.", StringComparison.Ordinal));
        }

        private static bool DerivesFromUnityObject(Type t) =>
            typeof(UnityEngine.Object).IsAssignableFrom(t);

        // ── Names ─────────────────────────────────────────────────────────────

        private static string[] BuildCandidates(MemberInfo member, string name)
        {
            var list = new List<string>();

            if (member is PropertyInfo)
            {
                var path = member.GetCustomAttribute<MdixPropertyAttribute>(true);
                if (path != null) AddUnique(list, path.Path);

                foreach (var alias in member.GetCustomAttributes<MdixAliasAttribute>(true))
                    AddUnique(list, alias.AliasPath);
            }

            var bare = StripPrefix(name);

            AddUnique(list, name);
            AddUnique(list, bare);
            AddUnique(list, ToSnakeCase(bare));
            AddUnique(list, ToSnakeCaseSmart(bare));
            AddUnique(list, LowerFirst(bare));
            AddUnique(list, bare.ToLowerInvariant());

            return list.ToArray();
        }

        private static void AddUnique(List<string> list, string value)
        {
            if (!string.IsNullOrEmpty(value) && !list.Contains(value)) list.Add(value);
        }

        private static string StripPrefix(string name)
        {
            if (name.StartsWith("m_", StringComparison.Ordinal) && name.Length > 2) return name.Substring(2);
            if (name.StartsWith("_", StringComparison.Ordinal)  && name.Length > 1) return name.Substring(1);
            return name;
        }

        private static string LowerFirst(string s) =>
            s.Length == 0 ? s : char.ToLowerInvariant(s[0]) + s.Substring(1);

        /// <summary>The serializer's own rule: an underscore before every capital except the first.</summary>
        private static string ToSnakeCase(string name)
        {
            var sb = new StringBuilder(name.Length + 4);
            for (int i = 0; i < name.Length; i++)
            {
                if (i > 0 && char.IsUpper(name[i])) sb.Append('_');
                sb.Append(char.ToLowerInvariant(name[i]));
            }
            return sb.ToString();
        }

        /// <summary>Keeps acronyms together: AIType becomes ai_type, MaxHP becomes max_hp.</summary>
        private static string ToSnakeCaseSmart(string name)
        {
            var sb = new StringBuilder(name.Length + 4);
            for (int i = 0; i < name.Length; i++)
            {
                var c = name[i];
                if (i > 0 && char.IsUpper(c))
                {
                    var prev     = name[i - 1];
                    var nextLow  = i + 1 < name.Length && char.IsLower(name[i + 1]);
                    if (char.IsLower(prev) || char.IsDigit(prev) || (char.IsUpper(prev) && nextLow))
                        sb.Append('_');
                }
                sb.Append(char.ToLowerInvariant(c));
            }
            return sb.ToString();
        }

        // ── Binding ───────────────────────────────────────────────────────────

        private sealed class Ctx
        {
            public readonly MdixDatabase  Db;
            public readonly MdixBindReport Report;
            public readonly bool          Apply;

            public Ctx(MdixDatabase db, MdixBindReport report, bool apply)
            {
                Db = db; Report = report; Apply = apply;
            }

            public void Problem(string path, string reason)
            {
                var text = Normalize(path) + ": " + reason;
                if (!Report.Problems.Contains(text)) Report.Problems.Add(text);
            }
        }

        private enum Read { Missing, Ok, Failed }

        private static void BindObject(Ctx ctx, object instance, Type type, string path, int depth)
        {
            var members  = GetMembers(type);
            var consumed = new HashSet<string>(StringComparer.Ordinal);

            foreach (var m in members)
            {
                ctx.Report.MembersSeen++;

                var    result = Read.Missing;
                object value  = null;
                string usedKey = null;

                foreach (var candidate in m.Candidates)
                {
                    result = ReadValue(ctx, Join(path, candidate), m.Type, depth, out value);
                    if (result != Read.Missing) { usedKey = candidate; break; }
                }

                if (result == Read.Missing)
                {
                    var looked = string.Join(", ", m.Candidates, 0, Math.Min(3, m.Candidates.Length));
                    AddUnique(ctx.Report.MissingMembers, type.Name + "." + m.Name + " (looked for: " + looked + ")");
                    continue;
                }

                consumed.Add(FirstSegment(usedKey));

                if (result != Read.Ok) continue;           // found, but could not be converted: already reported

                ctx.Report.MembersMatched++;

                if (ctx.Apply && instance != null)
                {
                    try { m.Set(instance, value); }
                    catch (Exception ex) { ctx.Problem(Join(path, usedKey), ex.Message); }
                }
            }

            ReportUnusedKeys(ctx, path, consumed);
        }

        private static void ReportUnusedKeys(Ctx ctx, string path, HashSet<string> consumed)
        {
            var keys = ctx.Db.GetKeys(string.IsNullOrEmpty(path) ? null : path);
            if (keys.IsFailure) return;

            foreach (var key in keys.SuccessResult)
            {
                // The root listing also carries every array item as "name[0]"; those belong to their array.
                if (key.IndexOf('[') >= 0 || key.IndexOf('.') >= 0) continue;
                if (consumed.Contains(key)) continue;

                AddUnique(ctx.Report.UnusedKeys, Normalize(Join(path, key)));
            }
        }

        private static Read ReadValue(Ctx ctx, string path, Type target, int depth, out object value)
        {
            value = null;

            if (depth > MaxDepth)
                return Fail(ctx, path, "nesting is deeper than " + MaxDepth + " levels");

            var type = Nullable.GetUnderlyingType(target) ?? target;

            var vt = ctx.Db.GetValueType(path);
            if (vt == MdixValueType.Unknown) return Read.Missing;

            if (vt == MdixValueType.Null)
            {
                value = DefaultOf(target);
                ctx.Report.BoundValues++;
                return Read.Ok;
            }

            // Lists and arrays
            Type element; bool isArray;
            if (TryGetCollection(type, out element, out isArray))
                return ReadCollection(ctx, path, vt, type, element, isArray, depth, out value);

            if (type == typeof(string))  return ReadString(ctx, path, vt, out value);
            if (type == typeof(bool))    return ReadBool(ctx, path, vt, out value);
            if (type.IsEnum)             return ReadEnum(ctx, path, vt, type, out value);
            if (IsNumber(type))          return ReadNumber(ctx, path, vt, type, out value);
            if (IsColorLike(type))       return ReadColor(ctx, path, vt, type, out value);

            if (type == typeof(MdixDate))
            {
                if (vt != MdixValueType.Date) return Fail(ctx, path, "expected a date but the data holds " + vt);
                var r = ctx.Db.GetDate(path);
                if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                value = r.SuccessResult; ctx.Report.BoundValues++; return Read.Ok;
            }

            if (type == typeof(MdixTimestamp))
            {
                if (vt != MdixValueType.Timestamp) return Fail(ctx, path, "expected a timestamp but the data holds " + vt);
                var r = ctx.Db.GetTimestamp(path);
                if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                value = r.SuccessResult; ctx.Report.BoundValues++; return Read.Ok;
            }

            if (DerivesFromUnityObject(type))
                return Fail(ctx, path, type.Name + " is a Unity object; data cannot create it");

            if (IsObjectLike(type))
                return ReadObject(ctx, path, vt, type, depth, out value);

            return Fail(ctx, path, "no way to convert data into " + type.Name);
        }

        private static Read ReadObject(Ctx ctx, string path, MdixValueType vt, Type type, int depth, out object value)
        {
            value = null;

            if (vt != MdixValueType.Object)
                return Fail(ctx, path, "expected an object but the data holds " + vt);

            object instance = null;
            if (ctx.Apply)
            {
                try { instance = Activator.CreateInstance(type, true); }
                catch (Exception ex)
                {
                    return Fail(ctx, path, type.Name + " needs a parameterless constructor (" + ex.Message + ")");
                }
            }

            BindObject(ctx, instance, type, path, depth + 1);
            value = instance;
            ctx.Report.BoundValues++;
            return Read.Ok;
        }

        private static Read ReadCollection(
            Ctx ctx, string path, MdixValueType vt, Type declared, Type element, bool isArray, int depth,
            out object value)
        {
            value = null;

            if (vt != MdixValueType.Array)
                return Fail(ctx, path, "expected an array but the data holds " + vt);

            var length = ctx.Db.GetArrayLength(path);
            if (length.IsFailure) return Fail(ctx, path, length.Error.Message);

            var count = length.SuccessResult;
            var limit = ctx.Apply ? count : Math.Min(count, ProbeItemLimit);

            var items = ctx.Apply ? new List<object>(count) : null;

            for (int i = 0; i < limit; i++)
            {
                object item;
                var itemPath = path + "[" + i + "]";
                var r = ReadValue(ctx, itemPath, element, depth + 1, out item);

                if (r == Read.Missing) ctx.Problem(itemPath, "the array item is missing");
                if (items != null) items.Add(r == Read.Ok ? item : DefaultOf(element));   // keep indices aligned with the data
            }

            if (items != null)
            {
                if (isArray)
                {
                    var array = Array.CreateInstance(element, items.Count);
                    for (int i = 0; i < items.Count; i++) array.SetValue(items[i], i);
                    value = array;
                }
                else
                {
                    var list = CreateList(declared, element);
                    for (int i = 0; i < items.Count; i++) list.Add(items[i]);
                    value = list;
                }
            }

            ctx.Report.BoundValues++;
            return Read.Ok;
        }

        /// <summary>
        /// The member's own declared List&lt;T&gt; when it is one (that closed type is already in the player,
        /// which matters on IL2CPP); a constructed List&lt;T&gt; only for interface-typed members.
        /// </summary>
        private static IList CreateList(Type declared, Type element)
        {
            if (!declared.IsInterface && !declared.IsAbstract && !declared.IsArray)
            {
                var made = Activator.CreateInstance(declared, true) as IList;
                if (made != null) return made;
            }
            return (IList)Activator.CreateInstance(typeof(List<>).MakeGenericType(element));
        }

        private static bool TryGetCollection(Type type, out Type element, out bool isArray)
        {
            element = null; isArray = false;

            if (type.IsArray)
            {
                if (type.GetArrayRank() != 1) return false;
                element = type.GetElementType(); isArray = true;
                return true;
            }

            if (!type.IsGenericType) return false;

            var def = type.GetGenericTypeDefinition();
            if (def == typeof(List<>)               || def == typeof(IList<>)  ||
                def == typeof(ICollection<>)        || def == typeof(IEnumerable<>) ||
                def == typeof(IReadOnlyList<>)      || def == typeof(IReadOnlyCollection<>))
            {
                element = type.GetGenericArguments()[0];
                return true;
            }
            return false;
        }

        private static bool IsObjectLike(Type type)
        {
            if (type.IsPrimitive || type.IsEnum || type == typeof(string)) return false;
            if (type.IsAbstract || type.IsInterface || type.IsGenericTypeDefinition) return false;
            return type.IsClass || type.IsValueType;
        }

        // ── Scalars ───────────────────────────────────────────────────────────

        private static Read ReadString(Ctx ctx, string path, MdixValueType vt, out object value)
        {
            value = null;

            switch (vt)
            {
                case MdixValueType.String:
                {
                    var r = ctx.Db.GetString(path);
                    if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                    value = r.SuccessResult;
                    break;
                }
                case MdixValueType.Int:
                {
                    var r = ctx.Db.GetInt(path);
                    if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                    value = r.SuccessResult.ToString(CultureInfo.InvariantCulture);
                    break;
                }
                case MdixValueType.Long:
                {
                    var r = ctx.Db.GetLong(path);
                    if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                    value = r.SuccessResult.ToString(CultureInfo.InvariantCulture);
                    break;
                }
                case MdixValueType.Float:
                {
                    var r = ctx.Db.GetFloat(path);
                    if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                    value = r.SuccessResult.ToString(CultureInfo.InvariantCulture);
                    break;
                }
                case MdixValueType.Double:
                {
                    var r = ctx.Db.GetDouble(path);
                    if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                    value = r.SuccessResult.ToString(CultureInfo.InvariantCulture);
                    break;
                }
                case MdixValueType.Bool:
                {
                    var r = ctx.Db.GetBool(path);
                    if (r.IsFailure) return Fail(ctx, path, r.Error.Message);
                    value = r.SuccessResult ? "true" : "false";
                    break;
                }
                default:
                    return Fail(ctx, path, "expected text but the data holds " + vt);
            }

            ctx.Report.BoundValues++;
            return Read.Ok;
        }

        private static Read ReadBool(Ctx ctx, string path, MdixValueType vt, out object value)
        {
            value = null;

            if (vt != MdixValueType.Bool)
                return Fail(ctx, path, "expected true or false but the data holds " + vt);

            var r = ctx.Db.GetBool(path);
            if (r.IsFailure) return Fail(ctx, path, r.Error.Message);

            value = r.SuccessResult;
            ctx.Report.BoundValues++;
            return Read.Ok;
        }

        private static bool IsNumber(Type t) =>
            t == typeof(int)   || t == typeof(long)  || t == typeof(short)  || t == typeof(byte)   ||
            t == typeof(sbyte) || t == typeof(uint)  || t == typeof(ushort) || t == typeof(ulong)  ||
            t == typeof(float) || t == typeof(double) || t == typeof(decimal);

        /// <summary>Reads any numeric kind the data holds, as a long when it is whole and a double otherwise.</summary>
        private static bool TryReadNumeric(
            Ctx ctx, string path, MdixValueType vt, out bool integral, out long whole, out double real, out string error)
        {
            integral = false; whole = 0; real = 0; error = null;

            switch (vt)
            {
                case MdixValueType.Int:
                {
                    var r = ctx.Db.GetInt(path);
                    if (r.IsFailure) { error = r.Error.Message; return false; }
                    integral = true; whole = r.SuccessResult; real = whole; return true;
                }
                case MdixValueType.Enum:
                {
                    var r = ctx.Db.GetEnumValue(path);
                    if (r.IsFailure) { error = r.Error.Message; return false; }
                    integral = true; whole = r.SuccessResult; real = whole; return true;
                }
                case MdixValueType.Long:
                {
                    var r = ctx.Db.GetLong(path);
                    if (r.IsFailure) { error = r.Error.Message; return false; }
                    integral = true; whole = r.SuccessResult; real = whole; return true;
                }
                case MdixValueType.Float:
                {
                    var r = ctx.Db.GetFloat(path);
                    if (r.IsFailure) { error = r.Error.Message; return false; }
                    real = r.SuccessResult; return true;
                }
                case MdixValueType.Double:
                {
                    var r = ctx.Db.GetDouble(path);
                    if (r.IsFailure) { error = r.Error.Message; return false; }
                    real = r.SuccessResult; return true;
                }
                default:
                    error = "expected a number but the data holds " + vt;
                    return false;
            }
        }

        private static Read ReadNumber(Ctx ctx, string path, MdixValueType vt, Type type, out object value)
        {
            value = null;

            bool integral; long whole; double real; string error;
            if (!TryReadNumeric(ctx, path, vt, out integral, out whole, out real, out error))
                return Fail(ctx, path, error);

            if (!ctx.Apply) { ctx.Report.BoundValues++; return Read.Ok; }

            try
            {
                if (type == typeof(double))       value = real;
                else if (type == typeof(float))   value = (float)real;
                else if (type == typeof(decimal)) value = (decimal)real;
                else
                {
                    if (!integral)
                    {
                        if (real != Math.Floor(real))
                            return Fail(ctx, path, "the value " + real.ToString(CultureInfo.InvariantCulture) +
                                                   " has a fraction and does not fit " + type.Name);
                        if (real < long.MinValue || real > long.MaxValue)
                            return Fail(ctx, path, "the value does not fit " + type.Name);
                        whole = (long)real;
                    }
                    value = Convert.ChangeType(whole, type, CultureInfo.InvariantCulture);
                }
            }
            catch (OverflowException)
            {
                value = null;
                return Fail(ctx, path, "the value does not fit " + type.Name);
            }

            ctx.Report.BoundValues++;
            return Read.Ok;
        }

        private static Read ReadEnum(Ctx ctx, string path, MdixValueType vt, Type type, out object value)
        {
            value = null;
            object boxed;

            if (vt == MdixValueType.String)
            {
                var s = ctx.Db.GetString(path);
                if (s.IsFailure) return Fail(ctx, path, s.Error.Message);

                object parsed;
                try { parsed = Enum.Parse(type, s.SuccessResult, true); }
                catch (ArgumentException)
                {
                    return Fail(ctx, path, "\"" + s.SuccessResult + "\" is not a member of " + type.Name);
                }
                boxed = parsed;
            }
            else
            {
                bool integral; long whole; double real; string error;
                if (!TryReadNumeric(ctx, path, vt, out integral, out whole, out real, out error) || !integral)
                    return Fail(ctx, path, error ?? "expected an enum number but the data holds " + vt);

                try
                {
                    var underlying = Convert.ChangeType(whole, Enum.GetUnderlyingType(type), CultureInfo.InvariantCulture);
                    boxed = Enum.ToObject(type, underlying);
                }
                catch (OverflowException)
                {
                    return Fail(ctx, path, "the number " + whole + " does not fit " + type.Name);
                }

                var isFlags = type.IsDefined(typeof(FlagsAttribute), false);
                if (!isFlags && !Enum.IsDefined(type, boxed))
                    return Fail(ctx, path, "the number " + whole + " is not a member of " + type.Name);
            }

            if (ctx.Apply) value = boxed;
            ctx.Report.BoundValues++;
            return Read.Ok;
        }

        private static bool IsColorLike(Type t) =>
            t == typeof(MdixHexColor) || t == typeof(UnityEngine.Color);

        private static Read ReadColor(Ctx ctx, string path, MdixValueType vt, Type type, out object value)
        {
            value = null;

            if (vt != MdixValueType.HexColor)
                return Fail(ctx, path, "expected a hex colour but the data holds " + vt);

            var r = ctx.Db.GetHexColor(path);
            if (r.IsFailure) return Fail(ctx, path, r.Error.Message);

            if (ctx.Apply)
            {
                var c = r.SuccessResult;
                value = type == typeof(MdixHexColor)
                    ? (object)c
                    : new UnityEngine.Color(c.R, c.G, c.B, c.A);
            }

            ctx.Report.BoundValues++;
            return Read.Ok;
        }

        // ── Small helpers ─────────────────────────────────────────────────────

        private static Read Fail(Ctx ctx, string path, string reason)
        {
            ctx.Problem(path, reason);
            return Read.Failed;
        }

        private static object DefaultOf(Type t) =>
            t.IsValueType ? Activator.CreateInstance(t) : null;

        private static string Join(string prefix, string key) =>
            string.IsNullOrEmpty(prefix) ? key : prefix + "." + key;

        private static string FirstSegment(string key)
        {
            var dot = key.IndexOf('.');
            var bracket = key.IndexOf('[');
            var cut = dot < 0 ? bracket : (bracket < 0 ? dot : Math.Min(dot, bracket));
            return cut < 0 ? key : key.Substring(0, cut);
        }

        private static string Normalize(string path) => IndexPattern.Replace(path, "[]");

        // ── Serialization gaps ────────────────────────────────────────────────

        private static void CollectGaps(Type type, List<string> gaps, HashSet<Type> visited, int depth)
        {
            if (depth > 8 || type == null || !visited.Add(type)) return;

            foreach (var m in GetMembers(type))
            {
                if (m.Property != null)
                {
                    var backing = type.GetField("<" + m.Name + ">k__BackingField",
                        BindingFlags.NonPublic | BindingFlags.Instance);

                    var serialised = backing != null && HasAttributeNamed(backing, "SerializeField");
                    if (!serialised)
                        gaps.Add(type.Name + "." + m.Name + " is a property without [field: SerializeField], " +
                                 "so Unity will not store it (use a public field or add the attribute)");
                }

                var inner = Nullable.GetUnderlyingType(m.Type) ?? m.Type;
                Type element; bool isArray;
                if (TryGetCollection(inner, out element, out isArray)) inner = element;

                if (inner.IsPrimitive || inner.IsEnum || inner == typeof(string) || inner == typeof(decimal)) continue;
                if (IsUnityType(inner) || inner.IsInterface || inner.IsAbstract) continue;

                if (!inner.IsSerializable)
                {
                    gaps.Add(inner.Name + " is not marked [Serializable], so Unity will not store " +
                             type.Name + "." + m.Name);
                    continue;
                }

                CollectGaps(inner, gaps, visited, depth + 1);
            }
        }
    }
}
