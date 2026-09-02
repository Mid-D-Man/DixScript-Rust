<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/**
 * How to resolve a key defined by more than one source in
 * MdixMerge::sources() / MdixMerge::sourcesWeighted().
 */
enum MergeStrategy: int
{
    /** Each source's weight decides the winner; equal weights fall back to the lower-indexed (primary) source. */
    case WeightedPriority = 0;
    /** The lower-indexed source always wins, regardless of weight. */
    case PrimaryWins = 1;
    /** The higher-indexed source always wins, regardless of weight. */
    case SecondaryWins = 2;
    /** Any key defined by more than one source is a hard error — the merge fails outright. */
    case ThrowOnConflict = 3;
}
