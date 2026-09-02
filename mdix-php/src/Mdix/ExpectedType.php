<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/** The value type a schema field must satisfy. Mirrors ValueType's variants. */
enum ExpectedType: string
{
    case String = 'String';
    case Int = 'Int';
    case Long = 'Long';
    case Float = 'Float';
    case Double = 'Double';
    case Bool = 'Bool';
    case Array = 'Array';
    case Object = 'Object';
    case Date = 'Date';
    case Timestamp = 'Timestamp';
    case HexColor = 'HexColor';
    case Blob = 'Blob';
    case Regex = 'Regex';
    case Enum = 'Enum';
    /** Accepts any value type. */
    case Any = 'Any';
}
