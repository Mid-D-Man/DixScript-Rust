<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/** How to combine two array-valued entries that share a path across merge sources. */
enum ArrayMergeStrategy: int
{
    /** The winning source's array entirely replaces the losing one's. */
    case Replace = 0;
    /** Both arrays are concatenated, winner's items first. */
    case Concat = 1;
    /** Concatenated (winner first), with exact-duplicate primitive values removed. Complex values are never deduped. */
    case ConcatDedup = 2;
}
