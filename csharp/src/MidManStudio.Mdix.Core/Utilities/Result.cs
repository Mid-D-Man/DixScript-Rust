using System;
using System.Threading.Tasks;

namespace MidManStudio.Utilities
{
    /// <summary>
    /// Represents the outcome of an operation that can either succeed with a value
    /// or fail with an error. Use <see cref="Result.Ok{TSuccess,TError}"/> and
    /// <see cref="Result.Err{TSuccess,TError}"/> to construct instances.
    /// </summary>
    public sealed class Result<TSuccess, TError>
    {
        private readonly TSuccess? _value;
        private readonly TError?   _error;

        private Result(TSuccess value)
        {
            IsSuccess      = true;
            _value         = value;
            _error         = default;
        }

        private Result(TError error, bool _)
        {
            IsSuccess      = false;
            _value         = default;
            _error         = error;
        }

        internal static Result<TSuccess, TError> CreateOk(TSuccess value)  => new(value);
        internal static Result<TSuccess, TError> CreateErr(TError error)    => new(error, false);

        /// <summary>True when the operation succeeded.</summary>
        public bool IsSuccess { get; }

        /// <summary>True when the operation failed.</summary>
        public bool IsFailure => !IsSuccess;

        /// <summary>
        /// The success value. Only valid when <see cref="IsSuccess"/> is true.
        /// Throws <see cref="InvalidOperationException"/> if accessed on a failure.
        /// </summary>
        public TSuccess SuccessResult =>
            IsSuccess
                ? _value!
                : throw new InvalidOperationException("Cannot access SuccessResult on a failed Result.");

        /// <summary>
        /// The error value. Only valid when <see cref="IsFailure"/> is true.
        /// Throws <see cref="InvalidOperationException"/> if accessed on a success.
        /// </summary>
        public TError Error =>
            IsFailure
                ? _error!
                : throw new InvalidOperationException("Cannot access Error on a successful Result.");

        // ── Unwrapping ────────────────────────────────────────────────────────

        /// <summary>
        /// Returns the success value or throws <see cref="ResultUnwrapException"/>.
        /// </summary>
        public TSuccess OrThrow()
        {
            if (IsSuccess) return _value!;
            throw new ResultUnwrapException($"Result was a failure: {_error}");
        }

        /// <summary>
        /// Returns the success value or throws the exception produced by
        /// <paramref name="exceptionFactory"/>.
        /// </summary>
        public TSuccess OrThrow(Func<TError, Exception> exceptionFactory)
        {
            if (IsSuccess) return _value!;
            throw exceptionFactory(_error!);
        }

        /// <summary>Alias for <see cref="OrThrow()"/> — familiar to Rust users.</summary>
        public TSuccess Unwrap() => OrThrow();

        /// <summary>
        /// Returns the success value, or <paramref name="fallback"/> if this is a failure.
        /// </summary>
        public TSuccess UnwrapOr(TSuccess fallback) => IsSuccess ? _value! : fallback;

        /// <summary>
        /// Returns the success value, or the value produced by
        /// <paramref name="fallbackFactory"/> if this is a failure.
        /// </summary>
        public TSuccess UnwrapOrElse(Func<TError, TSuccess> fallbackFactory) =>
            IsSuccess ? _value! : fallbackFactory(_error!);

        // ── Branching ─────────────────────────────────────────────────────────

        /// <summary>Invokes the matching action based on the result state.</summary>
        public void Match(Action<TSuccess> onSuccess, Action<TError> onFailure)
        {
            if (IsSuccess) onSuccess(_value!);
            else           onFailure(_error!);
        }

        /// <summary>
        /// Projects to a new value by invoking the matching function based on result state.
        /// </summary>
        public TResult Match<TResult>(
            Func<TSuccess, TResult> onSuccess,
            Func<TError,   TResult> onFailure) =>
            IsSuccess ? onSuccess(_value!) : onFailure(_error!);

        // ── Transformation ────────────────────────────────────────────────────

        /// <summary>
        /// Maps the success value to a new type. Failures are forwarded unchanged.
        /// </summary>
        public Result<TNewSuccess, TError> Map<TNewSuccess>(Func<TSuccess, TNewSuccess> mapper) =>
            IsSuccess
                ? Result<TNewSuccess, TError>.CreateOk(mapper(_value!))
                : Result<TNewSuccess, TError>.CreateErr(_error!);

        /// <summary>
        /// Maps the error value to a new type. Successes are forwarded unchanged.
        /// </summary>
        public Result<TSuccess, TNewError> MapError<TNewError>(Func<TError, TNewError> mapper) =>
            IsFailure
                ? Result<TSuccess, TNewError>.CreateErr(mapper(_error!))
                : Result<TSuccess, TNewError>.CreateOk(_value!);

        /// <summary>
        /// Maps both branches independently.
        /// </summary>
        public Result<TNewSuccess, TNewError> BiMap<TNewSuccess, TNewError>(
            Func<TSuccess, TNewSuccess> onSuccess,
            Func<TError,   TNewError>   onFailure) =>
            IsSuccess
                ? Result<TNewSuccess, TNewError>.CreateOk(onSuccess(_value!))
                : Result<TNewSuccess, TNewError>.CreateErr(onFailure(_error!));

        /// <summary>
        /// Chains a result-returning function on success. Failures short-circuit.
        /// </summary>
        public Result<TNewSuccess, TError> AndThen<TNewSuccess>(
            Func<TSuccess, Result<TNewSuccess, TError>> binder) =>
            IsSuccess
                ? binder(_value!)
                : Result<TNewSuccess, TError>.CreateErr(_error!);

        /// <summary>
        /// Returns <paramref name="fallback"/> if this is a failure, otherwise this.
        /// </summary>
        public Result<TSuccess, TError> Or(Result<TSuccess, TError> fallback) =>
            IsSuccess ? this : fallback;

        /// <summary>
        /// Validates the success value with a predicate.
        /// Returns <c>Err(error)</c> if the predicate returns false.
        /// </summary>
        public Result<TSuccess, TError> Ensure(Func<TSuccess, bool> predicate, TError error)
        {
            if (IsFailure)               return this;
            if (!predicate(_value!))     return CreateErr(error);
            return this;
        }

        // ── Side effects ──────────────────────────────────────────────────────

        /// <summary>Runs <paramref name="action"/> on the success value without transforming it.</summary>
        public Result<TSuccess, TError> Tap(Action<TSuccess> action)
        {
            if (IsSuccess) action(_value!);
            return this;
        }

        /// <summary>Runs <paramref name="action"/> on the error without transforming it.</summary>
        public Result<TSuccess, TError> TapError(Action<TError> action)
        {
            if (IsFailure) action(_error!);
            return this;
        }

        // ── Object overrides ──────────────────────────────────────────────────

        public override string ToString() =>
            IsSuccess ? $"Ok({_value})" : $"Err({_error})";
    }

    /// <summary>Static factory for <see cref="Result{TSuccess,TError}"/>.</summary>
    public static class Result
    {
        /// <summary>Creates a successful result wrapping <paramref name="value"/>.</summary>
        public static Result<TSuccess, TError> Ok<TSuccess, TError>(TSuccess value) =>
            Result<TSuccess, TError>.CreateOk(value);

        /// <summary>Creates a failed result wrapping <paramref name="error"/>.</summary>
        public static Result<TSuccess, TError> Err<TSuccess, TError>(TError error) =>
            Result<TSuccess, TError>.CreateErr(error);

        /// <summary>
        /// Wraps a potentially exception-throwing function into a Result.
        /// Any exception becomes <c>Err(exception)</c>.
        /// </summary>
        public static Result<TSuccess, Exception> Try<TSuccess>(Func<TSuccess> func)
        {
            try
            {
                return Result<TSuccess, Exception>.CreateOk(func());
            }
            catch (Exception ex)
            {
                return Result<TSuccess, Exception>.CreateErr(ex);
            }
        }

        /// <summary>
        /// Async version of <see cref="Try{TSuccess}"/>.
        /// Any exception becomes <c>Err(exception)</c>.
        /// </summary>
        public static async Task<Result<TSuccess, Exception>> TryAsync<TSuccess>(
            Func<Task<TSuccess>> func)
        {
            try
            {
                return Result<TSuccess, Exception>.CreateOk(await func().ConfigureAwait(false));
            }
            catch (Exception ex)
            {
                return Result<TSuccess, Exception>.CreateErr(ex);
            }
        }
    }

    /// <summary>
    /// Thrown by <see cref="Result{TSuccess,TError}.OrThrow()"/> and
    /// <see cref="Result{TSuccess,TError}.Unwrap()"/> when called on a failure.
    /// </summary>
    public sealed class ResultUnwrapException : Exception
    {
        public ResultUnwrapException(string message) : base(message) { }
        public ResultUnwrapException(string message, Exception inner) : base(message, inner) { }
    }
}
