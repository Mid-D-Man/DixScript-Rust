package mdix_tests

import "core:testing"
import mdix "../"

@(test)
hex_color_rrggbb :: proc(t: ^testing.T) {
	c, ok := mdix.parse_hex_color("#FF5733")
	testing.expect(t, ok, "expected #FF5733 to parse")
	testing.expect_value(t, c.r, f32(0xFF) / 255.0)
	testing.expect_value(t, c.g, f32(0x57) / 255.0)
	testing.expect_value(t, c.b, f32(0x33) / 255.0)
	testing.expect_value(t, c.a, f32(1))
}

@(test)
hex_color_rgb_shorthand :: proc(t: ^testing.T) {
	// CSS-style shorthand: each nibble N expands to NN, e.g. F -> FF.
	c, ok := mdix.parse_hex_color("#F00")
	testing.expect(t, ok, "expected #F00 to parse")
	testing.expect_value(t, c.r, f32(1))
	testing.expect_value(t, c.g, f32(0))
	testing.expect_value(t, c.b, f32(0))
}

@(test)
hex_color_rrggbbaa :: proc(t: ^testing.T) {
	c, ok := mdix.parse_hex_color("#FF573380")
	testing.expect(t, ok, "expected #FF573380 to parse")
	testing.expect_value(t, c.a, f32(0x80) / 255.0)
}

@(test)
hex_color_no_hash_prefix :: proc(t: ^testing.T) {
	c, ok := mdix.parse_hex_color("FF5733")
	testing.expect(t, ok, "hex color should parse without a leading #")
	testing.expect_value(t, c.r, f32(0xFF) / 255.0)
}

@(test)
hex_color_invalid_rejected :: proc(t: ^testing.T) {
	_, ok := mdix.parse_hex_color("#ZZZZZZ")
	testing.expect(t, !ok, "non-hex characters should fail to parse")

	_, ok2 := mdix.parse_hex_color("#FF57")
	testing.expect(t, !ok2, "4-digit hex (not 3/6/8) should fail to parse")
}

@(test)
date_parses :: proc(t: ^testing.T) {
	d, ok := mdix.parse_mdix_date("2025-06-15")
	testing.expect(t, ok, "expected 2025-06-15 to parse")
	testing.expect_value(t, d.raw, "2025-06-15")
}

@(test)
date_rejects_bad_format :: proc(t: ^testing.T) {
	_, ok := mdix.parse_mdix_date("15-06-2025")
	testing.expect(t, !ok, "DD-MM-YYYY should be rejected — only YYYY-MM-DD is valid")

	_, ok2 := mdix.parse_mdix_date("2025/06/15")
	testing.expect(t, !ok2, "slash-separated date should be rejected")

	_, ok3 := mdix.parse_mdix_date("2025-13-01")
	testing.expect(t, !ok3, "month 13 should be rejected")
}

@(test)
timestamp_parses_with_fraction :: proc(t: ^testing.T) {
	ts, ok := mdix.parse_mdix_timestamp("2025-06-15T09:30:00.500Z")
	testing.expect(t, ok, "expected timestamp with fractional seconds to parse")
	testing.expect_value(t, ts.raw, "2025-06-15T09:30:00.500Z")
}

@(test)
timestamp_parses_without_fraction :: proc(t: ^testing.T) {
	_, ok := mdix.parse_mdix_timestamp("2025-06-15T09:30:00Z")
	testing.expect(t, ok, "expected timestamp without fractional seconds to parse")
}

@(test)
timestamp_rejects_bad_format :: proc(t: ^testing.T) {
	_, ok := mdix.parse_mdix_timestamp("not a timestamp")
	testing.expect(t, !ok, "garbage input should be rejected")
}
