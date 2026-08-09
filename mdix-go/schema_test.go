package dixscript

import "testing"

const schemaFixture = `
@DATA(
  app_name = "TestApp"
  port     = 8080
  big_id   = 9_000_000_000L
  ratio    = 3.14f
  pi       = 3.14159265358979
  debug    = true
)`

func TestSchemaAllRequiredPresent(t *testing.T) {
	db, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	report := NewSchema().
		RequireString("app_name").
		RequireInt("port").
		RequireBool("debug").
		Validate(db)

	if !report.IsValid() {
		t.Errorf("report.IsValid() = false, errors: %v", report.Errors)
	}
	if report.ErrorCount() != 0 {
		t.Errorf("ErrorCount() = %d, want 0", report.ErrorCount())
	}
}

func TestSchemaMissingRequiredField(t *testing.T) {
	db, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	report := NewSchema().RequireString("does_not_exist").Validate(db)

	if report.IsValid() {
		t.Fatal("report.IsValid() = true for a database missing a required field")
	}
	if len(report.Errors) != 1 || report.Errors[0].Kind != ValidationMissing {
		t.Errorf("Errors = %+v, want exactly one ValidationMissing", report.Errors)
	}
}

func TestSchemaMissingOptionalFieldIsFine(t *testing.T) {
	db, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	report := NewSchema().OptionalString("does_not_exist").Validate(db)
	if !report.IsValid() {
		t.Errorf("report.IsValid() = false for a missing OPTIONAL field, errors: %v", report.Errors)
	}
}

func TestSchemaWrongType(t *testing.T) {
	db, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	// app_name is a String, not a Bool.
	report := NewSchema().RequireBool("app_name").Validate(db)
	if report.IsValid() {
		t.Fatal("report.IsValid() = true for a type mismatch")
	}
	if len(report.Errors) != 1 {
		t.Fatalf("len(Errors) = %d, want 1", len(report.Errors))
	}
	e := report.Errors[0]
	if e.Kind != ValidationWrongType || e.Expected != TypeBool || e.Actual != TypeString {
		t.Errorf("error = %+v, want WrongType Expected=Bool Actual=String", e)
	}
}

// TestSchemaIntLongAsymmetry is the direct test of the asymmetric
// matching rule documented on schemaTypeMatches: RequireLong must accept
// an actual Int (mdix_get_long widens losslessly), but RequireInt must
// NOT accept an actual Long (mdix_get_int has no such widening — calling
// GetInt on big_id would simply fail).
func TestSchemaIntLongAsymmetry(t *testing.T) {
	db, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	// port is a genuine Int (8080 fits comfortably in i32).
	if r := NewSchema().RequireLong("port").Validate(db); !r.IsValid() {
		t.Errorf("RequireLong(port) [Int-actual] should pass (widening), errors: %v", r.Errors)
	}

	// big_id is a genuine Long (overflows i32).
	if r := NewSchema().RequireInt("big_id").Validate(db); r.IsValid() {
		t.Error("RequireInt(big_id) [Long-actual] should fail — GetInt can't read it")
	}
	if r := NewSchema().RequireLong("big_id").Validate(db); !r.IsValid() {
		t.Errorf("RequireLong(big_id) [Long-actual] should pass, errors: %v", r.Errors)
	}
}

// TestSchemaFloatDoubleSymmetry checks the other half of the same rule:
// unlike Int/Long, Float and Double are fully symmetric at the FFI level
// (both mdix_get_float and mdix_get_double route through the same
// get::<f64>() call internally).
func TestSchemaFloatDoubleSymmetry(t *testing.T) {
	db, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	// ratio is a genuine Float (3.14f), pi is a genuine Double.
	if r := NewSchema().RequireDouble("ratio").Validate(db); !r.IsValid() {
		t.Errorf("RequireDouble(ratio) [Float-actual] should pass, errors: %v", r.Errors)
	}
	if r := NewSchema().RequireFloat("pi").Validate(db); !r.IsValid() {
		t.Errorf("RequireFloat(pi) [Double-actual] should pass, errors: %v", r.Errors)
	}
}

func TestSchemaFailedPathsAndFieldCount(t *testing.T) {
	db, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	schema := NewSchema().
		RequireString("app_name").
		RequireString("missing_a").
		RequireString("missing_b")

	if schema.FieldCount() != 3 {
		t.Errorf("FieldCount() = %d, want 3", schema.FieldCount())
	}

	report := schema.Validate(db)
	failed := report.FailedPaths()
	if len(failed) != 2 {
		t.Fatalf("FailedPaths() = %v, want 2 entries", failed)
	}
	if failed[0] != "missing_a" || failed[1] != "missing_b" {
		t.Errorf("FailedPaths() = %v, want [missing_a missing_b] in declaration order", failed)
	}
}

func TestSchemaIsReusableAcrossDatabases(t *testing.T) {
	schema := NewSchema().RequireString("app_name")

	dbGood, err := LoadStr(schemaFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer dbGood.Close()

	dbBad, err := LoadStr(`@DATA( other_field = 1 )`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer dbBad.Close()

	if !schema.Validate(dbGood).IsValid() {
		t.Error("schema should validate dbGood")
	}
	if schema.Validate(dbBad).IsValid() {
		t.Error("the same schema should NOT validate dbBad")
	}
}
