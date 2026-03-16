<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/**
 * Thrown when any DixScript operation fails.
 *
 * The message comes directly from the Rust error string.
 * Inspect $kind to branch on the failure category without string-matching.
 */
class MdixError extends \RuntimeException
{
    public function __construct(
        string $message,
        public readonly ErrorKind $kind = ErrorKind::Native,
        ?\Throwable $previous = null,
    ) {
        parent::__construct($message, 0, $previous);
    }

    /**
     * Infer an ErrorKind from a Rust error message string.
     * Used when a kind is not explicitly known.
     */
    public static function fromMessage(string $message): self
    {
        $lower = \strtolower($message);

        $kind = match (true) {
            \str_contains($lower, 'not found'), \str_contains($lower, 'path not found')
                => ErrorKind::NotFound,
            \str_contains($lower, 'type') && \str_contains($lower, 'convert')
                => ErrorKind::TypeMismatch,
            \str_contains($lower, 'null handle'), \str_contains($lower, 'null pointer')
                => ErrorKind::NullHandle,
            \str_contains($lower, 'invalid path'), \str_contains($lower, 'path is null')
                => ErrorKind::InvalidPath,
            \str_contains($lower, 'parse'), \str_contains($lower, 'syntax')
                => ErrorKind::Parse,
            \str_contains($lower, 'write'), \str_contains($lower, 'file')
                => ErrorKind::Io,
            \str_contains($lower, 'closed'), \str_contains($lower, 'freed')
                => ErrorKind::Closed,
            default => ErrorKind::Native,
        };

        return new self($message, $kind);
    }
}

/**
 * Classifies the category of a MdixError.
 */
enum ErrorKind
{
    case NotFound;
    case TypeMismatch;
    case NullHandle;
    case InvalidPath;
    case Native;
    case Io;
    case Parse;
    case Closed;
}
