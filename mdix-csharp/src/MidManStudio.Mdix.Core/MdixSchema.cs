// mdix-csharp/src/MidManStudio.Mdix.Core/MdixSchema.cs
using System;
using System.Collections.Generic;

namespace MidManStudio.Mdix.Core
{
    #region Error Kind

    public enum MdixValidationErrorKind
    {
        Missing,
        WrongType,
        InvalidValue,
    }

    #endregion

    #region MdixValidationError

    public sealed class MdixValidationError
    {
        public string Path { get; }
        public string Expected { get; }
        public string Actual { get; }
        public MdixValidationErrorKind Kind { get; }

        public MdixValidationError(
            string path, string expected, string actual, MdixValidationErrorKind kind)
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

    public sealed class MdixValidationReport
    {
        public bool IsValid => Errors.Count == 0;
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

    public sealed class MdixSchemaField
    {
        public string Path { get; }
        public Type ExpectedType { get; }
        public bool IsRequired { get; }
        public Func<MdixDatabase, MdixResult<Unit>>? CustomValidator { get; }

        internal MdixSchemaField(
            string path,
            Type   expectedType,
            bool   isRequired,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null)
        {
            Path            = path;
            ExpectedType    = expectedType;
            IsRequired      = isRequired;
            CustomValidator = customValidator;
        }
    }

    #endregion

    #region IMdixSchemaSource

    public interface IMdixSchemaSource
    {
        IEnumerable<MdixSchemaField> GetExpectedFields();
    }

    #endregion

    #region MdixSchemaBuilder

    /// <summary>
    /// Fluent builder for constructing an <see cref="IMdixSchemaSource"/> inline.
    /// Supports all scalar types including <c>long</c> for 64-bit integer fields.
    /// </summary>
    public sealed class MdixSchemaBuilder : IMdixSchemaSource
    {
        private readonly List<MdixSchemaField> _fields = new List<MdixSchemaField>();

        // ── Required fields ───────────────────────────────────────────────────

        public MdixSchemaBuilder Require<T>(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null)
        {
            _fields.Add(new MdixSchemaField(path, typeof(T), isRequired: true, customValidator));
            return this;
        }

        public MdixSchemaBuilder RequireString(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<string>(path, customValidator);

        public MdixSchemaBuilder RequireInt(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<int>(path, customValidator);

        /// <summary>
        /// Adds a required 64-bit integer field.
        /// Accepts both <see cref="MdixValueType.Long"/> and <see cref="MdixValueType.Int"/>
        /// values (Int widens to long without loss).
        /// </summary>
        public MdixSchemaBuilder RequireLong(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<long>(path, customValidator);

        public MdixSchemaBuilder RequireFloat(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<float>(path, customValidator);

        public MdixSchemaBuilder RequireDouble(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<double>(path, customValidator);

        public MdixSchemaBuilder RequireBool(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Require<bool>(path, customValidator);

        public MdixSchemaBuilder RequireWith<T>(
            string path,
            Func<MdixDatabase, MdixResult<Unit>> validator) =>
            Require<T>(path, validator);

        // ── Optional fields ───────────────────────────────────────────────────

        public MdixSchemaBuilder Optional<T>(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null)
        {
            _fields.Add(new MdixSchemaField(path, typeof(T), isRequired: false, customValidator));
            return this;
        }

        public MdixSchemaBuilder OptionalString(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<string>(path, customValidator);

        public MdixSchemaBuilder OptionalInt(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<int>(path, customValidator);

        /// <summary>
        /// Adds an optional 64-bit integer field.
        /// Accepts both <see cref="MdixValueType.Long"/> and <see cref="MdixValueType.Int"/> values.
        /// </summary>
        public MdixSchemaBuilder OptionalLong(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<long>(path, customValidator);

        public MdixSchemaBuilder OptionalFloat(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<float>(path, customValidator);

        public MdixSchemaBuilder OptionalDouble(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<double>(path, customValidator);

        public MdixSchemaBuilder OptionalBool(
            string path,
            Func<MdixDatabase, MdixResult<Unit>>? customValidator = null) =>
            Optional<bool>(path, customValidator);

        public MdixSchemaBuilder OptionalWith<T>(
            string path,
            Func<MdixDatabase, MdixResult<Unit>> validator) =>
            Optional<T>(path, validator);

        // ── IMdixSchemaSource ─────────────────────────────────────────────────

        public IEnumerable<MdixSchemaField> GetExpectedFields() => _fields;

        // ── Inspection ────────────────────────────────────────────────────────

        public int FieldCount => _fields.Count;
        public IEnumerable<string> Paths => _fields.ConvertAll(f => f.Path);

        // ── Validation ────────────────────────────────────────────────────────

        public MdixValidationReport Validate(MdixDatabase db) =>
            MdixDatabaseValidator.Validate(db, this);
    }

    #endregion

    #region MdixDatabaseValidator

    internal static class MdixDatabaseValidator
    {
        internal static MdixValidationReport Validate(
            MdixDatabase      db,
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

        private static bool TypeMatches(Type clrType, MdixValueType actual)
        {
            if (clrType == typeof(string))
                return actual == MdixValueType.String
                    || actual == MdixValueType.Date
                    || actual == MdixValueType.Timestamp
                    || actual == MdixValueType.HexColor
                    || actual == MdixValueType.Blob
                    || actual == MdixValueType.Regex;

            if (clrType == typeof(int))
                return actual == MdixValueType.Int || actual == MdixValueType.Enum;

            // Long accepts both Long and Int since Int widens to long without loss.
            if (clrType == typeof(long))
                return actual == MdixValueType.Long || actual == MdixValueType.Int;

            if (clrType == typeof(float))  return actual == MdixValueType.Float;

            if (clrType == typeof(double))
                return actual == MdixValueType.Double
                    || actual == MdixValueType.Float
                    || actual == MdixValueType.Int;

            if (clrType == typeof(bool))   return actual == MdixValueType.Bool;

            if (clrType == typeof(MdixHexColor))  return actual == MdixValueType.HexColor;
            if (clrType == typeof(MdixBlob))      return actual == MdixValueType.Blob;
            if (clrType == typeof(MdixRegex))     return actual == MdixValueType.Regex;
            if (clrType == typeof(MdixDate))      return actual == MdixValueType.Date;
            if (clrType == typeof(MdixTimestamp)) return actual == MdixValueType.Timestamp;

            // Unknown CLR type — allow through.
            return true;
        }
    }

    #endregion
}
