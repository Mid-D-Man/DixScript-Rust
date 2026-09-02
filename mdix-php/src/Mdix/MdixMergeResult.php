<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/** The merged database plus a report of every key more than one source defined. */
final class MdixMergeResult
{
    /** @param MdixMergeConflict[] $conflicts */
    public function __construct(
        public readonly MdixDatabase $database,
        public readonly array $conflicts,
    ) {
    }

    public function hasConflicts(): bool
    {
        return $this->conflicts !== [];
    }
}
