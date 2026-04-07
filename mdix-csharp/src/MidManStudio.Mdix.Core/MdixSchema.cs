using System;
using System.Collections.Generic;

namespace MidManStudio.Mdix.Core
{
    #region Error Kind

    /// <summary>Classifies a single schema validation failure.</summary>
    public enum MdixValidationErrorKind
    {
        /// <summary>A required field was not present in the data.</summary>
        Missing,

        /// <summary>The field exists but its type does not match the expectation.</summary>
        WrongType,

        /// <summary>The field exists and has the right type, but a custom validator rejected the value.</summary>
        InvalidValue,
    }

    #endregion

    #region MdixValidationError

    /// <summary>Describes one schema mismatch found during validation.</summary>
    public sealed class MdixValidationError
    {
        /// <summary>The dotted path that failed validation.</summary>
        public string Path { get; }

        /// <summary>Human-readable description of what was expected.</summary>
        public string Expected { get; }

        /// <summary>Human-readable description of what was found.</summary>
        public string Actual { get; }

        /// <summary>Category of the failure.</summary>
        public MdixValidationErrorKind Kind { get; }

        public MdixValidationError(
            string                  path,
            string                  expected,
            string                  actual,
            MdixValidationErrorKind kind)
        {
            Path     = path;
            Expected = expected;
            Actual   = actual;
            Kind     = kind;
        }

        public override string ToString() =>
            $"[{Kind}] '{Path}': expected {Expected}, got {Actual}";
    }

    #endregion

    #region MdixValidationReport

    /// <summary>
    /// Result of a schema validation pass.
    /// <see cref="IsValid"/> is <c>true</c> only when <see cref="Errors"/> is empty.
    /// </summary>
    public sealed class MdixValidationReport
    {
        /// <summary>True when no errors were found.</summary>
        public bool IsValid => Errors.Count == 0;

        /// <summary>All validation errors collected during this pass.</summary>
        public IReadOnlyList<MdixValidationError> Errors { get; }

        internal MdixValidationReport(IReadOnlyList<MdixValidationError> errors)
        {
            Errors = errors ?? Array.Empty<MdixValidationError>();
        }

        public override string ToString() =>
            IsValid
                ? "Validation passed."
                : $"Validation failed with {Errors.Count} error(s):\n" +
                  string.Join("\n", Errors);
    }

    #endregion

    #region MdixSchemaField

    /// <summary>
    /// Describes one expected field in a DixScript schema.
    /// Build instances via <see cref="MdixSchemaBuilder"/>.
    /// </summary>
    public sealed class MdixSchemaField
    {
        /// <summary>The dotted path of the expected field (e.g. <c>"server.port"</c>).</summary>
        public string Path { get; }

        /// <summary>The CLR type the value must satisfy (e.g. <c>typeof(int)</c>).</summary>
        public Type ExpectedType { get; }

        /// <summary>When true, absence of the field is a <see cref="MdixValidationErrorKind.Missing"/> error.</summary>
        public bool IsRequired { get; }

        /// <summary>
        /// Optional extra validator run after the type check passes.
        /// Return <c>Err</c> to add a <see cref="MdixValidationErrorKind.InvalidValue"/> error.
        /// </summary>
        public Func<MdixDatabase, MdixResult<Unit>>? CustomValidator { get; }

        internal MdixSchemaField(
            string                              path,
            Type                                expectedType,
            bool                                isRequired,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null)
        {
            Path             = path;
            ExpectedType     = expectedType;
            IsRequired       = isRequired;
            CustomValidator  = customValidator;
        }
    }

    #endregion

    #region IMdixSchemaSource

    /// <summary>
    /// Provides the expected field definitions for a schema validation pass.
    /// Implement this interface to plug in code-generated or format-driven schemas.
    /// When <c>@SCHEMA</c> lands in the DixScript format, add
    /// <c>MdixFormatSchema : IMdixSchemaSource</c> — zero changes to the validator.
    /// </summary>
    public interface IMdixSchemaSource
    {
        /// <summary>Returns the expected fields for this schema.</summary>
        IEnumerable<MdixSchemaField> GetExpectedFields();
    }

    #endregion

    #region MdixSchemaBuilder

    /// <summary>
    /// Fluent builder for constructing an <see cref="IMdixSchemaSource"/> inline.
    /// </summary>
    /// <example>
    /// <code>
    /// var schema = new MdixSchemaBuilder()
    ///     .Require&lt;int&gt;("server.port",
    ///         db => db.GetInt("server.port").AndThen(p =>
    ///             p is > 1024 and &lt; 65536
    ///                 ? MdixResult&lt;Unit&gt;.Ok(Unit.Value)
    ///                 : MdixError.NativeError("Port out of range.")))
    ///     .Optional&lt;string&gt;("server.host");
    ///
    /// var report = db.Validate(schema);
    /// </code>
    /// </example>
    public sealed class MdixSchemaBuilder : IMdixSchemaSource
    {
        private readonly List<MdixSchemaField> _fields = new List<MdixSchemaField>();

        // ── Required fields ───────────────────────────────────────────────────

        /// <summary>Adds a required field at <paramref name="path"/> of CLR type <typeparamref name="T"/>.</summary>
        public MdixSchemaBuilder Require<T>(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null)
        {
            _fields.Add(new MdixSchemaField(path, typeof(T), isRequired: true, customValidator));
            return this;
        }

        /// <summary>Adds a required string field.</summary>
        public MdixSchemaBuilder RequireString(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<string>(path, customValidator);

        /// <summary>Adds a required int field.</summary>
        public MdixSchemaBuilder RequireInt(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<int>(path, customValidator);

        /// <summary>Adds a required float field.</summary>
        public MdixSchemaBuilder RequireFloat(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<float>(path, customValidator);

        /// <summary>Adds a required double field.</summary>
        public MdixSchemaBuilder RequireDouble(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<double>(path, customValidator);

        /// <summary>Adds a required bool field.</summary>
        public MdixSchemaBuilder RequireBool(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<bool>(path, customValidator);

        // ── Optional fields ───────────────────────────────────────────────────

        /// <summary>Adds an optional field at <paramref name="path"/> of CLR type <typeparamref name="T"/>.</summary>
        public MdixSchemaBuilder Optional<T>(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null)
        {
            _fields.Add(new MdixSchemaField(path, typeof(T), isRequired: false, customValidator));
            return this;
        }

        /// <summary>Adds an optional string field.</summary>
        public MdixSchemaBuilder OptionalString(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<string>(path, customValidator);

        /// <summary>Adds an optional int field.</summary>
        public MdixSchemaBuilder OptionalInt(
            string                              path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<int>(path, customValidator);

        // ── IMdixSchemaSource ─────────────────────────────────────────────────

        /// <inheritdoc/>
        public IEnumerable<MdixSchemaField> GetExpectedFields() => _fields;
    }

    #endregion

    #region MdixDatabaseValidator

    /// <summary>
    /// Validates a <see cref="MdixDatabase"/> against an <see cref="IMdixSchemaSource"/>.
    /// Called by <see cref="MdixDatabase.Validate"/>.
    /// </summary>
    internal static class MdixDatabaseValidator
    {
        /// <summary>
        /// Runs a full validation pass and returns a <see cref="MdixValidationReport"/>
        /// containing all errors found. The report is always returned — never throws.
        /// </summary>
        internal static MdixValidationReport Validate(
            MdixDatabase    db,
            IMdixSchemaSource schema)
        {
            var errors = new List<MdixValidationError>();

            foreach (var field in schema.GetExpectedFields())
            {
                var exists = db.Exists(field.Path);

                // 1. Required presence check.
                if (!exists)
                {
                    if (field.IsRequired)
                    {
                        errors.Add(new MdixValidationError(
                            field.Path,
                            expected: $"{field.ExpectedType.Name} (required)",
                            actual:   "missing",
                            kind:     MdixValidationErrorKind.Missing));
                    }
                    // Optional and absent — skip remaining checks.
                    continue;
                }

                // 2. Type check.
                var actualType = db.GetValueType(field.Path);
                if (!TypeMatches(field.ExpectedType, actualType))
                {
                    errors.Add(new MdixValidationError(
                        field.Path,
                        expected: field.ExpectedType.Name,
                        actual:   actualType.ToString(),
                        kind:     MdixValidationErrorKind.WrongType));
                    // Skip custom validator when type is already wrong.
                    continue;
                }

                // 3. Custom validator (optional).
                if (field.CustomValidator != null)
                {
                    MdixResult<Unit> customResult;
                    try   { customResult = field.CustomValidator(db); }
                    catch (Exception ex)
                    {
                        errors.Add(new MdixValidationError(
                            field.Path,
                            expected: "custom validation to pass",
                            actual:   $"exception: {ex.Message}",
                            kind:     MdixValidationErrorKind.InvalidValue));
                        continue;
                    }

                    if (customResult.IsFailure)
                    {
                        errors.Add(new MdixValidationError(
                            field.Path,
                            expected: "custom validation to pass",
                            actual:   customResult.Error.Message,
                            kind:     MdixValidationErrorKind.InvalidValue));
                    }
                }
            }

            return new MdixValidationReport(errors);
        }

        /// <summary>
        /// Maps a CLR type to the set of <see cref="MdixValueType"/> values it satisfies.
        /// </summary>
        private static bool TypeMatches(Type clrType, MdixValueType actual)
        {
            if (clrType == typeof(string))
                return actual == MdixValueType.String
                    || actual == MdixValueType.Date
                    || actual == MdixValueType.Timestamp
                    || actual == MdixValueType.HexColor
                    || actual == MdixValueType.Blob
                    || actual == MdixValueType.Regex;

            if (clrType == typeof(int))    return actual == MdixValueType.Int || actual == MdixValueType.Enum;
            if (clrType == typeof(float))  return actual == MdixValueType.Float;
            if (clrType == typeof(double)) return actual == MdixValueType.Double;
            if (clrType == typeof(bool))   return actual == MdixValueType.Bool;

            if (clrType == typeof(MdixHexColor))  return actual == MdixValueType.HexColor;
            if (clrType == typeof(MdixBlob))      return actual == MdixValueType.Blob;
            if (clrType == typeof(MdixRegex))     return actual == MdixValueType.Regex;
            if (clrType == typeof(MdixDate))      return actual == MdixValueType.Date;
            if (clrType == typeof(MdixTimestamp)) return actual == MdixValueType.Timestamp;

            // Fallback — unknown CLR type, allow through.
            return true;
        }
    }

    #endregion
}
