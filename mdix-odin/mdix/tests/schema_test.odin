package mdix_tests

import "core:testing"
import mdix "../"

SCHEMA_FIXTURE :: `
@DATA(
  app_name = "TestApp"
  port     = 8080
  big_id   = 9_000_000_000L
  ratio    = 3.14f
  pi       = 3.14159265358979
  debug    = true
)`

@(test)
schema_all_required_present :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(SCHEMA_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	s := mdix.schema_new()
	defer mdix.schema_destroy(&s)
	mdix.schema_require_string(&s, "app_name")
	mdix.schema_require_int(&s, "port")
	mdix.schema_require_bool(&s, "debug")

	report := mdix.schema_validate(s, db)
	defer mdix.validation_report_destroy(&report)
	testing.expect(t, mdix.validation_report_is_valid(report), "report should be valid")
}

@(test)
schema_missing_required_field :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(SCHEMA_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	s := mdix.schema_new()
	defer mdix.schema_destroy(&s)
	mdix.schema_require_string(&s, "does_not_exist")

	report := mdix.schema_validate(s, db)
	defer mdix.validation_report_destroy(&report)
	testing.expect(t, !mdix.validation_report_is_valid(report), "report should be invalid")
	testing.expect_value(t, len(report.errors), 1)
	testing.expect_value(t, report.errors[0].kind, mdix.Validation_Error_Kind.Missing)
}

@(test)
schema_missing_optional_field_is_fine :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(SCHEMA_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	s := mdix.schema_new()
	defer mdix.schema_destroy(&s)
	mdix.schema_optional_string(&s, "does_not_exist")

	report := mdix.schema_validate(s, db)
	defer mdix.validation_report_destroy(&report)
	testing.expect(t, mdix.validation_report_is_valid(report), "a missing OPTIONAL field should not invalidate the report")
}

@(test)
schema_wrong_type :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(SCHEMA_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	s := mdix.schema_new()
	defer mdix.schema_destroy(&s)
	mdix.schema_require_bool(&s, "app_name") // app_name is a String, not a Bool

	report := mdix.schema_validate(s, db)
	defer mdix.validation_report_destroy(&report)
	testing.expect(t, !mdix.validation_report_is_valid(report), "report should be invalid")
	testing.expect_value(t, report.errors[0].kind, mdix.Validation_Error_Kind.Wrong_Type)
	testing.expect_value(t, report.errors[0].expected, mdix.Mdix_Type.Bool)
	testing.expect_value(t, report.errors[0].actual, mdix.Mdix_Type.String)
}

@(test)
schema_int_long_asymmetry :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(SCHEMA_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	// port is a genuine Int; Long should accept it (widening).
	s1 := mdix.schema_new()
	defer mdix.schema_destroy(&s1)
	mdix.schema_require_long(&s1, "port")
	r1 := mdix.schema_validate(s1, db)
	defer mdix.validation_report_destroy(&r1)
	testing.expect(t, mdix.validation_report_is_valid(r1), "require_long against an actual Int should pass (widening)")

	// big_id is a genuine Long; Int should NOT accept it.
	s2 := mdix.schema_new()
	defer mdix.schema_destroy(&s2)
	mdix.schema_require_int(&s2, "big_id")
	r2 := mdix.schema_validate(s2, db)
	defer mdix.validation_report_destroy(&r2)
	testing.expect(t, !mdix.validation_report_is_valid(r2), "require_int against an actual Long should fail — get_int can't read it")

	s3 := mdix.schema_new()
	defer mdix.schema_destroy(&s3)
	mdix.schema_require_long(&s3, "big_id")
	r3 := mdix.schema_validate(s3, db)
	defer mdix.validation_report_destroy(&r3)
	testing.expect(t, mdix.validation_report_is_valid(r3), "require_long against an actual Long should pass")
}

@(test)
schema_float_double_symmetry :: proc(t: ^testing.T) {
	db, ok := mdix.load_str(SCHEMA_FIXTURE)
	testing.expect(t, ok, "load_str should succeed")
	defer mdix.destroy(&db)

	s1 := mdix.schema_new()
	defer mdix.schema_destroy(&s1)
	mdix.schema_require_double(&s1, "ratio") // ratio is a genuine Float (3.14f)
	r1 := mdix.schema_validate(s1, db)
	defer mdix.validation_report_destroy(&r1)
	testing.expect(t, mdix.validation_report_is_valid(r1), "require_double against an actual Float should pass")

	s2 := mdix.schema_new()
	defer mdix.schema_destroy(&s2)
	mdix.schema_require_float(&s2, "pi") // pi is a genuine Double
	r2 := mdix.schema_validate(s2, db)
	defer mdix.validation_report_destroy(&r2)
	testing.expect(t, mdix.validation_report_is_valid(r2), "require_float against an actual Double should pass")
}

@(test)
schema_is_reusable_across_databases :: proc(t: ^testing.T) {
	s := mdix.schema_new()
	defer mdix.schema_destroy(&s)
	mdix.schema_require_string(&s, "app_name")

	db_good, ok1 := mdix.load_str(SCHEMA_FIXTURE)
	testing.expect(t, ok1, "load_str should succeed")
	defer mdix.destroy(&db_good)

	db_bad, ok2 := mdix.load_str(`@DATA( other_field = 1 )`)
	testing.expect(t, ok2, "load_str should succeed")
	defer mdix.destroy(&db_bad)

	r_good := mdix.schema_validate(s, db_good)
	defer mdix.validation_report_destroy(&r_good)
	testing.expect(t, mdix.validation_report_is_valid(r_good), "schema should validate db_good")

	r_bad := mdix.schema_validate(s, db_bad)
	defer mdix.validation_report_destroy(&r_bad)
	testing.expect(t, !mdix.validation_report_is_valid(r_bad), "the same schema should NOT validate db_bad")
}
