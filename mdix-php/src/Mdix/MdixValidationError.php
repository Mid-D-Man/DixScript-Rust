<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/** One field that failed schema validation. */
final class MdixValidationError implements \Stringable
{
    public function __construct(
        public readonly string $path,
        public readonly string $expected,
        public readonly string $actual,
        public readonly ValidationErrorKind $kind,
    ) {
    }

    public function __toString(): string
    {
        return "[{$this->kind->value}] '{$this->path}': expected {$this->expected}, got {$this->actual}";
    }
}
