<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/**
 * DixScript value type discriminants.
 *
 * Ordinal values match the integers returned by mdix_get_type() in the
 * native library (and MDIX_TYPE_* constants in the C header).
 */
enum ValueType: int
{
    case Unknown   = -1;
    case Null      =  0;
    case Bool      =  1;
    case Int       =  2;
    case Float     =  3;
    case Double    =  4;
    case String    =  5;
    case Date      =  6;
    case Timestamp =  7;
    case HexColor  =  8;
    case Blob      =  9;
    case Regex     = 10;
    case Array     = 11;
    case Object    = 12;
    case Tuple     = 13;
    case Enum      = 14;

    /**
     * Returns a human-readable label, e.g. "int", "string", "array".
     */
    public function label(): string
    {
        return match ($this) {
            self::Unknown   => 'unknown',
            self::Null      => 'null',
            self::Bool      => 'bool',
            self::Int       => 'int',
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
            self::Bool, self::Int, self::Float,
            self::Double, self::String => true,
            default => false,
        };
    }
}
