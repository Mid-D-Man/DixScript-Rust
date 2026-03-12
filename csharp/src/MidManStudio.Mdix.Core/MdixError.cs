using System;

namespace MidManStudio.Mdix.Core
{
    #region Error Kind

    /// <summary>Classifies the category of a DixScript runtime error.</summary>
    public enum MdixErrorKind
    {
        /// <summary>The requested path does not exist in the loaded data.</summary>
        NotFound,

        /// <summary>The value at the path cannot be converted to the requested type.</summary>
        TypeMismatch,

        /// <summary>The native handle is null or has already been freed.</summary>
        NullHandle,

        /// <summary>The path string is null or empty.</summary>
        InvalidPath,

        /// <summary>The native FFI layer returned an error.</summary>
        NativeError,

        /// <summary>A file system operation failed.</summary>
        IoError,

        /// <summary>The source could not be parsed as valid DixScript.</summary>
        ParseError,

        /// <summary>The loaded data does not match the expected schema.</summary>
        SchemaError,

        /// <summary>The object has been disposed and cannot be used.</summary>
        Disposed,
    }

    #endregion

    #region MdixError

    /// <summary>
    /// Immutable error value returned by all DixScript operations.
    /// </summary>
    public readonly struct MdixError : IEquatable<MdixError>
    {
        /// <summary>The category of this error.</summary>
        public MdixErrorKind Kind { get; }

        /// <summary>Human-readable description of what went wrong.</summary>
        public string Message { get; }

        /// <summary>The dotted path that triggered the error, if applicable.</summary>
        public string? Path { get; }

        /// <summary>An underlying exception, if the error originated from one.</summary>
        public Exception? InnerException { get; }

        public MdixError(
            MdixErrorKind kind,
            string        message,
            string?       path           = null,
            Exception?    innerException = null)
        {
            Kind           = kind;
            Message        = message;
            Path           = path;
            InnerException = innerException;
        }

        #region Factories

        public static MdixError NotFound(string path) =>
            new(MdixErrorKind.NotFound, $"Path not found: '{path}'", path);

        public static MdixError TypeMismatch(string path, string expected, string actual) =>
            new(MdixErrorKind.TypeMismatch,
                $"Type mismatch at '{path}': expected {expected}, got {actual}",
                path);

        public static MdixError NullHandle() =>
            new(MdixErrorKind.NullHandle, "The native handle is null or has been freed.");

        public static MdixError InvalidPath(string? path) =>
            new(MdixErrorKind.InvalidPath, $"Path is null or empty: '{path}'", path);

        public static MdixError NativeError(string message) =>
            new(MdixErrorKind.NativeError, message);

        public static MdixError IoError(string message, Exception? inner = null) =>
            new(MdixErrorKind.IoError, message, null, inner);

        public static MdixError ParseError(string message) =>
            new(MdixErrorKind.ParseError, message);

        public static MdixError SchemaError(string message) =>
            new(MdixErrorKind.SchemaError, message);

        public static MdixError Disposed(string typeName) =>
            new(MdixErrorKind.Disposed, $"{typeName} has been disposed and cannot be used.");

        #endregion

        #region Equality

        public bool Equals(MdixError other) =>
            Kind == other.Kind && Message == other.Message && Path == other.Path;

        public override bool Equals(object? obj) => obj is MdixError other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(Kind, Message, Path);

        public static bool operator ==(MdixError left, MdixError right) =>  left.Equals(right);
        public static bool operator !=(MdixError left, MdixError right) => !left.Equals(right);

        public override string ToString() =>
            Path is null
                ? $"[{Kind}] {Message}"
                : $"[{Kind}] {Message} (path: '{Path}')";

        #endregion
    }

    #endregion

    #region MdixException

    /// <summary>
    /// Thrown only by <c>OrThrow()</c> and <c>Unwrap()</c> on a failed
    /// <see cref="MdixResult{T}"/>. Never thrown implicitly.
    /// </summary>
    public sealed class MdixException : Exception
    {
        /// <summary>The structured error that caused this exception.</summary>
        public MdixError MdixError { get; }

        public MdixException(MdixError error)
            : base(error.ToString(), error.InnerException)
        {
            MdixError = error;
        }
    }

    #endregion

    #region MdixResult

    /// <summary>
    /// Represents the outcome of a DixScript operation — either a success value
    /// of type <typeparamref name="T"/> or a <see cref="MdixError"/>.
    /// The primary return type for all DixScript API calls.
    /// </summary>
    public sealed class MdixResult<T>
    {
        private readonly T?         _value;
        private readonly MdixError  _error;

        private MdixResult(T value)
        {
            IsSuccess = true;
            _value    = value;
            _error    = default;
        }

        private MdixResult(MdixError error)
        {
            IsSuccess = false;
            _value    = default;
            _error    = error;
        }

        // ── Construction ──────────────────────────────────────────────────────

        /// <summary>Creates a successful result wrapping <paramref name="value"/>.</summary>
        public static MdixResult<T> Ok(T value) => new(value);

        /// <summary>Creates a failed result wrapping <paramref name="error"/>.</summary>
        public static MdixResult<T> Err(MdixError error) => new(error);

        // ── State ─────────────────────────────────────────────────────────────

        /// <summary>True when the operation succeeded.</summary>
        public bool IsSuccess { get; }

        /// <summary>True when the operation failed.</summary>
        public bool IsFailure => !IsSuccess;

        /// <summary>
        /// The success value. Throws <see cref="InvalidOperationException"/>
        /// if accessed on a failure — prefer <see cref="OrThrow"/> or
        /// <see cref="Match{TResult}"/> instead.
        /// </summary>
        public T SuccessResult =>
            IsSuccess
                ? _value!
                : throw new InvalidOperationException(
                    "Cannot access SuccessResult on a failed MdixResult.");

        /// <summary>
        /// The error value. Throws <see cref="InvalidOperationException"/>
        /// if accessed on a success.
        /// </summary>
        public MdixError Error =>
            IsFailure
                ? _error
                : throw new InvalidOperationException(
                    "Cannot access Error on a successful MdixResult.");

        // ── Unwrapping ────────────────────────────────────────────────────────

        /// <summary>
        /// Returns the success value or throws <see cref="MdixException"/>.
        /// </summary>
        public T OrThrow()
        {
            if (IsSuccess) return _value!;
            throw new MdixException(_error);
        }

        /// <summary>Alias for <see cref="OrThrow()"/> — familiar to Rust users.</summary>
        public T Unwrap() => OrThrow();

        /// <summary>Returns the success value or <paramref name="fallback"/>.</summary>
        public T UnwrapOr(T fallback) => IsSuccess ? _value! : fallback;

        /// <summary>
        /// Returns the success value or the value produced by
        /// <paramref name="fallbackFactory"/>.
        /// </summary>
        public T UnwrapOrElse(Func<MdixError, T> fallbackFactory) =>
            IsSuccess ? _value! : fallbackFactory(_error);

        // ── Branching ─────────────────────────────────────────────────────────

        /// <summary>Invokes the matching action based on result state.</summary>
        public void Match(Action<T> onSuccess, Action<MdixError> onFailure)
        {
            if (IsSuccess) onSuccess(_value!);
            else           onFailure(_error);
        }

        /// <summary>Projects to a new value based on result state.</summary>
        public TResult Match<TResult>(
            Func<T,          TResult> onSuccess,
            Func<MdixError,  TResult> onFailure) =>
            IsSuccess ? onSuccess(_value!) : onFailure(_error);

        // ── Transformation ────────────────────────────────────────────────────

        /// <summary>Maps the success value. Failures are forwarded unchanged.</summary>
        public MdixResult<TNew> Map<TNew>(Func<T, TNew> mapper) =>
            IsSuccess
                ? MdixResult<TNew>.Ok(mapper(_value!))
                : MdixResult<TNew>.Err(_error);

        /// <summary>
        /// Chains a result-returning function on success. Failures short-circuit.
        /// </summary>
        public MdixResult<TNew> AndThen<TNew>(Func<T, MdixResult<TNew>> binder) =>
            IsSuccess ? binder(_value!) : MdixResult<TNew>.Err(_error);

        /// <summary>
        /// Validates the success value with a predicate. Returns
        /// <c>Err(error)</c> if the predicate returns false.
        /// </summary>
        public MdixResult<T> Ensure(Func<T, bool> predicate, MdixError error)
        {
            if (IsFailure)           return this;
            if (!predicate(_value!)) return Err(error);
            return this;
        }

        /// <summary>Returns <paramref name="fallback"/> if this is a failure.</summary>
        public MdixResult<T> Or(MdixResult<T> fallback) => IsSuccess ? this : fallback;

        // ── Side effects ──────────────────────────────────────────────────────

        /// <summary>Runs <paramref name="action"/> on the success value without transforming it.</summary>
        public MdixResult<T> Tap(Action<T> action)
        {
            if (IsSuccess) action(_value!);
            return this;
        }

        /// <summary>Runs <paramref name="action"/> on the error without transforming it.</summary>
        public MdixResult<T> TapError(Action<MdixError> action)
        {
            if (IsFailure) action(_error);
            return this;
        }

        // ── Implicit conversions ──────────────────────────────────────────────

        /// <summary>Allows returning a bare <see cref="MdixError"/> where a result is expected.</summary>
        public static implicit operator MdixResult<T>(MdixError error) => Err(error);

        // ── Object overrides ──────────────────────────────────────────────────

        public override string ToString() =>
            IsSuccess ? $"Ok({_value})" : $"Err({_error})";
    }

    #endregion

    #region Unit

    /// <summary>
    /// Represents a void success — used as the success type when an operation
    /// succeeds but produces no value (e.g. Save, Reload).
    /// </summary>
    public readonly struct Unit : IEquatable<Unit>
    {
        public static readonly Unit Value = default;

        public bool Equals(Unit other)          => true;
        public override bool Equals(object? obj) => obj is Unit;
        public override int  GetHashCode()       => 0;
        public override string ToString()        => "()";

        public static bool operator ==(Unit left, Unit right) => true;
        public static bool operator !=(Unit left, Unit right) => false;
    }

    #endregion
}
