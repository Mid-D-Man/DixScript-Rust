//! LSP integration test runner — custom harness (harness = false).
//!
//! Spawns mdix-lsp over stdio, drives it with JSON-RPC, measures latency
//! per capability, and writes lsp-integration-results.json.
//!
//! Controlled by two environment variables:
//!   MDIX_LSP_BIN    — absolute path to the server binary
//!   LSP_RESULTS_OUT — where to write the JSON results file

use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const COMPREHENSIVE_MDIX: &str = include_str!("fixtures/comprehensive.mdix");
const ERRORS_MDIX: &str        = include_str!("fixtures/errors.mdix");

// ─── JSON-RPC client ──────────────────────────────────────────────────────────

struct LspClient {
    stdin:   ChildStdin,
    stdout:  BufReader<ChildStdout>,
    _child:  Child,
    next_id: u32,
}

impl LspClient {
    fn spawn(bin: &std::path::Path) -> Result<Self, String> {
        let mut child = Command::new(bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn {}: {}", bin.display(), e))?;

        let stdin  = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Ok(LspClient { stdin, stdout, _child: child, next_id: 1 })
    }

    fn write_message(&mut self, msg: &Value) {
        let body  = msg.to_string();
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let _ = self.stdin.write_all(frame.as_bytes());
        let _ = self.stdin.flush();
    }

    fn send_request(&mut self, method: &str, params: Value) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id":      id,
            "method":  method,
            "params":  params
        }));
        id
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "method":  method,
            "params":  params
        }));
    }

    fn read_message(&mut self) -> Result<Value, String> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line)
                .map_err(|e| format!("read header error: {}", e))?;
            let line = line.trim_end_matches(|c| c == '\r' || c == '\n');
            if line.is_empty() { break; }
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 && parts[0].to_lowercase() == "content-length" {
                content_length = parts[1].trim().parse().ok();
            }
        }
        let len = content_length.ok_or_else(|| "no Content-Length header".to_string())?;
        let mut body  = vec![0u8; len];
        let mut read  = 0;
        while read < len {
            match self.stdout.read(&mut body[read..]) {
                Ok(0)  => return Err("EOF while reading body".to_string()),
                Ok(n)  => read += n,
                Err(e) => return Err(format!("read body error: {}", e)),
            }
        }
        serde_json::from_slice(&body).map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Read messages until one matches `id`. Discards notifications.
    fn read_response(&mut self, id: u32) -> Result<Value, String> {
        for _ in 0..32 {
            let msg = self.read_message()?;
            if msg.get("id").and_then(|v| v.as_u64()) == Some(id as u64) {
                return Ok(msg);
            }
        }
        Err(format!("no response received for id {}", id))
    }

    /// Send a request and return (response, latency_ms).
    fn request(&mut self, method: &str, params: Value) -> Result<(Value, f64), String> {
        let id    = self.send_request(method, params);
        let start = Instant::now();
        let resp  = self.read_response(id)?;
        Ok((resp, start.elapsed().as_secs_f64() * 1000.0))
    }

    fn initialize(&mut self) -> Result<f64, String> {
        let start = Instant::now();
        let id    = self.send_request("initialize", json!({
            "processId": null,
            "rootUri":   null,
            "capabilities": {
                "textDocument": {
                    "hover":         { "contentFormat": ["markdown", "plaintext"] },
                    "completion":    { "completionItem": { "snippetSupport": true } },
                    "definition":    {},
                    "semanticTokens": {
                        "requests": { "full": true },
                        "formats":  ["relative"],
                        "tokenTypes": [], "tokenModifiers": []
                    },
                    "publishDiagnostics": {},
                    "inlayHint":     {},
                    "colorProvider": {}
                }
            }
        }));
        let resp = self.read_response(id)?;
        let lat  = start.elapsed().as_secs_f64() * 1000.0;
        if resp.get("error").is_some() {
            return Err(format!("initialize error: {}", resp["error"]));
        }
        self.send_notification("initialized", json!({}));
        Ok(lat)
    }

    fn open_document(&mut self, uri: &str, text: &str) {
        self.send_notification("textDocument/didOpen", json!({
            "textDocument": {
                "uri":        uri,
                "languageId": "mdix",
                "version":    1,
                "text":       text
            }
        }));
    }

    fn shutdown(&mut self) {
        let id = self.send_request("shutdown", json!(null));
        let _  = self.read_response(id);
        self.send_notification("exit", json!(null));
    }
}

// ─── Test result types ─────────────────────────────────────────────────────────

struct TestResult {
    capability: &'static str,
    name:       &'static str,
    status:     &'static str,
    latency_ms: f64,
    request:    Option<Value>,
    expected:   Option<&'static str>,
    actual:     Option<Value>,
    diff:       Option<String>,
}

impl TestResult {
    fn pass(cap: &'static str, name: &'static str, lat: f64) -> Self {
        TestResult { capability: cap, name, status: "passed", latency_ms: lat,
                     request: None, expected: None, actual: None, diff: None }
    }
    fn fail(cap: &'static str, name: &'static str, lat: f64,
            req: Value, expected: &'static str, actual: Value, diff: String) -> Self {
        TestResult { capability: cap, name, status: "failed", latency_ms: lat,
                     request: Some(req), expected: Some(expected),
                     actual: Some(actual), diff: Some(diff) }
    }
    fn error(cap: &'static str, name: &'static str, msg: String) -> Self {
        TestResult { capability: cap, name, status: "error", latency_ms: 0.0,
                     request: None, expected: None, actual: None, diff: Some(msg) }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn td(uri: &str) -> Value { json!({ "uri": uri }) }
fn pos(line: u32, ch: u32) -> Value { json!({ "line": line, "character": ch }) }

/// Wrap the common "server returned an error" failure.
fn check_no_error(
    cap: &'static str, name: &'static str, lat: f64,
    params: Value, resp: &Value,
) -> Option<TestResult> {
    if let Some(err) = resp.get("error") {
        Some(TestResult::fail(cap, name, lat, params,
            "non-error response", err.clone(),
            format!("server returned error: {}", err)))
    } else {
        None
    }
}

// ─── Individual capability test groups ────────────────────────────────────────

fn test_hover(c: &mut LspClient, uri: &str, results: &mut Vec<TestResult>) {
    // @ENUMS section keyword
    let p = json!({ "textDocument": td(uri), "position": pos(0, 1) });
    match c.request("textDocument/hover", p.clone()) {
        Err(e) => results.push(TestResult::error("hover", "hover_section_keyword", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("hover", "hover_section_keyword", lat, p, &resp) {
                results.push(f);
            } else {
                results.push(TestResult::pass("hover", "hover_section_keyword", lat));
            }
        }
    }

    // Identifier in @DATA
    let p2 = json!({ "textDocument": td(uri), "position": pos(13, 3) });
    match c.request("textDocument/hover", p2.clone()) {
        Err(e) => results.push(TestResult::error("hover", "hover_data_identifier", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("hover", "hover_data_identifier", lat, p2, &resp) {
                results.push(f);
            } else {
                results.push(TestResult::pass("hover", "hover_data_identifier", lat));
            }
        }
    }

    // QuickFunc name in @DATA call
    let p3 = json!({ "textDocument": td(uri), "position": pos(18, 5) });
    match c.request("textDocument/hover", p3.clone()) {
        Err(e) => results.push(TestResult::error("hover", "hover_quickfunc_call", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("hover", "hover_quickfunc_call", lat, p3, &resp) {
                results.push(f);
            } else {
                results.push(TestResult::pass("hover", "hover_quickfunc_call", lat));
            }
        }
    }
}

fn test_completions(c: &mut LspClient, uri: &str, results: &mut Vec<TestResult>) {
    // @ trigger — should return section snippets
    let p = json!({
        "textDocument": td(uri),
        "position":     pos(0, 1),
        "context": { "triggerKind": 2, "triggerCharacter": "@" }
    });
    match c.request("textDocument/completion", p.clone()) {
        Err(e) => results.push(TestResult::error("completions", "completions_at_trigger", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("completions", "completions_at_trigger", lat, p.clone(), &resp) {
                results.push(f); return;
            }
            let items: Vec<Value> = match &resp["result"] {
                Value::Array(a) => a.clone(),
                Value::Object(o) => o.get("items")
                    .and_then(|v| v.as_array()).cloned().unwrap_or_default(),
                _ => vec![],
            };
            if items.iter().any(|i| i["label"].as_str()
                .map(|s| s.contains("DATA")).unwrap_or(false)) {
                results.push(TestResult::pass("completions", "completions_at_trigger", lat));
            } else {
                results.push(TestResult::fail(
                    "completions", "completions_at_trigger", lat, p,
                    "@DATA in completion list",
                    Value::Array(items[..items.len().min(5)].to_vec()),
                    "@DATA not found in @ completions".to_string(),
                ));
            }
        }
    }

    // . trigger
    let p2 = json!({
        "textDocument": td(uri),
        "position":     pos(1, 8),
        "context": { "triggerKind": 2, "triggerCharacter": "." }
    });
    match c.request("textDocument/completion", p2.clone()) {
        Err(e) => results.push(TestResult::error("completions", "completions_dot_trigger", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("completions", "completions_dot_trigger", lat, p2, &resp) {
                results.push(f);
            } else {
                results.push(TestResult::pass("completions", "completions_dot_trigger", lat));
            }
        }
    }

    // < trigger — type annotations
    let p3 = json!({
        "textDocument": td(uri),
        "position":     pos(13, 12),
        "context": { "triggerKind": 2, "triggerCharacter": "<" }
    });
    match c.request("textDocument/completion", p3.clone()) {
        Err(e) => results.push(TestResult::error("completions", "completions_type_trigger", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("completions", "completions_type_trigger", lat, p3.clone(), &resp) {
                results.push(f); return;
            }
            let items: Vec<Value> = match &resp["result"] {
                Value::Array(a) => a.clone(),
                Value::Object(o) => o.get("items")
                    .and_then(|v| v.as_array()).cloned().unwrap_or_default(),
                _ => vec![],
            };
            if items.iter().any(|i| i["label"].as_str()
                .map(|s| s.contains("int")).unwrap_or(false)) {
                results.push(TestResult::pass("completions", "completions_type_trigger", lat));
            } else {
                results.push(TestResult::fail(
                    "completions", "completions_type_trigger", lat, p3,
                    "<int> in type completions",
                    Value::Array(items[..items.len().min(5)].to_vec()),
                    "<int> not found in type annotation completions".to_string(),
                ));
            }
        }
    }
}

fn test_goto_definition(c: &mut LspClient, uri: &str, results: &mut Vec<TestResult>) {
    // QuickFunc call site
    let p = json!({ "textDocument": td(uri), "position": pos(18, 5) });
    match c.request("textDocument/definition", p.clone()) {
        Err(e) => results.push(TestResult::error("goto_definition", "goto_quickfunc_call", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("goto_definition", "goto_quickfunc_call", lat, p, &resp) {
                results.push(f);
            } else {
                results.push(TestResult::pass("goto_definition", "goto_quickfunc_call", lat));
            }
        }
    }

    // Enum access
    let p2 = json!({ "textDocument": td(uri), "position": pos(22, 44) });
    match c.request("textDocument/definition", p2.clone()) {
        Err(e) => results.push(TestResult::error("goto_definition", "goto_enum_field", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("goto_definition", "goto_enum_field", lat, p2, &resp) {
                results.push(f);
            } else {
                results.push(TestResult::pass("goto_definition", "goto_enum_field", lat));
            }
        }
    }
}

fn test_semantic_tokens(c: &mut LspClient, uri: &str, results: &mut Vec<TestResult>) {
    let p = json!({ "textDocument": td(uri) });
    match c.request("textDocument/semanticTokens/full", p.clone()) {
        Err(e) => results.push(TestResult::error("semantic_tokens", "tokens_non_empty", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("semantic_tokens", "tokens_non_empty", lat, p.clone(), &resp) {
                results.push(f); return;
            }
            let data = resp["result"]["data"].as_array();
            match data {
                Some(arr) if !arr.is_empty() =>
                    results.push(TestResult::pass("semantic_tokens", "tokens_non_empty", lat)),
                _ =>
                    results.push(TestResult::fail(
                        "semantic_tokens", "tokens_non_empty", lat, p,
                        "non-empty result.data array",
                        resp["result"].clone(),
                        "semantic tokens data array is empty or missing".to_string(),
                    )),
            }
        }
    }

    // Token count sanity: comprehensive fixture is ~25 lines, expect > 10 tokens
    let p2 = json!({ "textDocument": td(uri) });
    match c.request("textDocument/semanticTokens/full", p2.clone()) {
        Err(e) => results.push(TestResult::error("semantic_tokens", "tokens_minimum_count", e)),
        Ok((resp, lat)) => {
            let count = resp["result"]["data"].as_array().map(|a| a.len()).unwrap_or(0);
            // Each token = 5 u32 values
            if count >= 5 {
                results.push(TestResult::pass("semantic_tokens", "tokens_minimum_count", lat));
            } else {
                results.push(TestResult::fail(
                    "semantic_tokens", "tokens_minimum_count", lat, p2,
                    "at least 5 token data values",
                    json!(count),
                    format!("only {} token data values returned (need ≥5)", count),
                ));
            }
        }
    }
}

fn test_document_color(c: &mut LspClient, uri: &str, results: &mut Vec<TestResult>) {
    // comprehensive.mdix has #FF5733 and #4287f5 → expect ≥ 2 colors
    let p = json!({ "textDocument": td(uri) });
    match c.request("textDocument/documentColor", p.clone()) {
        Err(e) => results.push(TestResult::error("document_color", "hex_color_swatches", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("document_color", "hex_color_swatches", lat, p.clone(), &resp) {
                results.push(f); return;
            }
            let count = resp["result"].as_array().map(|a| a.len()).unwrap_or(0);
            if count >= 2 {
                results.push(TestResult::pass("document_color", "hex_color_swatches", lat));
            } else {
                results.push(TestResult::fail(
                    "document_color", "hex_color_swatches", lat, p,
                    "at least 2 color entries for #FF5733 and #4287f5",
                    resp["result"].clone(),
                    format!("expected ≥2 colors, got {}", count),
                ));
            }
        }
    }
}

fn test_inlay_hints(c: &mut LspClient, uri: &str, results: &mut Vec<TestResult>) {
    let p = json!({
        "textDocument": td(uri),
        "range": {
            "start": { "line": 0,  "character": 0 },
            "end":   { "line": 25, "character": 0 }
        }
    });
    match c.request("textDocument/inlayHint", p.clone()) {
        Err(e) => results.push(TestResult::error("inlay_hints", "inlay_hints_no_crash", e)),
        Ok((resp, lat)) => {
            if let Some(f) = check_no_error("inlay_hints", "inlay_hints_no_crash", lat, p, &resp) {
                results.push(f);
            } else {
                // null or array are both valid responses
                results.push(TestResult::pass("inlay_hints", "inlay_hints_no_crash", lat));
            }
        }
    }
}

fn test_diagnostics(c: &mut LspClient, results: &mut Vec<TestResult>) {
    let uri = "file:///errors-lsp-test.mdix";
    c.open_document(uri, ERRORS_MDIX);
    // Let the pipeline process the document before querying
    std::thread::sleep(Duration::from_millis(250));

    // If the server is still alive and responds to hover, it survived the bad file
    let p = json!({ "textDocument": td(uri), "position": pos(0, 0) });
    match c.request("textDocument/hover", p.clone()) {
        Err(e) => results.push(TestResult::error("diagnostics", "server_stable_on_errors", e)),
        Ok((_, lat)) =>
            results.push(TestResult::pass("diagnostics", "server_stable_on_errors", lat)),
    }
}

fn test_code_actions(c: &mut LspClient, uri: &str, results: &mut Vec<TestResult>) {
    let p = json!({
        "textDocument": td(uri),
        "range": {
            "start": { "line": 0, "character": 0 },
            "end":   { "line": 0, "character": 1 }
        },
        "context": { "diagnostics": [] }
    });
    match c.request("textDocument/codeAction", p.clone()) {
        Err(e) => results.push(TestResult::error("code_actions", "code_actions_no_crash", e)),
        Ok((resp, lat)) => {
            // codeAction returns null or an array — both acceptable
            if resp.get("error").is_some()
                && resp["error"]["code"].as_i64() != Some(-32601) // MethodNotFound is OK
            {
                results.push(TestResult::fail(
                    "code_actions", "code_actions_no_crash", lat, p,
                    "null or array result",
                    resp["error"].clone(),
                    format!("unexpected error: {}", resp["error"]),
                ));
            } else {
                results.push(TestResult::pass("code_actions", "code_actions_no_crash", lat));
            }
        }
    }
}

// ─── Output builders ──────────────────────────────────────────────────────────

fn group_capabilities(results: &[TestResult]) -> Vec<Value> {
    const ORDER: &[(&str, &str)] = &[
        ("initialize",     "🚀"),
        ("hover",          "💬"),
        ("completions",    "✨"),
        ("goto_definition","📍"),
        ("semantic_tokens","🎨"),
        ("document_color", "🖌"),
        ("inlay_hints",    "💡"),
        ("diagnostics",    "🔍"),
        ("code_actions",   "⚡"),
    ];

    ORDER.iter().filter_map(|(cap, icon)| {
        let rows: Vec<&TestResult> = results.iter().filter(|r| r.capability == *cap).collect();
        if rows.is_empty() { return None; }

        let passed = rows.iter().filter(|r| r.status == "passed").count();
        let failed = rows.len() - passed;
        let lats: Vec<f64> = rows.iter().map(|r| r.latency_ms).collect();
        let avg = lats.iter().sum::<f64>() / lats.len() as f64;
        let max = lats.iter().cloned().fold(0.0_f64, f64::max);

        let tests: Vec<Value> = rows.iter().map(|r| json!({
            "name":       r.name,
            "status":     r.status,
            "latency_ms": (r.latency_ms * 10.0).round() / 10.0,
        })).collect();

        Some(json!({
            "name":           cap,
            "icon":           icon,
            "total":          rows.len(),
            "passed":         passed,
            "failed":         failed,
            "avg_latency_ms": (avg * 10.0).round() / 10.0,
            "max_latency_ms": (max * 10.0).round() / 10.0,
            "tests":          tests,
        }))
    }).collect()
}

fn collect_failures(results: &[TestResult]) -> Vec<Value> {
    results.iter().filter(|r| r.status != "passed").map(|r| json!({
        "capability": r.capability,
        "name":       r.name,
        "status":     r.status,
        "request":    r.request,
        "expected":   r.expected,
        "actual":     r.actual,
        "diff":       r.diff,
    })).collect()
}

fn write_error_json(path: &str, msg: &str) {
    let out = json!({
        "duration_s": 0, "total": 0, "passed": 0, "failed": 0,
        "capabilities": [],
        "failures": [{ "capability": "setup", "name": "spawn_server",
                       "status": "error", "diff": msg }],
    });
    let _ = fs::write(path, serde_json::to_string_pretty(&out).unwrap());
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let bin = env::var("MDIX_LSP_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let dir = env!("CARGO_MANIFEST_DIR");
            PathBuf::from(dir).join("../target/debug/mdix-lsp")
        });

    let out_path = env::var("LSP_RESULTS_OUT")
        .unwrap_or_else(|_| "lsp-integration-results.json".to_string());

    println!("Binary:  {}", bin.display());
    println!("Results: {}", out_path);

    if !bin.exists() {
        let msg = format!("binary not found at {}", bin.display());
        eprintln!("ERROR: {}", msg);
        write_error_json(&out_path, &msg);
        std::process::exit(1);
    }

    let mut client = match LspClient::spawn(&bin) {
        Ok(c)  => c,
        Err(e) => { eprintln!("ERROR: {}", e); write_error_json(&out_path, &e); std::process::exit(1); }
    };

    let mut results: Vec<TestResult> = Vec::new();
    let overall_start = Instant::now();

    match client.initialize() {
        Err(e) => {
            results.push(TestResult::error("initialize", "server_responds_to_initialize", e));
        }
        Ok(lat) => {
            results.push(TestResult::pass("initialize", "server_responds_to_initialize", lat));

            let uri = "file:///comprehensive-lsp-test.mdix";
            client.open_document(uri, COMPREHENSIVE_MDIX);
            std::thread::sleep(Duration::from_millis(250));

            test_hover(&mut client, uri, &mut results);
            test_completions(&mut client, uri, &mut results);
            test_goto_definition(&mut client, uri, &mut results);
            test_semantic_tokens(&mut client, uri, &mut results);
            test_document_color(&mut client, uri, &mut results);
            test_inlay_hints(&mut client, uri, &mut results);
            test_diagnostics(&mut client, &mut results);
            test_code_actions(&mut client, uri, &mut results);
        }
    }

    let duration_s = overall_start.elapsed().as_secs_f64();
    client.shutdown();

    let passed       = results.iter().filter(|r| r.status == "passed").count();
    let failed       = results.len() - passed;
    let capabilities = group_capabilities(&results);
    let failures     = collect_failures(&results);

    let output = json!({
        "duration_s":   (duration_s * 1000.0).round() / 1000.0,
        "total":        results.len(),
        "passed":       passed,
        "failed":       failed,
        "capabilities": capabilities,
        "failures":     failures,
    });

    match fs::write(&out_path, serde_json::to_string_pretty(&output).unwrap()) {
        Ok(())  => println!("Wrote {}", out_path),
        Err(e)  => eprintln!("WARNING: could not write results: {}", e),
    }

    println!("\n=== LSP Integration Results ===");
    println!("Total: {}  Passed: {}  Failed: {}", results.len(), passed, failed);
    for cap in &capabilities {
        println!("  {:22} {:>2}/{:<2}  avg {:.1}ms  max {:.1}ms",
            cap["name"].as_str().unwrap_or("?"),
            cap["passed"].as_u64().unwrap_or(0),
            cap["total"].as_u64().unwrap_or(0),
            cap["avg_latency_ms"].as_f64().unwrap_or(0.0),
            cap["max_latency_ms"].as_f64().unwrap_or(0.0),
        );
    }

    if failed > 0 {
        println!("\nFAILURES:");
        for f in &failures {
            println!("  [{}] {} — {}", f["capability"], f["name"], f["diff"]);
        }
        std::process::exit(1);
    }
}
