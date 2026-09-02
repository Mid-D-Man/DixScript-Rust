<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/** One key that more than one merge source defined, and which source's value won. */
final class MdixMergeConflict implements \Stringable
{
    public function __construct(
        public readonly string $path,
        public readonly int $winningSource,
        public readonly ?string $winningLabel,
    ) {
    }

    public function __toString(): string
    {
        $label = $this->winningLabel !== null ? " ({$this->winningLabel})" : '';
        return "'{$this->path}' -> source[{$this->winningSource}]{$label}";
    }
}
