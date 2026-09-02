<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

/** Why a schema field failed validation. */
enum ValidationErrorKind: string
{
    /** The field is required but absent. */
    case Missing = 'Missing';
    /** The field is present but has the wrong value type. */
    case WrongType = 'WrongType';
    /** The field passes the type check but fails a custom validator. */
    case InvalidValue = 'InvalidValue';

    public static function fromWire(string $wire): self
    {
        return match ($wire) {
            'WrongType' => self::WrongType,
            'InvalidValue' => self::InvalidValue,
            default => self::Missing,
        };
    }
}
