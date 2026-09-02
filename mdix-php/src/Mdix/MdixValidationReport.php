<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/** The result of a schema validation pass. Never throws — always returned. */
final class MdixValidationReport implements \Stringable
{
    /** @param MdixValidationError[] $errors */
    public function __construct(public readonly array $errors)
    {
    }

    /** True when no errors were found. */
    public function isValid(): bool
    {
        return $this->errors === [];
    }

    public function errorCount(): int
    {
        return \count($this->errors);
    }

    /** @return MdixValidationError[] */
    public function errorsOfKind(ValidationErrorKind $kind): array
    {
        return \array_values(\array_filter($this->errors, fn (MdixValidationError $e): bool => $e->kind === $kind));
    }

    /** @return string[] */
    public function failedPaths(): array
    {
        return \array_map(fn (MdixValidationError $e): string => $e->path, $this->errors);
    }

    public function __toString(): string
    {
        if ($this->isValid()) {
            return 'Validation passed.';
        }

        $lines = ['Validation failed with ' . \count($this->errors) . ' error(s):'];
        foreach ($this->errors as $e) {
            $lines[] = (string) $e;
        }
        return \implode("\n", $lines);
    }
}
