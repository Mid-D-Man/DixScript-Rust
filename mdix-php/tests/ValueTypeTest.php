<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\MdixDatabase;
use MidManStudio\Mdix\ValueType;
use PHPUnit\Framework\TestCase;

/**
 * Regression coverage for a real bug: ValueType (and ffi_header.h's MdixType
 * mirror) was missing the Long case entirely, silently shifting every case
 * from Float onward one below its real native discriminant, and topping out
 * one short of the real Enum value -- so any actual DixScript enum field
 * crashed valueTypeAt() with an uncaught \ValueError. Requires the native
 * lib to be on MDIX_LIB_PATH or in mdix-php/lib/.
 */
final class ValueTypeTest extends TestCase
{
    public function testEnumOrdinalsMatchNativeDiscriminants(): void
    {
        // These must match mdix-ffi/src/lib.rs's MdixType exactly -- this is
        // the assertion that would have caught the original bug immediately.
        self::assertSame(-1, ValueType::Unknown->value);
        self::assertSame(0, ValueType::Null->value);
        self::assertSame(1, ValueType::Bool->value);
        self::assertSame(2, ValueType::Int->value);
        self::assertSame(3, ValueType::Long->value);
        self::assertSame(4, ValueType::Float->value);
        self::assertSame(5, ValueType::Double->value);
        self::assertSame(6, ValueType::String->value);
        self::assertSame(7, ValueType::Date->value);
        self::assertSame(8, ValueType::Timestamp->value);
        self::assertSame(9, ValueType::HexColor->value);
        self::assertSame(10, ValueType::Blob->value);
        self::assertSame(11, ValueType::Regex->value);
        self::assertSame(12, ValueType::Array->value);
        self::assertSame(13, ValueType::Object->value);
        self::assertSame(14, ValueType::Tuple->value);
        self::assertSame(15, ValueType::Enum->value);
    }

    public function testFromDoesNotThrowForEveryRealDiscriminant(): void
    {
        // The pre-fix enum topped out at 14 (Enum was undefined), so
        // ValueType::from(15) threw an uncaught \ValueError -- every real
        // enum-typed DixScript field would have crashed valueTypeAt().
        foreach (\range(-1, 15) as $code) {
            $type = ValueType::from($code);
            self::assertInstanceOf(ValueType::class, $type);
        }
    }

    public function testValueTypeAtReportsLongCorrectly(): void
    {
        $db = MdixDatabase::loadStr('@DATA( big_number = 9000000000L )');
        try {
            self::assertSame(ValueType::Long, $db->valueTypeAt('big_number'));
        } finally {
            $db->close();
        }
    }

    public function testValueTypeAtReportsEnumWithoutThrowing(): void
    {
        $db = MdixDatabase::loadStr(<<<'MDIX'
@ENUMS( Status:: ACTIVE = 0, INACTIVE = 1 )
@DATA( status = Status.ACTIVE )
MDIX);
        try {
            self::assertSame(ValueType::Enum, $db->valueTypeAt('status'));
        } finally {
            $db->close();
        }
    }

    public function testGetLongReadsA64BitValue(): void
    {
        $db = MdixDatabase::loadStr('@DATA( big_number = 9000000000L )');
        try {
            self::assertSame(9000000000, $db->getLong('big_number'));
        } finally {
            $db->close();
        }
    }
}
