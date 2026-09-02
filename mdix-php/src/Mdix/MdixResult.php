<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/**
 * Railway-oriented result type.
 *
 * Allows chaining operations without try/catch blocks.
 *
 *   $port = MdixDatabase::tryLoadStr($source)
 *       ->andThen(fn($db) => $db->tryGetInt('server.port'))
 *       ->ensure(fn($p) => $p > 1024, 'port must be > 1024')
 *       ->map(fn($p) => $p * 2)
 *       ->unwrapOr(3000);
 */
final class MdixResult
{
    private function __construct(
        private readonly mixed $value,
        private readonly ?string $error,
    ) {}

    // ── Construction ──────────────────────────────────────────────────────────

    public static function ok(mixed $value): self
    {
        return new self($value, null);
    }

    public static function err(string $message): self
    {
        return new self(null, $message);
    }

    /** Capture a MdixError (or any Throwable) as a failure result. */
    public static function fromThrowable(\Throwable $e): self
    {
        return new self(null, $e->getMessage());
    }

    // ── State ─────────────────────────────────────────────────────────────────

    public function isSuccess(): bool
    {
        return $this->error === null;
    }

    public function isFailure(): bool
    {
        return $this->error !== null;
    }

    public function getValue(): mixed
    {
        if ($this->error !== null) {
            throw new \LogicException(
                'Cannot call getValue() on a failed MdixResult. Check isSuccess() first.'
            );
        }

        return $this->value;
    }

    public function getError(): string
    {
        if ($this->error === null) {
            throw new \LogicException(
                'Cannot call getError() on a successful MdixResult.'
            );
        }

        return $this->error;
    }

    // ── Unwrapping ────────────────────────────────────────────────────────────

    /**
     * Return the value or throw MdixError on failure.
     *
     * @throws MdixError
     */
    public function orRaise(): mixed
    {
        if ($this->error !== null) {
            throw MdixError::fromMessage($this->error);
        }

        return $this->value;
    }

    /** Alias for orRaise(). */
    public function unwrap(): mixed
    {
        return $this->orRaise();
    }

    /** Return the value or $fallback if this result is a failure. */
    public function unwrapOr(mixed $fallback): mixed
    {
        return $this->error === null ? $this->value : $fallback;
    }

    /**
     * Return the value, or call $factory(errorMessage) and return its result.
     *
     * @param callable(string): mixed $factory
     */
    public function unwrapOrElse(callable $factory): mixed
    {
        return $this->error === null
            ? $this->value
            : $factory($this->error);
    }

    // ── Transformation ────────────────────────────────────────────────────────

    /**
     * Apply $f to the value if success; forward the failure unchanged.
     *
     * @param callable(mixed): mixed $f
     */
    public function map(callable $f): self
    {
        if ($this->error !== null) {
            return $this;
        }

        try {
            return self::ok($f($this->value));
        } catch (\Throwable $e) {
            return self::fromThrowable($e);
        }
    }

    /**
     * Chain another result-returning operation on success.
     *
     * @param callable(mixed): MdixResult $f
     */
    public function andThen(callable $f): self
    {
        if ($this->error !== null) {
            return $this;
        }

        try {
            $next = $f($this->value);
            if (!$next instanceof self) {
                throw new \LogicException(
                    'andThen callback must return a MdixResult instance.'
                );
            }

            return $next;
        } catch (\Throwable $e) {
            // FIX: this used to also have a `catch (self $e)` branch ahead
            // of this one, presumably meant to special-case the
            // \LogicException thrown just above — `self` is not a legal
            // catch type in PHP (a fatal parse error the moment this file
            // is loaded, i.e. the first time anything calls a try*()
            // method anywhere in this library) and MdixResult isn't
            // \Throwable in the first place, so that branch could never
            // have been reached even with valid syntax. This single
            // \Throwable catch already covers the LogicException case.
            return self::fromThrowable($e);
        }
    }

    /**
     * Validate the value with a predicate; fail with $errorMessage if it returns false.
     *
     * @param callable(mixed): bool $predicate
     */
    public function ensure(callable $predicate, string $errorMessage): self
    {
        if ($this->error !== null) {
            return $this;
        }

        try {
            return $predicate($this->value) ? $this : self::err($errorMessage);
        } catch (\Throwable $e) {
            return self::fromThrowable($e);
        }
    }

    /**
     * Return this result on success; return $fallback result on failure.
     */
    public function or(self $fallback): self
    {
        return $this->error === null ? $this : $fallback;
    }

    // ── Branching ─────────────────────────────────────────────────────────────

    /**
     * Call one of two callbacks depending on success/failure.
     *
     * @param callable(mixed): mixed  $onSuccess
     * @param callable(string): mixed $onFailure
     */
    public function fold(callable $onSuccess, callable $onFailure): mixed
    {
        return $this->error === null
            ? $onSuccess($this->value)
            : $onFailure($this->error);
    }

    // ── Side effects ──────────────────────────────────────────────────────────

    /**
     * Call $f with the value (for side effects) on success; return self.
     *
     * @param callable(mixed): void $f
     */
    public function tap(callable $f): self
    {
        if ($this->error === null) {
            $f($this->value);
        }

        return $this;
    }

    /**
     * Call $f with the error message on failure; return self.
     *
     * @param callable(string): void $f
     */
    public function tapError(callable $f): self
    {
        if ($this->error !== null) {
            $f($this->error);
        }

        return $this;
    }

    // ── Dunder ────────────────────────────────────────────────────────────────

    public function __toString(): string
    {
        if ($this->error === null) {
            $repr = \is_string($this->value)
                ? "'{$this->value}'"
                : \print_r($this->value, true);

            return "MdixResult::ok({$repr})";
        }

        return "MdixResult::err('{$this->error}')";
    }
}
