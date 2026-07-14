using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Json;

namespace MidManStudio.Mdix.Core
{
    // ══════════════════════════════════════════════════════════════════════════
    // Public attributes
    // ══════════════════════════════════════════════════════════════════════════

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct)]
    public sealed class MdixObjectAttribute : Attribute
    {
        public string? Prefix { get; }
        public MdixObjectAttribute(string? prefix = null) => Prefix = prefix;
    }

    [AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
    public sealed class MdixPropertyAttribute : Attribute
    {
        public string Path { get; }
        public MdixPropertyAttribute(string path) => Path = path;
    }

    [AttributeUsage(AttributeTargets.Property, AllowMultiple = true)]
    public sealed class MdixAliasAttribute : Attribute
    {
        public string AliasPath { get; }
        public MdixAliasAttribute(string aliasPath) => AliasPath = aliasPath;
    }

    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixIgnoreAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
    public sealed class MdixRequiredAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixDefaultValueAttribute : Attribute
    {
        public object? DefaultValue { get; }
        public MdixDefaultValueAttribute(object? defaultValue) => DefaultValue = defaultValue;
    }

    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixTransformAttribute : Attribute
    {
        public Func<object, object>? Transform { get; }

        public MdixTransformAttribute(Type transformerType, string methodName)
        {
            var m = transformerType.GetMethod(methodName, BindingFlags.Public | BindingFlags.Static);
            if (m == null)
                throw new InvalidOperationException(
                    $"[MdixTransform] could not find a public static method named " +
                    $"'{methodName}' on '{transformerType.Name}'. Check the method name for " +
                    "typos, and that it's public and static.");
            Transform = obj => m.Invoke(null, new[] { obj })!;
        }
    }

    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixValidationAttribute : Attribute
    {
        public Func<object, bool>? Validator { get; }

        public MdixValidationAttribute(Type validatorType, string methodName)
        {
            var m = validatorType.GetMethod(methodName, BindingFlags.Public | BindingFlags.Static);
            if (m == null)
                throw new InvalidOperationException(
                    $"[MdixValidation] could not find a public static method named " +
                    $"'{methodName}' on '{validatorType.Name}'. Check the method name for " +
                    "typos, and that it's public and static.");
            Validator = obj => (bool)m.Invoke(null, new[] { obj })!;
        }
    }

    [AttributeUsage(AttributeTargets.Constructor)]
    public sealed class MdixConstructorAttribute : Attribute { }

    public enum MdixConversionMode { Strict, Safe, Forced }

    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixConvertAttribute : Attribute
    {
        public MdixConversionMode Mode { get; }
        public MdixConvertAttribute(MdixConversionMode mode = MdixConversionMode.Safe) =>
            Mode = mode;
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Serializer
    // ══════════════════════════════════════════════════════════════════════════

    internal sealed class MdixSerializer
    {
        private static readonly MethodInfo _deserializeMethod =
            typeof(MdixSerializer)
                .GetMethods(BindingFlags.Instance | BindingFlags.NonPublic)
                .First(m => m.Name == nameof(Deserialize) && m.IsGenericMethodDefinition);

        private static readonly Dictionary<Type, TypeSerializationInfo> _cache = new();
        private static readonly object _cacheLock = new();

        // A self-referential or cyclic POCO graph (Node.Parent: Node, or
        // A.B: B / B.A: A) would otherwise recurse forever here -- an unfound
        // *optional* property isn't a failure, it just keeps the type's
        // default value, so nothing about a missing "parent.parent.parent..."
        // path in the actual data ever stops the recursion on its own.
        // StackOverflowException is not catchable in .NET; by the time that
        // would fire, the whole process is already gone. Two independent
        // guards below: a hard depth ceiling (catches everything, including
        // pathological-but-technically-acyclic nesting), and explicit type
        // cycle detection (gives a far more actionable error message for the
        // common case -- "X refers back to itself" instead of "hit a limit").
        private const int MaxNestingDepth = 32;

        // ── Deserialization ───────────────────────────────────────────────────

        internal MdixResult<T> Deserialize<T>(
            MdixDatabase db, string? prefix = null,
            HashSet<Type>? typeStack = null, int depth = 0)
        {
            typeStack ??= new HashSet<Type>();

            if (depth > MaxNestingDepth)
                return MdixError.NativeError(
                    $"Deserialize<{typeof(T).Name}>: nesting depth exceeded {MaxNestingDepth} " +
                    "levels. This almost always means a self-referential or cyclic type graph " +
                    "(a type whose property chain eventually refers back to itself) -- mark the " +
                    "back-reference [MdixIgnore], or restructure so Mdix doesn't need to " +
                    "represent it. If this is a genuinely, intentionally deep (but non-cyclic) " +
                    "object graph, this ceiling is a constant in MdixSerializer.cs.");

            // Value types can't directly contain a field of their own type (the
            // compiler already rejects that as an unbounded-size type), so only
            // reference types need cycle tracking here.
            bool tracksCycle = !typeof(T).IsValueType && typeStack.Add(typeof(T));
            if (!tracksCycle && !typeof(T).IsValueType)
                return MdixError.NativeError(
                    $"Deserialize<{typeof(T).Name}>: cyclic type graph detected -- " +
                    $"'{typeof(T).Name}' already appears earlier in its own property chain " +
                    "(directly or through another type). Mark the back-reference [MdixIgnore], " +
                    "or restructure the type graph so it doesn't need to round-trip through Mdix.");

            try
            {
                var typeInfo        = GetOrBuildTypeInfo(typeof(T));
                var effectivePrefix = prefix ?? typeInfo.ClassPrefix ?? string.Empty;

                if (typeInfo.PrimaryConstructor != null)
                    return DeserializeViaCtor<T>(db, typeInfo, effectivePrefix, typeStack, depth);

                if (!typeof(T).IsValueType && !HasParameterlessCtor(typeof(T)))
                    return MdixError.NativeError(
                        $"'{typeof(T).Name}' needs a parameterless constructor or a constructor " +
                        "whose parameters are mappable via [MdixProperty].");

                var instance = Activator.CreateInstance<T>();
                object boxed = instance!;

                var err = FillProperties(db, typeInfo, effectivePrefix, ref boxed, null, typeStack, depth);
                if (err.HasValue) return MdixResult<T>.Err(err.Value);

                return MdixResult<T>.Ok((T)boxed);
            }
            catch (Exception ex)
            {
                return MdixError.NativeError($"Deserialize<{typeof(T).Name}> failed: {ex.Message}");
            }
            finally
            {
                if (tracksCycle) typeStack.Remove(typeof(T));
            }
        }

        private MdixResult<T> DeserializeViaCtor<T>(
            MdixDatabase db, TypeSerializationInfo typeInfo, string prefix,
            HashSet<Type> typeStack, int depth)
        {
            var ctor       = typeInfo.PrimaryConstructor!;
            var parameters = ctor.GetParameters();
            var values     = new object?[parameters.Length];

            for (int i = 0; i < parameters.Length; i++)
            {
                var param = parameters[i];
                var pInfo = typeInfo.CtorParams.FirstOrDefault(p =>
                    string.Equals(p.Param.Name, param.Name, StringComparison.OrdinalIgnoreCase));

                if (pInfo == null)
                {
                    values[i] = param.HasDefaultValue ? param.DefaultValue : DefaultOf(param.ParameterType);
                    continue;
                }

                var (found, value) = TryResolvePaths(
                    db, param.ParameterType, pInfo.Paths, prefix, typeStack, depth);
                if (found)
                {
                    values[i] = value;
                }
                else
                {
                    if (pInfo.IsRequired)
                        return MdixError.NativeError(
                            $"Required constructor parameter '{param.Name}' not found at '{pInfo.Paths[0]}'.");

                    values[i] = pInfo.DefaultValue
                        ?? (param.HasDefaultValue ? param.DefaultValue : DefaultOf(param.ParameterType));
                }
            }

            T instance;
            try   { instance = (T)ctor.Invoke(values); }
            catch (Exception ex)
            {
                return MdixError.NativeError(
                    $"Constructor invocation failed for '{typeof(T).Name}': {ex.Message}");
            }

            var ctorNames = new HashSet<string>(
                typeInfo.CtorParams.Select(p => p.Param.Name ?? string.Empty),
                StringComparer.OrdinalIgnoreCase);

            object boxed = instance!;
            var err = FillProperties(db, typeInfo, prefix, ref boxed, ctorNames, typeStack, depth);
            if (err.HasValue) return MdixResult<T>.Err(err.Value);

            return MdixResult<T>.Ok((T)boxed);
        }

        private MdixError? FillProperties(
            MdixDatabase          db,
            TypeSerializationInfo typeInfo,
            string                prefix,
            ref object            boxed,
            HashSet<string>?      skipNames,
            HashSet<Type>         typeStack,
            int                   depth)
        {
            foreach (var prop in typeInfo.Properties)
            {
                if (prop.IsIgnored) continue;
                if (!prop.PropInfo.CanWrite) continue;
                if (skipNames != null && skipNames.Contains(prop.PropInfo.Name)) continue;

                var (found, value) = TryResolvePaths(
                    db, prop.PropInfo.PropertyType, prop.Paths, prefix, typeStack, depth);

                if (found)
                {
                    if (prop.Transform != null)
                        value = prop.Transform(value!);

                    if (prop.Validator != null && !prop.Validator(value!))
                        return MdixError.NativeError(
                            $"Validation failed for '{prop.PropInfo.Name}' at '{prop.Paths[0]}'.");

                    prop.PropInfo.SetValue(boxed, value);
                }
                else if (prop.IsRequired)
                {
                    return MdixError.NativeError(
                        $"Required property '{prop.PropInfo.Name}' not found at '{prop.Paths[0]}'.");
                }
                else if (prop.DefaultValue != null)
                {
                    prop.PropInfo.SetValue(boxed, prop.DefaultValue);
                }
            }

            return null;
        }

        // ── Path resolution ───────────────────────────────────────────────────

        private (bool found, object? value) TryResolvePaths(
            MdixDatabase db, Type targetType, List<string> paths, string prefix,
            HashSet<Type> typeStack, int depth)
        {
            foreach (var rawPath in paths)
            {
                var fullPath = BuildFullPath(prefix, rawPath);

                if (IsComplexType(targetType))
                {
                    var nested = DeserializeNested(db, targetType, fullPath, typeStack, depth);
                    if (nested != null) return (true, nested);
                    continue;
                }

                var (success, val) = DirectGet(db, targetType, fullPath);
                if (success) return (true, val);

                if (fullPath.Contains('.'))
                {
                    var (success2, val2) = TryGetViaParentJson(db, targetType, fullPath);
                    if (success2) return (true, val2);
                }
            }

            return (false, null);
        }

        // ── Direct typed getter ───────────────────────────────────────────────

        private static (bool success, object? value) DirectGet(
            MdixDatabase db, Type targetType, string path)
        {
            try
            {
                if (targetType == typeof(string))
                {
                    var r = db.GetString(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(int))
                {
                    var r = db.GetInt(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                // FIX: use GetLong instead of GetInt+cast — avoids truncation of 64-bit values.
                if (targetType == typeof(long))
                {
                    var r = db.GetLong(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(float))
                {
                    var r = db.GetFloat(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(double))
                {
                    var r = db.GetDouble(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(bool))
                {
                    var r = db.GetBool(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(short))
                {
                    var r = db.GetInt(path);
                    return r.IsSuccess ? (true, (object)(short)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(byte))
                {
                    var r = db.GetInt(path);
                    return r.IsSuccess ? (true, (object)(byte)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(decimal))
                {
                    var r = db.GetDouble(path);
                    return r.IsSuccess ? (true, (object)(decimal)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(DateTime))
                {
                    var r = db.GetString(path);
                    if (r.IsSuccess && DateTime.TryParse(
                            r.SuccessResult,
                            System.Globalization.CultureInfo.InvariantCulture,
                            System.Globalization.DateTimeStyles.RoundtripKind,
                            out var dt))
                        return (true, (object)dt);
                    return (false, null);
                }
                if (targetType == typeof(MdixHexColor))
                {
                    var r = db.GetHexColor(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(MdixBlob))
                {
                    var r = db.GetBlob(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(MdixRegex))
                {
                    var r = db.GetRegex(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(MdixDate))
                {
                    var r = db.GetDate(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                if (targetType == typeof(MdixTimestamp))
                {
                    var r = db.GetTimestamp(path);
                    return r.IsSuccess ? (true, (object)r.SuccessResult) : (false, null);
                }
                return (false, null);
            }
            catch
            {
                return (false, null);
            }
        }

        // ── Parent JSON traversal fallback ────────────────────────────────────

        private static (bool success, object? value) TryGetViaParentJson(
            MdixDatabase db, Type targetType, string path)
        {
            try
            {
                var segments = path.Split('.');

                for (int parentLen = segments.Length - 1; parentLen >= 1; parentLen--)
                {
                    var parentPath = string.Join(".", segments, 0, parentLen);
                    var jsonResult = db.GetJson(parentPath);
                    if (jsonResult.IsFailure) continue;

                    JsonElement cloned;
                    using (var doc = JsonDocument.Parse(jsonResult.SuccessResult))
                    {
                        var el    = doc.RootElement;
                        bool found = true;

                        for (int i = parentLen; i < segments.Length; i++)
                        {
                            if (el.ValueKind != JsonValueKind.Object ||
                                !el.TryGetProperty(segments[i], out el))
                            {
                                found = false;
                                break;
                            }
                        }

                        if (!found) continue;
                        cloned = el.Clone();
                    }

                    return ParseJsonElementAsType(cloned, targetType);
                }

                return (false, null);
            }
            catch
            {
                return (false, null);
            }
        }

        private static (bool success, object? value) ParseJsonElementAsType(
            JsonElement el, Type targetType)
        {
            try
            {
                if (targetType == typeof(string))
                {
                    var s = el.ValueKind == JsonValueKind.String
                        ? el.GetString()
                        : el.GetRawText();
                    return (s != null, (object?)s);
                }
                if (targetType == typeof(int))     return (true, (object)el.GetInt32());
                if (targetType == typeof(long))    return (true, (object)el.GetInt64());
                if (targetType == typeof(short))   return (true, (object)(short)el.GetInt32());
                if (targetType == typeof(byte))    return (true, (object)(byte)el.GetInt32());
                if (targetType == typeof(float))   return (true, (object)(float)el.GetDouble());
                if (targetType == typeof(double))  return (true, (object)el.GetDouble());
                if (targetType == typeof(decimal)) return (true, (object)el.GetDecimal());
                if (targetType == typeof(bool))    return (true, (object)el.GetBoolean());
                if (targetType == typeof(DateTime))
                {
                    var raw = el.ValueKind == JsonValueKind.String ? el.GetString() : el.GetRawText();
                    if (raw != null && DateTime.TryParse(
                            raw,
                            System.Globalization.CultureInfo.InvariantCulture,
                            System.Globalization.DateTimeStyles.RoundtripKind,
                            out var dt))
                        return (true, (object)dt);
                    return (false, null);
                }
                return (false, null);
            }
            catch
            {
                return (false, null);
            }
        }

        // ── Nested complex-type deserialization ───────────────────────────────

        private object? DeserializeNested(
            MdixDatabase db, Type targetType, string prefix,
            HashSet<Type> typeStack, int depth)
        {
            var method = _deserializeMethod.MakeGenericMethod(targetType);
            var result = method.Invoke(this, new object?[] { db, prefix, typeStack, depth + 1 });
            if (result == null) return null;

            var resultType = result.GetType();
            var isSuccess  = (bool)resultType.GetProperty("IsSuccess")!.GetValue(result)!;
            if (!isSuccess) return null;

            return resultType.GetProperty("SuccessResult")!.GetValue(result);
        }

        // ── Serialization ─────────────────────────────────────────────────────

        internal MdixResult<Unit> Serialize<T>(
            T obj, MdixDataSectionBuilder data, string? prefix = null)
        {
            if (obj == null)
                return MdixError.NativeError("Cannot serialize a null object.");

            try
            {
                var typeInfo        = GetOrBuildTypeInfo(typeof(T));
                var effectivePrefix = prefix ?? typeInfo.ClassPrefix ?? string.Empty;

                var pairs = new List<(string path, object? value)>();
                // Reference-identity tracking, not type tracking: the same
                // *type* legitimately appearing twice in sibling positions
                // (Team.Captain and Team.ViceCaptain, two different Player
                // instances) is fine -- only the same *instance* recurring in
                // its own ancestor chain (a real runtime reference cycle,
                // e.g. child.Parent == parent while parent.Children contains
                // child, an entirely ordinary pattern) is the problem. See
                // Deserialize<T>'s matching guard for why this exists at all
                // -- same uncatchable-StackOverflowException risk, this side
                // just walks real instances instead of freshly building them.
                var visited = new HashSet<object>(IdentityComparer.Instance);
                CollectPairs(obj, typeInfo, effectivePrefix, pairs, visited, 0);

                var flat    = pairs.Where(p => !p.path.Contains('.')).ToList();
                var grouped = pairs.Where(p =>  p.path.Contains('.'))
                                   .GroupBy(p => p.path.Substring(0, p.path.LastIndexOf('.')))
                                   .ToList();

                foreach (var (path, value) in flat)
                    ApplyFlat(data, path, value);

                foreach (var group in grouped)
                {
                    var tablePath = group.Key;
                    data.WithTableProperties(tablePath, t =>
                    {
                        foreach (var (fullPath, value) in group)
                        {
                            var propName = fullPath.Substring(tablePath.Length + 1);
                            ApplyTable(t, propName, value);
                        }
                    });
                }

                return MdixResult<Unit>.Ok(Unit.Value);
            }
            catch (Exception ex)
            {
                return MdixError.NativeError($"Serialize<{typeof(T).Name}> failed: {ex.Message}");
            }
        }

        private void CollectPairs(
            object obj, TypeSerializationInfo typeInfo, string prefix,
            List<(string, object?)> pairs, HashSet<object> visited, int depth)
        {
            if (depth > MaxNestingDepth)
                throw new InvalidOperationException(
                    $"Serialize: nesting depth exceeded {MaxNestingDepth} levels at path " +
                    $"'{prefix}'. This almost always means a cyclic object graph (an instance " +
                    "that eventually refers back to itself through its own properties) -- mark " +
                    "the back-reference [MdixIgnore], or break the cycle before serializing.");

            if (!visited.Add(obj))
                throw new InvalidOperationException(
                    $"Serialize: reference cycle detected at path '{prefix}' -- this object " +
                    "already appears earlier in its own property chain. Mark the " +
                    "back-reference [MdixIgnore], or break the cycle before serializing.");

            try
            {
                foreach (var prop in typeInfo.Properties)
                {
                    if (prop.IsIgnored) continue;

                    var value    = prop.PropInfo.GetValue(obj);
                    var propPath = BuildFullPath(prefix, prop.Paths[0]);

                    if (IsComplexType(prop.PropInfo.PropertyType) && value != null)
                    {
                        var nested = GetOrBuildTypeInfo(prop.PropInfo.PropertyType);
                        CollectPairs(value, nested, propPath, pairs, visited, depth + 1);
                    }
                    else
                    {
                        pairs.Add((propPath, value));
                    }
                }
            }
            finally
            {
                // Only removes what *this* call added -- a sibling branch
                // reusing the same instance (not an ancestor cycle, e.g. two
                // different lists both holding a shared reference) is fine
                // and should be visitable again once this branch is done.
                visited.Remove(obj);
            }
        }

        private static void ApplyFlat(MdixDataSectionBuilder data, string path, object? value)
        {
            switch (value)
            {
                case string  s:  data.WithString(path, s);    break;
                case int     i:  data.WithInt(path, i);       break;
                // FIX: use WithLong to preserve 64-bit precision (was WithInt+(int)l).
                case long    l:  data.WithLong(path, l);      break;
                case float   f:  data.WithFloat(path, f);     break;
                case double  d:  data.WithDouble(path, d);    break;
                case bool    b:  data.WithBool(path, b);      break;
                case short   s:  data.WithInt(path, s);       break;
                case byte    by: data.WithInt(path, by);      break;
                case decimal dc: data.WithDouble(path, (double)dc); break;
                case DateTime dt:
                    if (dt.TimeOfDay == TimeSpan.Zero)
                        data.WithDate(path, dt);
                    else
                        data.WithTimestamp(path, dt);
                    break;
                case null: break;
                default:   data.WithString(path, value.ToString() ?? string.Empty); break;
            }
        }

        private static void ApplyTable(MdixTablePropertiesBuilder t, string name, object? value)
        {
            switch (value)
            {
                case string  s:  t.WithString(name, s);    break;
                case int     i:  t.WithInt(name, i);       break;
                // FIX: use WithLong to preserve 64-bit precision (was WithInt+(int)l).
                case long    l:  t.WithLong(name, l);      break;
                case float   f:  t.WithFloat(name, f);     break;
                case double  d:  t.WithDouble(name, d);    break;
                case bool    b:  t.WithBool(name, b);      break;
                case short   s:  t.WithInt(name, s);       break;
                case byte    by: t.WithInt(name, by);      break;
                case decimal dc: t.WithDouble(name, (double)dc); break;
                case DateTime dt:
                    if (dt.TimeOfDay == TimeSpan.Zero)
                        t.WithDate(name, dt);
                    else
                        t.WithTimestamp(name, dt);
                    break;
                case null: break;
                default:   t.WithString(name, value.ToString() ?? string.Empty); break;
            }
        }

        // ── Type info cache ───────────────────────────────────────────────────

        private TypeSerializationInfo GetOrBuildTypeInfo(Type type)
        {
            lock (_cacheLock)
            {
                if (_cache.TryGetValue(type, out var cached)) return cached;
                var info = BuildTypeInfo(type);
                _cache[type] = info;
                return info;
            }
        }

        private TypeSerializationInfo BuildTypeInfo(Type type)
        {
            var info = new TypeSerializationInfo
            {
                Type        = type,
                ClassPrefix = type.GetCustomAttribute<MdixObjectAttribute>()?.Prefix,
                Properties  = new List<PropertySerializationInfo>(),
                CtorParams  = new List<CtorParamInfo>(),
            };

            foreach (var prop in type.GetProperties(BindingFlags.Public | BindingFlags.Instance))
            {
                info.Properties.Add(new PropertySerializationInfo
                {
                    PropInfo     = prop,
                    Paths        = BuildPropertyPaths(prop),
                    IsIgnored    = prop.GetCustomAttribute<MdixIgnoreAttribute>()   != null,
                    IsRequired   = prop.GetCustomAttribute<MdixRequiredAttribute>() != null,
                    DefaultValue = prop.GetCustomAttribute<MdixDefaultValueAttribute>()?.DefaultValue,
                    Transform    = prop.GetCustomAttribute<MdixTransformAttribute>()?.Transform,
                    Validator    = prop.GetCustomAttribute<MdixValidationAttribute>()?.Validator,
                });
            }

            var ctors      = type.GetConstructors(BindingFlags.Public | BindingFlags.Instance);
            var nonDefault = ctors.Where(c => c.GetParameters().Length > 0).ToArray();

            if (nonDefault.Length > 0)
            {
                info.PrimaryConstructor =
                    nonDefault.FirstOrDefault(c => c.GetCustomAttribute<MdixConstructorAttribute>() != null)
                    ?? nonDefault.OrderByDescending(c => c.GetParameters().Length).First();

                foreach (var param in info.PrimaryConstructor.GetParameters())
                {
                    info.CtorParams.Add(new CtorParamInfo
                    {
                        Param        = param,
                        Paths        = BuildCtorParamPaths(param, info.Properties),
                        IsRequired   = !param.HasDefaultValue
                                       && param.GetCustomAttribute<MdixRequiredAttribute>() != null,
                        DefaultValue = param.HasDefaultValue ? param.DefaultValue : null,
                    });
                }
            }

            return info;
        }

        private static List<string> BuildPropertyPaths(PropertyInfo prop)
        {
            var paths = new List<string>();
            var attr  = prop.GetCustomAttribute<MdixPropertyAttribute>();
            paths.Add(attr?.Path ?? ToSnakeCase(prop.Name));
            paths.AddRange(prop.GetCustomAttributes<MdixAliasAttribute>().Select(a => a.AliasPath));
            return paths;
        }

        private static List<string> BuildCtorParamPaths(
            ParameterInfo param, List<PropertySerializationInfo> props)
        {
            var attr = param.GetCustomAttribute<MdixPropertyAttribute>();
            if (attr != null) return new List<string> { attr.Path };

            var match = props.FirstOrDefault(p =>
                string.Equals(p.PropInfo.Name, param.Name, StringComparison.OrdinalIgnoreCase));

            return match != null
                ? new List<string>(match.Paths)
                : new List<string> { ToSnakeCase(param.Name ?? string.Empty) };
        }

        // ── Helpers ───────────────────────────────────────────────────────────

        /// <summary>
        /// Reference-identity comparer for Serialize's cycle-detection set.
        /// Hand-rolled rather than System.Collections.Generic.ReferenceEqualityComparer
        /// -- that type was only added in .NET 5, and this project targets
        /// netstandard2.1. RuntimeHelpers.GetHashCode always gives an
        /// identity-based hash regardless of whether the object overrides
        /// GetHashCode/Equals itself, which is exactly what's needed here
        /// (some POCOs may reasonably define value-style equality that would
        /// otherwise make two genuinely-different instances look identical to
        /// this set).
        /// </summary>
        private sealed class IdentityComparer : IEqualityComparer<object>
        {
            internal static readonly IdentityComparer Instance = new();
            private IdentityComparer() { }

            public new bool Equals(object? x, object? y) => ReferenceEquals(x, y);
            public int GetHashCode(object obj) => RuntimeHelpers.GetHashCode(obj);
        }

        private static string BuildFullPath(string prefix, string path)
        {
            if (string.IsNullOrEmpty(prefix)) return path;
            if (string.IsNullOrEmpty(path))   return prefix;
            return $"{prefix}.{path}";
        }

        private static string ToSnakeCase(string name)
        {
            if (string.IsNullOrEmpty(name)) return name;
            var sb = new StringBuilder(name.Length + 4);
            for (int i = 0; i < name.Length; i++)
            {
                if (i > 0 && char.IsUpper(name[i])) sb.Append('_');
                sb.Append(char.ToLowerInvariant(name[i]));
            }
            return sb.ToString();
        }

        private static readonly HashSet<Type> _simpleTypes = new HashSet<Type>
        {
            typeof(string),
            typeof(int),     typeof(long),    typeof(short),  typeof(byte),
            typeof(float),   typeof(double),  typeof(decimal),
            typeof(bool),
            typeof(DateTime), typeof(DateTimeOffset),
            typeof(MdixHexColor), typeof(MdixBlob), typeof(MdixRegex),
            typeof(MdixDate),     typeof(MdixTimestamp),
        };

        private static bool IsComplexType(Type t)
        {
            if (_simpleTypes.Contains(t))   return false;
            if (t.IsPrimitive)              return false;
            if (t.IsEnum)                   return false;
            if (typeof(IEnumerable).IsAssignableFrom(t)) return false;

            var underlying = Nullable.GetUnderlyingType(t);
            if (underlying != null &&
                (_simpleTypes.Contains(underlying) || underlying.IsPrimitive || underlying.IsEnum))
                return false;

            return t.IsClass || t.IsValueType;
        }

        private static bool HasParameterlessCtor(Type t) =>
            t.GetConstructor(
                BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
                null, Type.EmptyTypes, null) != null;

        private static object? DefaultOf(Type t) =>
            t.IsValueType ? Activator.CreateInstance(t) : null;

        public static void ClearCache()
        {
            lock (_cacheLock) _cache.Clear();
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Internal data containers
    // ══════════════════════════════════════════════════════════════════════════

    internal sealed class TypeSerializationInfo
    {
        internal Type                            Type               { get; set; } = null!;
        internal string?                         ClassPrefix        { get; set; }
        internal List<PropertySerializationInfo> Properties         { get; set; } = new();
        internal ConstructorInfo?                PrimaryConstructor { get; set; }
        internal List<CtorParamInfo>             CtorParams         { get; set; } = new();
    }

    internal sealed class PropertySerializationInfo
    {
        internal PropertyInfo          PropInfo     { get; set; } = null!;
        internal List<string>          Paths        { get; set; } = new();
        internal bool                  IsIgnored    { get; set; }
        internal bool                  IsRequired   { get; set; }
        internal object?               DefaultValue { get; set; }
        internal Func<object, object>? Transform    { get; set; }
        internal Func<object, bool>?   Validator    { get; set; }
    }

    internal sealed class CtorParamInfo
    {
        internal ParameterInfo Param        { get; set; } = null!;
        internal List<string>  Paths        { get; set; } = new();
        internal bool          IsRequired   { get; set; }
        internal object?       DefaultValue { get; set; }
    }
}
