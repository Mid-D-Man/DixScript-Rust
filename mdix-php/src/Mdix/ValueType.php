<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/**
 * DixScript value type discriminants.
 *
 * Ordinal values match the integers returned by mdix_get_type() in the
 * native library (and MDIX_TYPE_* constants in the C header).
 *
 * FIX: this enum was previously missing the Long case entirely, which
 * shifted every case from Float onward one below its real native
 * discriminant (native Long=3 read back as this enum's old Float=3;
 * native Enum=15 had no matching case at all, since this enum only went
 * up to 14 — every actual DixScript enum field crashed valueTypeAt() with
 * an uncaught \ValueError). Values below are verified against
 * mdix-ffi/src/lib.rs's MdixType exactly.
 */
enum ValueType: int
{
    case Unknown   = -1;
    case Null      =  0;
    case Bool      =  1;
    case Int       =  2;
    case Long      =  3;
    case Float     =  4;
    case Double    =  5;
    case String    =  6;
    case Date      =  7;
    case Timestamp =  8;
    case HexColor  =  9;
    case Blob      = 10;
    case Regex     = 11;
    case Array     = 12;
    case Object    = 13;
    case Tuple     = 14;
    case Enum      = 15;

    /**
     * Returns a human-readable label, e.g. "int", "long", "string", "array".
     */
    public function label(): string
    {
        return match ($this) {
            self::Unknown   => 'unknown',
            self::Null      => 'null',
            self::Bool      => 'bool',
            self::Int       => 'int',
            self::Long      => 'long',
            self::Float     => 'float',
            self::Double    => 'double',
            self::String    => 'string',
            self::Date      => 'date',
            self::Timestamp => 'timestamp',
            self::HexColor  => 'hex_color',
            self::Blob      => 'blob',
            self::Regex     => 'regex',
            self::Array     => 'array',
            self::Object    => 'object',
            self::Tuple     => 'tuple',
            self::Enum      => 'enum',
        };
    }

    public function isScalar(): bool
    {
        return match ($this) {
            self::Bool, self::Int, self::Long, self::Float,
            self::Double, self::String => true,
            default => false,
        };
    }
}
