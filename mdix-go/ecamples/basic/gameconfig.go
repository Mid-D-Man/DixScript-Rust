package main

import (
	"fmt"
	"log"

	dixscript "github.com/Mid-D-Man/dixscript-go"
)

func main() {
	// ── Load from string ──────────────────────────────────────────────────────
	const src = `
@DATA(
  app_name = "DemoApp"
  version  = 2
  debug    = false
  pi       = 3.14159
  server: host = "localhost", port = 8080, ssl = true
  tags:: "go", "config", "fast"
)
`
	db, err := dixscript.LoadStr(src)
	if err != nil {
		log.Fatalf("load: %v", err)
	}
	defer db.Close()

	// ── Read scalars ──────────────────────────────────────────────────────────
	appName, _ := db.GetString("app_name")
	version, _ := db.GetInt("version")
	debug, _   := db.GetBool("debug")
	pi, _      := db.GetFloat64("pi")

	fmt.Printf("app:     %s v%d  debug=%v  pi=%.5f\n", appName, version, debug, pi)

	// ── Read nested (table property) ─────────────────────────────────────────
	host, _ := db.GetString("server.host")
	port, _ := db.GetInt("server.port")
	ssl, _  := db.GetBool("server.ssl")

	fmt.Printf("server:  %s:%d  ssl=%v\n", host, port, ssl)

	// ── ValueType inspection ──────────────────────────────────────────────────
	fmt.Printf("type of 'version': %s\n", db.ValueTypeAt("version"))
	fmt.Printf("type of 'tags':    %s\n", db.ValueTypeAt("tags"))
	fmt.Printf("exists 'nope':     %v\n", db.Exists("nope"))

	// ── Array length ──────────────────────────────────────────────────────────
	tagCount, _ := db.ArrayLength("tags")
	fmt.Printf("tag count: %d\n", tagCount)

	// ── Top-level keys ────────────────────────────────────────────────────────
	keys, _ := db.Keys("")
	fmt.Printf("top-level keys: %v\n", keys)

	// ── Builder ───────────────────────────────────────────────────────────────
	b := dixscript.NewBuilder()
	defer b.Close()

	_ = b.SetString("profile.name", "player1")
	_ = b.SetInt("profile.level", 42)
	_ = b.SetFloat64("profile.score", 9876.5)
	_ = b.SetBool("profile.active", true)

	mdixStr, err := b.ToString()
	if err != nil {
		log.Fatalf("builder: %v", err)
	}
	fmt.Printf("\nBuilt .mdix:\n%s\n", mdixStr)

	// ── Conversion ────────────────────────────────────────────────────────────
	jsonStr, err := dixscript.Convert.ToJSON(db, true)
	if err != nil {
		log.Fatalf("toJSON: %v", err)
	}
	fmt.Printf("JSON export (first 120 chars):\n%.120s...\n", jsonStr)

	fmt.Printf("\nDixScript native version: %s\n", dixscript.Version())
}
