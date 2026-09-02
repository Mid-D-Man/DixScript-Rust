<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

use MidManStudio\Mdix\Internal\NativeLoader;

/**
 * Fluent builder for schema definitions. Mirrors Rust's
 * dixscript::Runtime::SchemaBuilder.
 *
 * Every require*()/optional() call chains; each adds one field.
 * withDescription() annotates the most recently added field. Call
 * validate() to run the check — the same builder can be reused across
 * multiple databases.
 *
 *   $report = (new MdixSchemaBuilder())
 *       ->requireString('app_name')
 *       ->requireInt('port')
 *       ->requireWith('port', ExpectedType::Int, function (MdixDatabase $data) {
 *           $port = $data->getInt('port');
 *           return ($port >= 1025 && $port <= 65535) ? null : "port {$port} out of range";
 *       })
 *       ->optionalBool('debug')
 *       ->validate($db);
 *
 *   if (!$report->isValid()) {
 *       echo $report, "\n";
 *       // Validation failed with 1 error(s):
 *       // [Missing] 'app_name': expected string (required), got missing
 *   }
 *
 * The type/required check runs natively via mdix_schema_validate() — the
 * same SchemaBuilder DixScript's Rust runtime uses. Custom validators
 * (requireWith()/optionalWith()) can't cross the FFI boundary as a Rust
 * closure the way they do in the Rust API, so they run afterward in pure
 * PHP instead, against the already-loaded MdixDatabase — functionally
 * equivalent, just evaluated in managed code.
 */
final class MdixSchemaBuilder
{
    /** @var array<int, array{path: string, required: bool, type: ExpectedType, description: ?string}> */
    private array $fields = [];

    /** @var array<int, array{path: string, validator: callable(MdixDatabase): ?string}> */
    private array $validators = [];

    // ── required ─────────────────────────────────────────────────────────────

    public function require(string $path, ExpectedType $type): self
    {
        $this->fields[] = ['path' => $path, 'required' => true, 'type' => $type, 'description' => null];
        return $this;
    }

    /**
     * Adds a required field with a type check AND a custom validator. The
     * validator runs only when the type check passes, evaluated against
     * the whole MdixDatabase.
     *
     * @param callable(MdixDatabase): ?string $validator returns null if valid, or an error message.
     */
    public function requireWith(string $path, ExpectedType $type, callable $validator): self
    {
        $this->require($path, $type);
        $this->validators[] = ['path' => $path, 'validator' => $validator];
        return $this;
    }

    public function requireString(string $path): self { return $this->require($path, ExpectedType::String); }
    public function requireInt(string $path): self { return $this->require($path, ExpectedType::Int); }
    public function requireLong(string $path): self { return $this->require($path, ExpectedType::Long); }
    public function requireFloat(string $path): self { return $this->require($path, ExpectedType::Float); }
    public function requireDouble(string $path): self { return $this->require($path, ExpectedType::Double); }
    public function requireBool(string $path): self { return $this->require($path, ExpectedType::Bool); }
    public function requireArray(string $path): self { return $this->require($path, ExpectedType::Array); }
    public function requireObject(string $path): self { return $this->require($path, ExpectedType::Object); }
    public function requireEnum(string $path): self { return $this->require($path, ExpectedType::Enum); }

    // ── optional ─────────────────────────────────────────────────────────────

    public function optional(string $path, ExpectedType $type): self
    {
        $this->fields[] = ['path' => $path, 'required' => false, 'type' => $type, 'description' => null];
        return $this;
    }

    /** As requireWith(), but the field (and its custom validator) is only checked when present. */
    public function optionalWith(string $path, ExpectedType $type, callable $validator): self
    {
        $this->optional($path, $type);
        $this->validators[] = ['path' => $path, 'validator' => $validator];
        return $this;
    }

    public function optionalString(string $path): self { return $this->optional($path, ExpectedType::String); }
    public function optionalInt(string $path): self { return $this->optional($path, ExpectedType::Int); }
    public function optionalLong(string $path): self { return $this->optional($path, ExpectedType::Long); }
    public function optionalFloat(string $path): self { return $this->optional($path, ExpectedType::Float); }
    public function optionalDouble(string $path): self { return $this->optional($path, ExpectedType::Double); }
    public function optionalBool(string $path): self { return $this->optional($path, ExpectedType::Bool); }
    public function optionalArray(string $path): self { return $this->optional($path, ExpectedType::Array); }
    public function optionalObject(string $path): self { return $this->optional($path, ExpectedType::Object); }
    public function optionalEnum(string $path): self { return $this->optional($path, ExpectedType::Enum); }

    // ── metadata ─────────────────────────────────────────────────────────────

    /** Annotates the most recently added field with a human-readable description. */
    public function withDescription(string $description): self
    {
        if ($this->fields === []) {
            throw new MdixError('MdixSchemaBuilder: withDescription() called before any require/optional field', ErrorKind::InvalidPath);
        }
        $this->fields[\array_key_last($this->fields)]['description'] = $description;
        return $this;
    }

    public function fieldCount(): int
    {
        return \count($this->fields);
    }

    /** @return string[] */
    public function paths(): array
    {
        return \array_map(static fn (array $f): string => $f['path'], $this->fields);
    }

    // ── validate ─────────────────────────────────────────────────────────────

    /** Runs every field check (natively) and every custom validator (in PHP) against $data. */
    public function validate(MdixDatabase $data): MdixValidationReport
    {
        $ffi = NativeLoader::get();
        $fieldsJson = \json_encode(\array_map(
            static function (array $f): array {
                $entry = ['path' => $f['path'], 'required' => $f['required'], 'type' => $f['type']->value];
                if ($f['description'] !== null) {
                    $entry['description'] = $f['description'];
                }
                return $entry;
            },
            $this->fields,
        ), \JSON_THROW_ON_ERROR);

        $ptr = $ffi->mdix_schema_validate($data->rawHandle(), $fieldsJson);
        if ($ptr === null) {
            $errPtr = $ffi->mdix_get_last_error();
            $msg = $errPtr !== null ? $errPtr : 'unknown native error';
            throw MdixError::fromMessage("[mdix:schemaValidate] {$msg}");
        }
        $errorsJson = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        $errors = self::parseErrors($errorsJson);

        foreach ($this->validators as ['path' => $path, 'validator' => $validator]) {
            $typeCheckFailed = false;
            foreach ($errors as $e) {
                if ($e->path === $path) {
                    $typeCheckFailed = true;
                    break;
                }
            }
            if ($typeCheckFailed) {
                continue; // matches Rust: the custom validator only runs once the type check passes
            }

            try {
                $message = $validator($data);
            } catch (\Throwable $e) {
                $message = $e->getMessage();
            }
            if ($message !== null) {
                $errors[] = new MdixValidationError($path, 'custom validation to pass', $message, ValidationErrorKind::InvalidValue);
            }
        }

        return new MdixValidationReport($errors);
    }

    /** @return MdixValidationError[] */
    private static function parseErrors(string $json): array
    {
        $decoded = \json_decode($json, associative: true);
        if (!\is_array($decoded)) {
            return [];
        }

        $out = [];
        foreach ($decoded as $entry) {
            if (!\is_array($entry)) {
                continue;
            }
            $out[] = new MdixValidationError(
                (string) ($entry['path'] ?? ''),
                (string) ($entry['expected'] ?? ''),
                (string) ($entry['actual'] ?? ''),
                ValidationErrorKind::fromWire((string) ($entry['kind'] ?? '')),
            );
        }
        return $out;
    }
}
