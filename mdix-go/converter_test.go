package dixscript

import "testing"

func TestConverterToJSONAndFromJSON(t *testing.T) {
	db, err := LoadStr(`@DATA( name = "Widget", count = 7 )`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	jsonStr, err := Convert.ToJSON(db, false)
	if err != nil {
		t.Fatalf("ToJSON: %v", err)
	}
	if jsonStr == "" {
		t.Fatal("ToJSON returned an empty string")
	}

	roundTripped, err := Convert.FromJSON(jsonStr)
	if err != nil {
		t.Fatalf("FromJSON: %v", err)
	}
	defer roundTripped.Close()

	name, err := roundTripped.GetString("name")
	if err != nil || name != "Widget" {
		t.Errorf("GetString(name) after JSON round trip = %q, %v; want \"Widget\", nil", name, err)
	}
	count, err := roundTripped.GetInt("count")
	if err != nil || count != 7 {
		t.Errorf("GetInt(count) after JSON round trip = %d, %v; want 7, nil", count, err)
	}
}

func TestConverterJSONRoundTripHelper(t *testing.T) {
	db, err := LoadStr(`@DATA( name = "Widget" )`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	roundTripped, err := Convert.JSONRoundTrip(db)
	if err != nil {
		t.Fatalf("JSONRoundTrip: %v", err)
	}
	defer roundTripped.Close()

	if name, err := roundTripped.GetString("name"); err != nil || name != "Widget" {
		t.Errorf("GetString(name) after JSONRoundTrip = %q, %v; want \"Widget\", nil", name, err)
	}
}

func TestConverterToToml(t *testing.T) {
	db, err := LoadStr(`@DATA( name = "Widget", count = 7 )`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	toml, err := Convert.ToToml(db)
	if err != nil {
		t.Fatalf("ToToml: %v", err)
	}
	if toml == "" {
		t.Error("ToToml returned an empty string")
	}
}

func TestConverterFormatAndMinifySource(t *testing.T) {
	const src = `@DATA(   name="Widget"   ,   count=7   )`

	formatted, err := Convert.FormatSource(src, FormatPretty)
	if err != nil {
		t.Fatalf("FormatSource: %v", err)
	}
	if formatted == "" {
		t.Error("FormatSource returned an empty string")
	}

	minified, err := Convert.MinifySource(src)
	if err != nil {
		t.Fatalf("MinifySource: %v", err)
	}
	if minified == "" {
		t.Error("MinifySource returned an empty string")
	}

	// Both must still parse as valid DixScript — the point of formatting
	// is to change whitespace/layout, not the data.
	if _, err := LoadStr(formatted); err != nil {
		t.Errorf("formatted source failed to reload: %v", err)
	}
	if _, err := LoadStr(minified); err != nil {
		t.Errorf("minified source failed to reload: %v", err)
	}
}

func TestConverterOnClosedDatabaseReturnsError(t *testing.T) {
	db, err := LoadStr(`@DATA( name = "Widget" )`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	db.Close()

	if _, err := Convert.ToJSON(db, false); err == nil {
		t.Error("ToJSON on a closed database returned nil error")
	}
}

func TestConverterOnNilDatabaseReturnsError(t *testing.T) {
	if _, err := Convert.ToJSON(nil, false); err == nil {
		t.Error("ToJSON(nil, ...) returned nil error, want ErrNullHandle")
	}
}
