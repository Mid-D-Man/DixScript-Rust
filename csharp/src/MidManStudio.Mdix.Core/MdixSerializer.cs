using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Text;

namespace MidManStudio.Mdix.Core
{
    // ══════════════════════════════════════════════════════════════════════════
    // Public attributes — decorate user types with these
    // ══════════════════════════════════════════════════════════════════════════

    /// <summary>
    /// Declares the root path prefix for a class or struct when deserializing
    /// from or serializing to a DixScript database.
    /// A prefix passed directly to <see cref="MdixDatabase.Deserialize{T}"/> overrides this.
    /// </summary>
    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct)]
    public sealed class MdixObjectAttribute : Attribute
    {
        public string? Prefix { get; }
        public MdixObjectAttribute(string? prefix = null) => Prefix = prefix;
    }

    /// <summary>
    /// Maps a property or constructor parameter to an explicit DixScript path.
    /// Without this attribute the property name is converted to snake_case automatically.
    /// </summary>
    [AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
    public sealed class MdixPropertyAttribute : Attribute
    {
        public string Path { get; }
        public MdixPropertyAttribute(string path) => Path = path;
    }

    /// <summary>
    /// Provides one or more fallback paths tried in order after the primary path fails.
    /// Useful for backward-compatible renames. May be applied multiple times.
    /// </summary>
    [AttributeUsage(AttributeTargets.Property, AllowMultiple = true)]
    public sealed class MdixAliasAttribute : Attribute
    {
        public string AliasPath { get; }
        public MdixAliasAttribute(string aliasPath) => AliasPath = aliasPath;
    }

    /// <summary>Skips this property during both deserialization and serialization.</summary>
    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixIgnoreAttribute : Attribute { }

    /// <summary>
    /// Marks a property or constructor parameter as required.
    /// Deserialization fails with a descriptive error if the path is absent.
    /// </summary>
    [AttributeUsage(AttributeTargets.Property | AttributeTargets.Parameter)]
    public sealed class MdixRequiredAttribute : Attribute { }

    /// <summary>
    /// Provides a compile-time constant fallback value when the path is absent.
    /// The value must be assignable to the property type.
    /// </summary>
    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixDefaultValueAttribute : Attribute
    {
        public object? DefaultValue { get; }
        public MdixDefaultValueAttribute(object? defaultValue) => DefaultValue = defaultValue;
    }

    /// <summary>
    /// Applies a static transformation method after the value is read from the database.
    /// The method must be <c>public static object MethodName(object value)</c>.
    /// </summary>
    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixTransformAttribute : Attribute
    {
        public Func<object, object>? Transform { get; }

        public MdixTransformAttribute(Type transformerType, string methodName)
        {
            var m = transformerType.GetMethod(methodName, BindingFlags.Public | BindingFlags.Static);
            if (m != null)
                Transform = obj => m.Invoke(null, new[] { obj })!;
        }
    }

    /// <summary>
    /// Runs a static validation method after the value is read and transformed.
    /// Deserialization fails if the method returns false.
    /// The method must be <c>public static bool MethodName(object value)</c>.
    /// </summary>
    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixValidationAttribute : Attribute
    {
        public Func<object, bool>? Validator { get; }

        public MdixValidationAttribute(Type validatorType, string methodName)
        {
            var m = validatorType.GetMethod(methodName, BindingFlags.Public | BindingFlags.Static);
            if (m != null)
                Validator = obj => (bool)m.Invoke(null, new[] { obj })!;
        }
    }

    /// <summary>
    /// Marks a specific constructor as the preferred one for deserialization.
    /// Without this, the constructor with the most parameters is chosen automatically.
    /// </summary>
    [AttributeUsage(AttributeTargets.Constructor)]
    public sealed class MdixConstructorAttribute : Attribute { }

    /// <summary>Controls how type conversion is attempted during deserialization.</summary>
    public enum MdixConversionMode
    {
        /// <summary>Exact type match only. Fails on any type mismatch.</summary>
        Strict,
        /// <summary>Attempts <see cref="Convert.ChangeType"/> on mismatch. Default.</summary>
        Safe,
        /// <summary>Attempts all conversion strategies including string parsing.</summary>
        Forced,
    }

    /// <summary>Overrides the default conversion mode for a single property.</summary>
    [AttributeUsage(AttributeTargets.Property)]
    public sealed class MdixConvertAttribute : Attribute
    {
        public MdixConversionMode Mode { get; }
        public MdixConvertAttribute(MdixConversionMode mode = MdixConversionMode.Safe) => Mode = mode;
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Serializer — internal, called via MdixDatabase.Deserialize<T> and
    //              MdixBuilder.Serialize<T>
    // ══════════════════════════════════════════════════════════════════════════

    internal sealed class MdixSerializer
    {
        // Reflection method handles cached once per AppDomain lifetime.
        private static readonly MethodInfo _deserializeMethod =
            typeof(MdixSerializer)
                .GetMethods(BindingFlags.Instance | BindingFlags.NonPublic)
                .First(m => m.Name == nameof(Deserialize) && m.IsGenericMethodDefinition);

        private static readonly MethodInfo _dbGetMethod =
            typeof(MdixDatabase).GetMethod(nameof(MdixDatabase.Get))!;

        private static readonly Dictionary<Type, TypeSerializationInfo> _cache = new();
        private static readonly object _cacheLock = new();

        // ── Deserialization ───────────────────────────────────────────────────

        internal MdixResult<T> Deserialize<T>(MdixDatabase db, string? prefix = null)
        {
            try
            {
                var typeInfo = GetOrBuildTypeInfo(typeof(T));
                var effectivePrefix = prefix ?? typeInfo.ClassPrefix ?? string.Empty;

                if (typeInfo.PrimaryConstructor != null)
                    return DeserializeViaCtor<T>(db, typeInfo, effectivePrefix);

                if (!typeof(T).IsValueType && !HasParameterlessCtor(typeof(T)))
                    return MdixError.NativeError(
                        $"'{typeof(T).Name}' needs a parameterless constructor or a constructor " +
                        $"whose parameters are mappable via [MdixProperty].");

                var instance = Activator.CreateInstance<T>();
                object boxed = instance!;

                var err = FillProperties(db, typeInfo, effectivePrefix, ref boxed, null);
                if (err.HasValue) return MdixResult<T>.Err(err.Value);

                return MdixResult<T>.Ok((T)boxed);
            }
            catch (Exception ex)
            {
                return MdixError.NativeError($"Deserialize<{typeof(T).Name}> failed: {ex.Message}");
            }
        }

        private MdixResult<T> DeserializeViaCtor<T>(
            MdixDatabase db,
            TypeSerializationInfo typeInfo,
            string prefix)
        {
            var ctor = typeInfo.PrimaryConstructor!;
            var parameters = ctor.GetParameters();
            var values = new object?[parameters.Length];

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

                var (found, value) = TryResolvePaths(db, param.ParameterType, pInfo.Paths, prefix);
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
            try
            {
                instance = (T)ctor.Invoke(values);
            }
            catch (Exception ex)
            {
                return MdixError.NativeError(
                    $"Constructor invocation failed for '{typeof(T).Name}': {ex.Message}");
            }

            // Fill any settable properties not already covered by the constructor.
            var ctorNames = new HashSet<string>(
                typeInfo.CtorParams.Select(p => p.Param.Name ?? string.Empty),
                StringComparer.OrdinalIgnoreCase);

            object boxed = instance!;
            var err = FillProperties(db, typeInfo, prefix, ref boxed, ctorNames);
            if (err.HasValue) return MdixResult<T>.Err(err.Value);

            return MdixResult<T>.Ok((T)boxed);
        }

        private MdixError? FillProperties(
            MdixDatabase db,
            TypeSerializationInfo typeInfo,
            string prefix,
            ref object boxed,
            HashSet<string>? skipNames)
        {
            foreach (var prop in typeInfo.Properties)
            {
                if (prop.IsIgnored) continue;
                if (!prop.PropInfo.CanWrite) continue;
                if (skipNames != null && skipNames.Contains(prop.PropInfo.Name)) continue;

                var (found, value) = TryResolvePaths(db, prop.PropInfo.PropertyType, prop.Paths, prefix);

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

        private (bool found, object? value) TryResolvePaths(
            MdixDatabase db,
            Type targetType,
            List<string> paths,
            string prefix)
        {
            foreach (var rawPath in paths)
            {
                var fullPath = BuildFullPath(prefix, rawPath);

                if (IsComplexType(targetType))
                {
                    var nested = DeserializeNested(db, targetType, fullPath);
                    if (nested != null) return (true, nested);
                    continue;
                }

                var convMode = MdixConversionMode.Safe;
                var (success, val) = InvokeGet(db, targetType, fullPath, convMode);
                if (success) return (true, val);
            }

            return (false, null);
        }

        private object? DeserializeNested(MdixDatabase db, Type targetType, string prefix)
        {
            var method = _deserializeMethod.MakeGenericMethod(targetType);
            var result = method.Invoke(this, new object?[] { db, prefix });
            if (result == null) return null;

            var resultType = result.GetType();
            var isSuccess = (bool)resultType.GetProperty("IsSuccess")!.GetValue(result)!;
            if (!isSuccess) return null;

            return resultType.GetProperty("SuccessResult")!.GetValue(result);
        }

        private static (bool success, object? value) InvokeGet(
            MdixDatabase db,
            Type targetType,
            string path,
            MdixConversionMode mode)
        {
            try
            {
                var method = _dbGetMethod.MakeGenericMethod(targetType);
                var result = method.Invoke(db, new object[] { path });
                if (result == null) return (false, null);

                var resultType = result.GetType();
                var isSuccess = (bool)resultType.GetProperty("IsSuccess")!.GetValue(result)!;
                if (!isSuccess) return (false, null);

                var value = resultType.GetProperty("SuccessResult")!.GetValue(result);
                return (true, value);
            }
            catch
            {
                return (false, null);
            }
        }

        // ── Serialization ─────────────────────────────────────────────────────

        internal MdixResult<Unit> Serialize<T>(
            T obj,
            MdixDataSectionBuilder data,
            string? prefix = null)
        {
            if (obj == null)
                return MdixError.NativeError("Cannot serialize a null object.");

            try
            {
                var typeInfo = GetOrBuildTypeInfo(typeof(T));
                var effectivePrefix = prefix ?? typeInfo.ClassPrefix ?? string.Empty;

                var pairs = new List<(string path, object? value)>();
                CollectPairs(obj, typeInfo, effectivePrefix, pairs);

                // Two-tier ordering: no-dot paths (flat) first, then grouped by table path.
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
            object obj,
            TypeSerializationInfo typeInfo,
            string prefix,
            List<(string, object?)> pairs)
        {
            foreach (var prop in typeInfo.Properties)
            {
                if (prop.IsIgnored) continue;

                var value    = prop.PropInfo.GetValue(obj);
                var propPath = BuildFullPath(prefix, prop.Paths[0]);

                if (IsComplexType(prop.PropInfo.PropertyType) && value != null)
                {
                    var nested = GetOrBuildTypeInfo(prop.PropInfo.PropertyType);
                    CollectPairs(value, nested, propPath, pairs);
                }
                else
                {
                    pairs.Add((propPath, value));
                }
            }
        }

        private static void ApplyFlat(MdixDataSectionBuilder data, string path, object? value)
        {
            switch (value)
            {
                case string  s:  data.WithString(path, s);    break;
                case int     i:  data.WithInt(path, i);       break;
                case float   f:  data.WithFloat(path, f);     break;
                case double  d:  data.WithDouble(path, d);    break;
                case bool    b:  data.WithBool(path, b);      break;
                case long    l:  data.WithInt(path, (int)l);  break;
                case short   s:  data.WithInt(path, s);       break;
                case byte    by: data.WithInt(path, by);      break;
                case decimal dc: data.WithDouble(path, (double)dc); break;
                case DateTime dt:
                    if (dt.TimeOfDay == TimeSpan.Zero)
                        data.WithDate(path, dt);
                    else
                        data.WithTimestamp(path, dt);
                    break;
                case null: break; // omit nulls
                default:   data.WithString(path, value.ToString() ?? string.Empty); break;
            }
        }

        private static void ApplyTable(MdixTablePropertiesBuilder t, string name, object? value)
        {
            switch (value)
            {
                case string  s:  t.WithString(name, s);    break;
                case int     i:  t.WithInt(name, i);       break;
                case float   f:  t.WithFloat(name, f);     break;
                case double  d:  t.WithDouble(name, d);    break;
                case bool    b:  t.WithBool(name, b);      break;
                case long    l:  t.WithInt(name, (int)l);  break;
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
                if (_cache.TryGetValue(type, out var cached))
                    return cached;

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

            // Constructor selection: explicit [MdixConstructor] > most-params > none.
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
            ParameterInfo param,
            List<PropertySerializationInfo> props)
        {
            var attr = param.GetCustomAttribute<MdixPropertyAttribute>();
            if (attr != null)
                return new List<string> { attr.Path };

            var match = props.FirstOrDefault(p =>
                string.Equals(p.PropInfo.Name, param.Name, StringComparison.OrdinalIgnoreCase));

            return match != null
                ? new List<string>(match.Paths)
                : new List<string> { ToSnakeCase(param.Name ?? string.Empty) };
        }

        // ── Helpers ───────────────────────────────────────────────────────────

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
                if (i > 0 && char.IsUpper(name[i]))
                    sb.Append('_');
                sb.Append(char.ToLowerInvariant(name[i]));
            }
            return sb.ToString();
        }

        // Types that MdixDatabase.Get<T> handles natively — do NOT recurse into these.
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

        /// <summary>
        /// Clears the reflection cache. Call after hot-reload or dynamic assembly loading.
        /// </summary>
        public static void ClearCache()
        {
            lock (_cacheLock)
                _cache.Clear();
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Internal data containers
    // ══════════════════════════════════════════════════════════════════════════

    internal sealed class TypeSerializationInfo
    {
        internal Type                         Type              { get; set; } = null!;
        internal string?                      ClassPrefix       { get; set; }
        internal List<PropertySerializationInfo> Properties     { get; set; } = new();
        internal ConstructorInfo?             PrimaryConstructor{ get; set; }
        internal List<CtorParamInfo>          CtorParams        { get; set; } = new();
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
