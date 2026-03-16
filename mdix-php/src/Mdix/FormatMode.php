<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/**
 * Controls how DixScript source or database content is formatted.
 *
 * Passed to MdixDatabase::toMdix() and MdixConverter::toMdix().
 * Ordinals match MDIX_FORMAT_MODE_* in the C header.
 */
enum FormatMode: int
{
    /** Readable output with standard 2-space indentation. */
    case Default  = 0;

    /** Readable output with 4-space indentation and sorted keys. */
    case Pretty   = 1;

    /** Compact output — trailing whitespace removed, blank lines collapsed. */
    case Compact  = 2;

    /** Smallest possible output — all unnecessary whitespace stripped. */
    case Minified = 3;
}
