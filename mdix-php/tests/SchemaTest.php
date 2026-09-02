<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\ExpectedType;
use MidManStudio\Mdix\MdixDatabase;
use MidManStudio\Mdix\MdixError;
use MidManStudio\Mdix\MdixSchemaBuilder;
use MidManStudio\Mdix\ValidationErrorKind;
use PHPUnit\Framework\TestCase;

/**
 * Integration tests for MdixSchemaBuilder.
 * Requires the native lib to be on MDIX_LIB_PATH or in mdix-php/lib/.
 */
final class SchemaTest extends TestCase
{
    private const VALID_SRC = '@DATA( app_name = "MyApp" port = 8080 debug = true )';
    private const MISSING_FIELD_SRC = '@DATA( port = 8080 )';
    private const WRONG_TYPE_SRC = '@DATA( app_name = "MyApp" port = "not-a-number" )';

    public function testValidateAllFieldsPresentAndTypedPasses(): void
    {
        $db = MdixDatabase::loadStr(self::VALID_SRC);
        try {
            $report = (new MdixSchemaBuilder())
                ->requireString('app_name')
                ->requireInt('port')
                ->optionalBool('debug')
                ->validate($db);

            self::assertTrue($report->isValid());
            self::assertSame(0, $report->errorCount());
        } finally {
            $db->close();
        }
    }

    public function testValidateMissingOptionalFieldStillPasses(): void
    {
        $db = MdixDatabase::loadStr(self::VALID_SRC);
        try {
            $report = (new MdixSchemaBuilder())
                ->requireString('app_name')
                ->optionalString('not_present')
                ->validate($db);

            self::assertTrue($report->isValid());
        } finally {
            $db->close();
        }
    }

    public function testValidateMissingRequiredFieldReportsMissing(): void
    {
        $db = MdixDatabase::loadStr(self::MISSING_FIELD_SRC);
        try {
            $report = (new MdixSchemaBuilder())
                ->requireString('app_name')
                ->requireInt('port')
                ->validate($db);

            self::assertFalse($report->isValid());
            self::assertCount(1, $report->errorsOfKind(ValidationErrorKind::Missing));
            self::assertSame(['app_name'], $report->failedPaths());
        } finally {
            $db->close();
        }
    }

    public function testValidateWrongTypeReportsWrongType(): void
    {
        $db = MdixDatabase::loadStr(self::WRONG_TYPE_SRC);
        try {
            $report = (new MdixSchemaBuilder())
                ->requireString('app_name')
                ->requireInt('port')
                ->validate($db);

            self::assertFalse($report->isValid());
            self::assertCount(1, $report->errorsOfKind(ValidationErrorKind::WrongType));
        } finally {
            $db->close();
        }
    }

    public function testValidateMultipleErrorsReportsAll(): void
    {
        $db = MdixDatabase::loadStr(self::MISSING_FIELD_SRC);
        try {
            $report = (new MdixSchemaBuilder())
                ->requireString('app_name')
                ->requireString('author')
                ->validate($db);

            self::assertSame(2, $report->errorCount());
        } finally {
            $db->close();
        }
    }

    public function testRequireWithCustomValidatorPasses(): void
    {
        $db = MdixDatabase::loadStr(self::VALID_SRC);
        try {
            $report = (new MdixSchemaBuilder())
                ->requireWith('port', ExpectedType::Int, function (MdixDatabase $data): ?string {
                    $port = $data->getInt('port');
                    return ($port >= 1025 && $port <= 65535) ? null : 'port out of range';
                })
                ->validate($db);

            self::assertTrue($report->isValid());
        } finally {
            $db->close();
        }
    }

    public function testRequireWithCustomValidatorFailsReportsInvalidValue(): void
    {
        $db = MdixDatabase::loadStr(self::VALID_SRC);
        try {
            $report = (new MdixSchemaBuilder())
                ->requireWith('port', ExpectedType::Int, fn (MdixDatabase $data): string => 'always fails')
                ->validate($db);

            self::assertFalse($report->isValid());
            self::assertCount(1, $report->errorsOfKind(ValidationErrorKind::InvalidValue));
        } finally {
            $db->close();
        }
    }

    public function testRequireWithSkipsCustomValidatorWhenTypeCheckAlreadyFailed(): void
    {
        $db = MdixDatabase::loadStr(self::MISSING_FIELD_SRC);
        try {
            $validatorRan = false;
            $report = (new MdixSchemaBuilder())
                ->requireWith('app_name', ExpectedType::String, function (MdixDatabase $data) use (&$validatorRan): ?string {
                    $validatorRan = true;
                    return null;
                })
                ->validate($db);

            self::assertFalse($report->isValid());
            self::assertFalse($validatorRan); // never ran -- the field was missing (type check failed first)
        } finally {
            $db->close();
        }
    }

    public function testFieldCountAndPathsReflectAddedFields(): void
    {
        $schema = (new MdixSchemaBuilder())->requireString('a')->requireInt('b')->optionalBool('c');
        self::assertSame(3, $schema->fieldCount());
        self::assertSame(['a', 'b', 'c'], $schema->paths());
    }

    public function testWithDescriptionBeforeAnyFieldThrows(): void
    {
        $this->expectException(MdixError::class);
        (new MdixSchemaBuilder())->withDescription('oops');
    }

    public function testReportToStringIsHumanReadable(): void
    {
        $db = MdixDatabase::loadStr(self::MISSING_FIELD_SRC);
        try {
            $report = (new MdixSchemaBuilder())->requireString('app_name')->validate($db);
            $text = (string) $report;
            self::assertStringContainsString('Validation failed', $text);
            self::assertStringContainsString('app_name', $text);
        } finally {
            $db->close();
        }
    }
}
