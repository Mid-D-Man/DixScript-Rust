// tests/roundtrip.rs
//! Integration tests — every test must produce identical SVG output
//! after a source → binary → decode → render roundtrip.

use msx::{compile, decode, parse_scene, render, Scene};
use msx_ast::{Def, Element, LoopMode, MediaSource};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn normalise_svg(svg: &str) -> String {
    svg.split_whitespace().collect::<Vec<_>>().join(" ")
}

// f64 fields round-trip through an f32 wire format (see msx-binary's
// write_f32/read_f32), so exact equality is the wrong tool for them —
// mirrors apps/msx-cli's own `approx_eq` in cmd_roundtrip's
// `animations_match`, ported here because this file needs the same
// tolerance for the same reason and has no shared crate to pull it from.
fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-4 * a.abs().max(b.abs()).max(1.0)
}

fn find_def<'a>(scene: &'a Scene, id: &str) -> Option<&'a Def> {
    scene.defs.iter().find(|d| d.id() == id)
}

fn check_roundtrip(label: &str, source: &str) {
    let scene_a = parse_scene(source)
        .unwrap_or_else(|e| panic!("[{}] parse failed: {}", label, e));

    let svg_a = render(&scene_a);

    let binary = compile(&scene_a, true)
        .unwrap_or_else(|e| panic!("[{}] compile failed: {}", label, e));

    let scene_b = decode(&binary)
        .unwrap_or_else(|e| panic!("[{}] decode failed: {}", label, e));

    let svg_b = render(&scene_b);

    let na = normalise_svg(&svg_a);
    let nb = normalise_svg(&svg_b);

    if na != nb {
        let chars_a: Vec<char> = na.chars().collect();
        let chars_b: Vec<char> = nb.chars().collect();
        for (i, (a, b)) in chars_a.iter().zip(chars_b.iter()).enumerate() {
            if a != b {
                let ctx_a = &na[i.saturating_sub(30)..((i + 30).min(na.len()))];
                let ctx_b = &nb[i.saturating_sub(30)..((i + 30).min(nb.len()))];
                panic!("[{}] SVG mismatch at char {}: {:?} vs {:?}", label, i, ctx_a, ctx_b);
            }
        }
        if na.len() != nb.len() {
            panic!("[{}] SVG length mismatch: {} vs {}", label, na.len(), nb.len());
        }
    }

    let ratio = binary.len() as f64 / svg_a.len() as f64 * 100.0;
    println!("[{}] PASS — {} elements, {}B binary, {}B svg ({:.1}%)",
        label, scene_a.element_count(), binary.len(), svg_a.len(), ratio);
}

// ── Basic geometry ────────────────────────────────────────────────────────────

#[test]
fn roundtrip_circle() {
    check_roundtrip("circle", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 200, height = 200, background = #ffffff }
  elements::
    { type = "circle", cx = 100, cy = 100, r = 50,
      style = { fill = #ff0000, stroke = "none", stroke_width = 0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_rect_rounded() {
    check_roundtrip("rect_rounded", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 300, background = #1a1a2e }
  elements::
    { type = "rect", x = 20, y = 20, width = 200, height = 120, rx = 12,
      style = { fill = #0f3460, stroke = #4a9eff, stroke_width = 2.0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_ellipse() {
    check_roundtrip("ellipse", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 500, height = 300, background = #ffffff }
  elements::
    { type = "ellipse", cx = 250, cy = 150, rx = 180, ry = 80,
      style = { fill = #a78bfa, stroke = #7c3aed, stroke_width = 3.0, opacity = 0.9 } }
)
"#);
}

#[test]
fn roundtrip_line() {
    check_roundtrip("line", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 400, background = #000000 }
  elements::
    { type = "line", x1 = 10, y1 = 10, x2 = 390, y2 = 390,
      style = { fill = "none", stroke = #f5a623, stroke_width = 4.0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_path_triangle() {
    check_roundtrip("path_triangle", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 500, height = 500, background = #0d1117 }
  elements::
    { type = "path", d = "M 50 450 L 250 50 L 450 450 Z",
      style = { fill = #3498db, stroke = #2980b9, stroke_width = 3.0, opacity = 0.9 } }
)
"#);
}

#[test]
fn roundtrip_path_cubic_bezier() {
    check_roundtrip("path_cubic", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 500, height = 500, background = #0d1117 }
  elements::
    { type = "path",
      d = "M 250 120 C 340 60 440 140 420 240 C 400 340 300 420 210 390 C 120 360 80 260 100 180 C 120 100 160 180 250 120 Z",
      style = { fill = "none", stroke = #e74c3c, stroke_width = 4.0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_path_arc() {
    check_roundtrip("path_arc", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 500, height = 500, background = #0d1117 }
  elements::
    { type = "path", d = "M 120 380 A 130 130 0 0 1 380 380",
      style = { fill = "none", stroke = #f5a623, stroke_width = 5.0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_text() {
    check_roundtrip("text", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 100, background = #ffffff }
  elements::
    { type = "text", x = 200, y = 60, content = "Hello MSX",
      style = { fill = #000000, font_size = 24, text_anchor = "middle",
                stroke = "none", stroke_width = 0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_polyline() {
    check_roundtrip("polyline", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 300, height = 300, background = #ffffff }
  elements::
    { type = "polyline", points = [ [20, 20], [150, 250], [280, 20], [150, 150] ],
      style = { fill = "none", stroke = #16a34a, stroke_width = 3.0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_polygon() {
    check_roundtrip("polygon", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 300, height = 300, background = #111827 }
  elements::
    { type = "polygon", points = [ [150, 20], [280, 220], [20, 220] ],
      style = { fill = #fbbf24, stroke = #92400e, stroke_width = 4.0, opacity = 1.0 } }
)
"#);
}

#[test]
fn polygon_decodes_as_closed_polyline_not_as_polygon_variant() {
    // Found while adding the fixture above: `Element::Polygon` and
    // `Element::Polyline` both wrap the same `Polyline` struct — only
    // `closed` distinguishes them (see msx-ast::Element::tag_name() and
    // msx-render-svg::shapes::render_polyline's doc comment, which already
    // works around this). scene_decode.rs's `TAG_POLYLINE | TAG_POLYGON`
    // arm reconstructs `closed` correctly but always wraps the result in
    // `Element::Polyline`, never `Element::Polygon`, regardless of which
    // tag byte was actually read. Every current renderer already dispatches
    // on `.closed` rather than the variant, which is exactly why
    // `check_roundtrip`'s SVG-text comparison above can't see this — but
    // `Element::tag_name()` (or any future code matching `Element::Polygon`
    // on its own) would silently report "polyline" for a decoded polygon.
    // Pinned here so a future fix to scene_decode.rs is a deliberate
    // change, not a silent behavior shift either way.
    let source = r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 300, height = 300, background = #ffffff }
  elements::
    { type = "polygon", points = [ [150, 20], [280, 220], [20, 220] ],
      style = { fill = #fbbf24, stroke = #92400e, stroke_width = 4.0, opacity = 1.0 } }
)
"#;
    let scene_a = parse_scene(source).expect("[polygon_tag] parse");
    assert_eq!(scene_a.elements[0].tag_name(), "polygon");

    let binary  = compile(&scene_a, true).expect("[polygon_tag] compile");
    let scene_b = decode(&binary).expect("[polygon_tag] decode");

    match &scene_b.elements[0] {
        Element::Polyline(p) => assert!(p.closed, "decoded polygon should at least keep closed = true"),
        Element::Polygon(_) => panic!(
            "[polygon_tag] decode now preserves Element::Polygon directly — great, \
             but this test (and shapes.rs's doc comment) are now stale, update both"
        ),
        other => panic!("[polygon_tag] unexpected element: {:?}", other),
    }
}

// ── SDF (signed-distance-field) trees ────────────────────────────────────────
//
// NOTE: docs/format-spec.md's SDF section (from this session's own docs
// rewrite) doesn't match the real parser — cross-checked directly against
// core/msx-parser/src/sdf.rs while writing these fixtures. Real syntax used
// below, not the doc's: `box` takes `x,y,width,height` (top-left anchor, not
// `cx,cy,hx,hy`); `arc` takes `angle_start,angle_end` (absolute, not
// `start_angle,sweep_angle`); `union`/`smooth_union` take a `children` array
// (not an `a`/`b` pair — that pair form is real, but only for
// `subtract`/`intersect`/`smooth_subtract`/`smooth_intersect`); `offset`
// takes `child` (singular, not `node`). Worth fixing the doc separately.

#[test]
fn roundtrip_sdf_subtract() {
    check_roundtrip("sdf_subtract", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 300, height = 300, background = #0d1117 }
  elements::
    { type = "sdf",
      tree = { type = "subtract",
        a = { type = "box", x = 60, y = 60, width = 180, height = 180, corner_radius = 20 },
        b = { type = "circle", cx = 150, cy = 150, r = 70 } },
      fill = #38bdf8 }
)
"#);
}

#[test]
fn roundtrip_sdf_smooth_union_variadic() {
    // Exercises the `children` array form specifically (as opposed to
    // subtract/intersect's 2-child `a`/`b` form above) — the two use
    // genuinely different parse paths in sdf.rs.
    check_roundtrip("sdf_smooth_union", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 300, height = 300, background = #ffffff }
  elements::
    { type = "sdf",
      tree = { type = "smooth_union", k = 20,
        children = [
          { type = "circle", cx = 110, cy = 150, r = 60 },
          { type = "circle", cx = 190, cy = 150, r = 60 },
          { type = "ring", cx = 150, cy = 90, r = 40, thickness = 10 }
        ] },
      fill = #ec4899, stroke = #831843, stroke_width = 2.0 }
)
"#);
}

// ── Splat ─────────────────────────────────────────────────────────────────────

#[test]
fn roundtrip_splat() {
    check_roundtrip("splat", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 200, height = 200, background = #000000 }
  elements::
    { type = "splat", x = 100, y = 100, sigma_x = 40, sigma_y = 25, rotation = 0.6,
      color = #22d3ee, opacity = 0.85 }
)
"#);
}

// ── Layer (blend modes + effects) ────────────────────────────────────────────

#[test]
fn roundtrip_layer_blend_and_clip() {
    let source = r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 300, height = 200, background = #1a1a2e }
  elements::
    { type = "rect", x = 40, y = 40, width = 120, height = 120,
      style = { fill = #ff0000, stroke = "none", stroke_width = 0, opacity = 1.0 } }
    { type = "layer", blend_mode = "multiply", opacity = 0.8, clip = true,
      effects = [ { type = "blur", radius = 6 } ],
      elements = [
        { type = "circle", cx = 150, cy = 100, r = 70,
          style = { fill = #00ff00, stroke = "none", stroke_width = 0, opacity = 1.0 } }
      ] }
)
"#;
    let scene_a = parse_scene(source).expect("[layer_blend_clip] parse");
    let svg_a   = render(&scene_a);
    let binary  = compile(&scene_a, true).expect("[layer_blend_clip] compile");
    let scene_b = decode(&binary).expect("[layer_blend_clip] decode");
    let svg_b   = render(&scene_b);
    assert_eq!(normalise_svg(&svg_a), normalise_svg(&svg_b), "[layer_blend_clip] SVG mismatch");

    // `clip` has no SVG 1.1 fallback and is deliberately left unenforced by
    // msx-render-svg (see render/msx-render-svg/src/layer.rs's own doc
    // comment on `render_layer`) — the SVG comparison above can't see it at
    // all, even though it's a real, binary-encoded field (header FLAG bit
    // aside, `encode_layer` writes it unconditionally). blend_mode and
    // opacity DO surface as CSS so the comparison above already covers
    // those; this closes the one field it structurally can't.
    match (scene_a.elements.last(), scene_b.elements.last()) {
        (Some(Element::Layer(la)), Some(Element::Layer(lb))) => {
            assert_eq!(la.clip, lb.clip);
            assert!(la.clip, "fixture itself should set clip = true");
        }
        other => panic!("[layer_blend_clip] expected two Element::Layer, got {:?}", other),
    }
}

// ── Image ─────────────────────────────────────────────────────────────────────

#[test]
fn roundtrip_image_embedded() {
    // Same minimal (non-functional, header-only) PNG base64 literal already
    // used by core/msx-parser/src/lib.rs's own embedded-image test, reused
    // here rather than inventing a new one.
    check_roundtrip("image_embedded", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 200, height = 200, background = #ffffff }
  elements::
    { type = "image", data = "iVBORw0KGgoAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
      x = 60, y = 60, width = 80, height = 80, anchor = "center" }
)
"#);
}

// ── Defs whose fields `check_roundtrip`'s SVG-text comparison can't see ──────
//
// ConicGradient, Shader, and Audio all have real fields that never surface
// in msx-render-svg's output at all (confirmed by reading each renderer's
// source directly, not assumed):
//   - `ConicGradient::to_svg()` only ever emits an explanatory HTML comment
//     ("SVG 1.1 has no native conic-gradient primitive") — cx/cy/angle/stops
//     never appear anywhere in the rendered text. This is a third rendering
//     gap beyond the flat-color-gradient and no-op-text gaps already known —
//     conic gradients don't render on CPU or GPU (per the existing finding)
//     *or* in SVG.
//   - An ordinary shader-filled shape's `fill="url(#id)"` is left
//     unresolved (no real paint-server element is ever emitted for it) and
//     the def itself only emits a comment — source_ref/entry_point/uniforms
//     never surface either.
//   - `Def::Audio` has no canvas position and nothing in this project plays
//     audio yet, so it's invisible to every renderer by design.
// Each test below therefore compares the decoded AST fields directly,
// the same way `roundtrip_layer_blend_and_clip` does for `clip` above.

#[test]
fn roundtrip_conic_gradient_fields() {
    let source = r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 300, height = 300, background = #ffffff }
  defs::
    { type = "conic_gradient", id = "wheel", cx = 0.5, cy = 0.4, angle = 1.2,
      stops = [
        { offset = 0.0, color = #ff0000, opacity = 1.0 },
        { offset = 0.5, color = #00ff00, opacity = 1.0 },
        { offset = 1.0, color = #0000ff, opacity = 1.0 }
      ] }
  elements::
    { type = "circle", cx = 150, cy = 150, r = 100,
      style = { fill = "url(#wheel)", stroke = "none", stroke_width = 0, opacity = 1.0 } }
)
"#;
    let scene_a = parse_scene(source).expect("[conic_gradient] parse");
    let binary  = compile(&scene_a, true).expect("[conic_gradient] compile");
    let scene_b = decode(&binary).expect("[conic_gradient] decode");

    match (find_def(&scene_a, "wheel"), find_def(&scene_b, "wheel")) {
        (Some(Def::ConicGradient(ga)), Some(Def::ConicGradient(gb))) => {
            assert!(approx_eq(ga.cx, gb.cx), "cx: {} vs {}", ga.cx, gb.cx);
            assert!(approx_eq(ga.cy, gb.cy), "cy: {} vs {}", ga.cy, gb.cy);
            assert!(approx_eq(ga.angle, gb.angle), "angle: {} vs {}", ga.angle, gb.angle);
            assert_eq!(ga.stops.len(), gb.stops.len());
            for (sa, sb) in ga.stops.iter().zip(&gb.stops) {
                assert!(approx_eq(sa.offset, sb.offset), "stop offset: {} vs {}", sa.offset, sb.offset);
                assert_eq!(sa.color, sb.color);
            }
        }
        other => panic!("[conic_gradient] expected two Def::ConicGradient, got {:?}", other),
    }
}

#[test]
fn roundtrip_shader_def_fields() {
    let source = r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 200, height = 200, background = #000000 }
  defs::
    { type = "shader", id = "plasma", source_ref = "shaders/plasma.wgsl",
      entry_point = "fs_plasma", fallback_color = #6b46ff,
      uniforms = [ { name = "speed", type = "float", value = 1.5 },
                   { name = "resolution", type = "vec2", value = [200.0, 200.0] } ] }
  elements::
    { type = "rect", x = 20, y = 20, width = 160, height = 160,
      style = { fill = "url(#plasma)", stroke = "none", stroke_width = 0, opacity = 1.0 } }
)
"#;
    let scene_a = parse_scene(source).expect("[shader_def_fields] parse");
    let svg_a   = render(&scene_a);
    let binary  = compile(&scene_a, true).expect("[shader_def_fields] compile");
    let scene_b = decode(&binary).expect("[shader_def_fields] decode");
    let svg_b   = render(&scene_b);
    assert_eq!(normalise_svg(&svg_a), normalise_svg(&svg_b), "[shader_def_fields] SVG mismatch");

    match (find_def(&scene_a, "plasma"), find_def(&scene_b, "plasma")) {
        (Some(Def::Shader(sa)), Some(Def::Shader(sb))) => {
            assert_eq!(sa.source_ref, sb.source_ref);
            assert_eq!(sa.entry_point, sb.entry_point);
            assert_eq!(sa.fallback_color, sb.fallback_color);
            assert_eq!(sa.uniforms.len(), sb.uniforms.len());
            for (ua, ub) in sa.uniforms.iter().zip(&sb.uniforms) {
                assert_eq!(ua.name, ub.name);
                // Written and read back as f32 with no intermediate f64
                // rounding (see compiler.rs's shader-uniform encode), so
                // exact equality is correct here, unlike the f64 fields
                // elsewhere in this file that need approx_eq.
                assert_eq!(ua.value, ub.value);
            }
        }
        other => panic!("[shader_def_fields] expected two Def::Shader, got {:?}", other),
    }
}

#[test]
fn roundtrip_audio_def_embedded() {
    // Same synthetic RIFF/WAVE base64 literal already used by
    // core/msx-parser/src/lib.rs and core/msx-binary's own
    // `audio_def_roundtrips` test — reused rather than inventing a new one.
    let source = r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 10, height = 10, background = #000000 }
  defs::
    { type = "audio", id = "chime",
      data = "UklGRgAAAABXQVZFAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==" }
  elements::
)
"#;
    let scene_a = parse_scene(source).expect("[audio_embedded] parse");
    let binary  = compile(&scene_a, true).expect("[audio_embedded] compile");
    let scene_b = decode(&binary).expect("[audio_embedded] decode");

    match (find_def(&scene_a, "chime"), find_def(&scene_b, "chime")) {
        (Some(Def::Audio(aa)), Some(Def::Audio(ab))) => {
            assert_eq!(aa.source, ab.source);
            match &aa.source {
                MediaSource::Embedded(bytes) => {
                    assert_eq!(&bytes[0..4], b"RIFF");
                    assert_eq!(&bytes[8..12], b"WAVE");
                }
                other => panic!("[audio_embedded] expected MediaSource::Embedded, got {:?}", other),
            }
        }
        other => panic!("[audio_embedded] expected two Def::Audio, got {:?}", other),
    }
}

#[test]
fn roundtrip_audio_def_file_ref() {
    let source = r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 10, height = 10, background = #000000 }
  defs::
    { type = "audio", id = "chime", source_ref = "assets/chime.wav" }
  elements::
)
"#;
    let scene_a = parse_scene(source).expect("[audio_file_ref] parse");
    let binary  = compile(&scene_a, true).expect("[audio_file_ref] compile");
    let scene_b = decode(&binary).expect("[audio_file_ref] decode");

    match (find_def(&scene_a, "chime"), find_def(&scene_b, "chime")) {
        (Some(Def::Audio(aa)), Some(Def::Audio(ab))) => {
            assert_eq!(aa.source, ab.source);
            assert_eq!(aa.source, MediaSource::FileRef("assets/chime.wav".to_string()));
        }
        other => panic!("[audio_file_ref] expected two Def::Audio, got {:?}", other),
    }
}

// ── Animation timeline ────────────────────────────────────────────────────────
//
// The one fixture in this entire file that exercises Scene::animations /
// duration / loop_mode at all. Until now, tests/roundtrip.rs's SVG-text
// comparison gave zero coverage of the exact bug closed this session
// (compiler.rs's compile()/decode() never touched these three fields at
// all). msx-render-svg draws a scene's static base geometry only — it
// doesn't sample the timeline at all — so `check_roundtrip` alone would
// have silently reported PASS both before and after that fix, the same
// blindness apps/msx-cli's own `animations_match` was added to close for
// `cmd_roundtrip`. This does both: the SVG check for the geometry, plus a
// direct field-by-field comparison (with the same f32-wire-format epsilon
// tolerance as `animations_match`) for the timeline.

#[test]
fn roundtrip_animation_timeline() {
    let source = r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 200, height = 200, background = #000000 }
  duration = 3.0
  loop_mode = "ping_pong"
  elements::
    { type = "circle", id = "orbiter", cx = 100, cy = 100, r = 15,
      style = { fill = #f97316, stroke = "none", stroke_width = 0, opacity = 1.0 } }
  animations::
    { target_id = "orbiter", property = "translate_x",
      keyframes = [ { time = 0.0, value = 0.0, easing = "linear" },
                    { time = 1.5, value = 60.0, easing = "ease_in_out" },
                    { time = 3.0, value = 0.0, easing = "ease_out" } ] }
    { target_id = "orbiter", property = "opacity",
      keyframes = [ { time = 0.0, value = 1.0 },
                    { time = 3.0, value = 0.4, easing = "ease_in" } ] }
)
"#;
    let scene_a = parse_scene(source).expect("[animation_timeline] parse");
    let svg_a   = render(&scene_a);
    let binary  = compile(&scene_a, true).expect("[animation_timeline] compile");
    let scene_b = decode(&binary).expect("[animation_timeline] decode");
    let svg_b   = render(&scene_b);

    assert_eq!(normalise_svg(&svg_a), normalise_svg(&svg_b), "[animation_timeline] SVG mismatch");

    assert_eq!(scene_a.loop_mode, LoopMode::PingPong, "[animation_timeline] fixture sanity");
    assert!(approx_eq(scene_a.duration, 3.0), "[animation_timeline] fixture sanity");

    assert!(approx_eq(scene_a.duration, scene_b.duration),
        "[animation_timeline] duration: {} vs {}", scene_a.duration, scene_b.duration);
    assert_eq!(scene_a.loop_mode, scene_b.loop_mode, "[animation_timeline] loop_mode");
    assert_eq!(scene_a.animations.len(), scene_b.animations.len(), "[animation_timeline] track count");

    for (ta, tb) in scene_a.animations.iter().zip(&scene_b.animations) {
        assert_eq!(ta.target_id, tb.target_id, "[animation_timeline] target_id");
        assert_eq!(ta.property, tb.property, "[animation_timeline] property");
        assert_eq!(ta.keyframes.len(), tb.keyframes.len(), "[animation_timeline] keyframe count");
        for (ka, kb) in ta.keyframes.iter().zip(&tb.keyframes) {
            assert!(approx_eq(ka.time, kb.time), "[animation_timeline] keyframe time: {} vs {}", ka.time, kb.time);
            assert!(approx_eq(ka.value, kb.value), "[animation_timeline] keyframe value: {} vs {}", ka.value, kb.value);
            assert_eq!(ka.easing, kb.easing, "[animation_timeline] easing");
        }
    }
}

// ── Groups and nesting ────────────────────────────────────────────────────────

#[test]
fn roundtrip_group_with_transform() {
    check_roundtrip("group_transform", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 400, background = #f0f0f0 }
  elements::
    { type = "group",
      transform = { type = "translate", x = 100, y = 100 },
      elements = [
        { type = "rect", x = 0, y = 0, width = 80, height = 80,
          style = { fill = #007bff, stroke = "none", stroke_width = 0, opacity = 1.0 } },
        { type = "circle", cx = 40, cy = 40, r = 20,
          style = { fill = #ffffff, stroke = "none", stroke_width = 0, opacity = 0.8 } }
      ] }
)
"#);
}

#[test]
fn roundtrip_nested_groups() {
    // NOTE: 2 levels of group nesting + a styled leaf would land one level
    // past DixScript's MAX_NESTING_DEPTH=5 (data_section_analyzer.rs) —
    // confirmed by tracing its depth counter, which increments once per
    // array AND once per object on the way down. style is fully optional
    // (msx-parser's parse_style defaults sanely when absent), so dropping
    // it here keeps the actual group-within-group structure this test
    // exists to verify, while staying inside the depth limit.
    check_roundtrip("nested_groups", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 400, background = #ffffff }
  elements::
    { type = "group",
      elements = [
        { type = "group",
          transform = { type = "rotate", angle = 45 },
          elements = [
            { type = "rect", x = -25, y = -25, width = 50, height = 50 }
          ] }
      ] }
)
"#);
}

// ── QuickFuncs evaluated before roundtrip ────────────────────────────────────

#[test]
fn roundtrip_quickfunc_badge() {
    check_roundtrip("quickfunc_badge", r#"
@CONFIG( version -> "1.0.0" )
@QUICKFUNCS(
  ~badge<object>(x, y, label, color) {
    return {
      type = "group"
      elements = [
        { type = "rect", x = x, y = y, width = 90, height = 30, rx = 15,
          style = { fill = color, stroke = "none", stroke_width = 0, opacity = 1.0 } },
        { type = "text", x = x + 45, y = y + 20, content = label,
          style = { fill = #ffffff, font_size = 12, text_anchor = "middle",
                    font_weight = "bold", stroke = "none", stroke_width = 0, opacity = 1.0 } }
      ]
    }
  }
)
@DATA(
  scene = { width = 500, height = 100, background = #f4f5f7 }
  elements::
    badge(20,  30, "primary", #007bff)
    badge(130, 30, "success", #28a745)
    badge(240, 30, "danger",  #dc3545)
)
"#);
}

#[test]
fn roundtrip_parametric_circles() {
    check_roundtrip("parametric_circles", r#"
@CONFIG( version -> "1.0.0" )
@QUICKFUNCS(
  ~dot<object>(cx, cy, r, color) {
    return {
      type = "circle"
      cx = cx  cy = cy  r = r
      style = { fill = color, stroke = "none", stroke_width = 0, opacity = 0.8 }
    }
  }
)
@DATA(
  scene = { width = 600, height = 200, background = #1a1a2e }
  elements::
    dot(60,  100, 40, #e94560)
    dot(160, 100, 35, #533483)
    dot(260, 100, 45, #0f3460)
    dot(360, 100, 30, #4a9eff)
    dot(460, 100, 50, #22c55e)
    dot(540, 100, 25, #f5a623)
)
"#);
}

// ── Gradients ─────────────────────────────────────────────────────────────────

#[test]
fn roundtrip_linear_gradient() {
    check_roundtrip("linear_gradient", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 600, height = 200, background = #ffffff }
  defs::
    { type = "linear_gradient", id = "sunset",
      x1 = 0.0, y1 = 0.0, x2 = 1.0, y2 = 0.0,
      stops = [
        { offset = 0.0, color = #f7971e, opacity = 1.0 },
        { offset = 1.0, color = #ffd200, opacity = 1.0 }
      ] }
  elements::
    { type = "rect", x = 50, y = 50, width = 500, height = 100,
      style = { fill = "url(#sunset)", stroke = "none", stroke_width = 0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_radial_gradient() {
    check_roundtrip("radial_gradient", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 400, background = #000000 }
  defs::
    { type = "radial_gradient", id = "glow",
      cx = 0.5, cy = 0.5, r = 0.5,
      stops = [
        { offset = 0.0, color = #4facfe, opacity = 1.0 },
        { offset = 1.0, color = #00f2fe, opacity = 0.0 }
      ] }
  elements::
    { type = "circle", cx = 200, cy = 200, r = 180,
      style = { fill = "url(#glow)", stroke = "none", stroke_width = 0, opacity = 1.0 } }
)
"#);
}

// ── Edge cases ────────────────────────────────────────────────────────────────

#[test]
fn roundtrip_many_elements_compression_ratio() {
    let colors = ["#e94560", "#533483", "#0f3460", "#4a9eff", "#22c55e",
                  "#f5a623", "#a78bfa", "#ef4444", "#3b82f6", "#10b981"];
    let mut src = String::from(
r#"@CONFIG( version -> "1.0.0" )
@QUICKFUNCS(
  ~dot<object>(cx, cy, r, color) {
    return { type = "circle", cx = cx, cy = cy, r = r,
             style = { fill = color, stroke = "none", stroke_width = 0, opacity = 0.9 } }
  }
)
@DATA(
  scene = { width = 1000, height = 1000, background = #ffffff }
  elements::
"#);
    for i in 0..200usize {
        let x = (i % 20) * 50 + 25;
        let y = (i / 20) * 50 + 25;
        let r = 15 + (i % 5) * 3;
        let c = colors[i % colors.len()];
        src.push_str(&format!("    dot({}, {}, {}, {})\n", x, y, r, c));
    }
    src.push(')');

    let scene_a = parse_scene(&src).expect("parse 200 circles");
    let svg_a   = render(&scene_a);
    let binary  = compile(&scene_a, true).expect("compile 200 circles");
    let scene_b = decode(&binary).expect("decode 200 circles");
    let svg_b   = render(&scene_b);

    assert_eq!(normalise_svg(&svg_a), normalise_svg(&svg_b));
    assert!(binary.len() < svg_a.len(),
        "compressed binary ({} B) should be smaller than SVG ({} B)",
        binary.len(), svg_a.len());
}

#[test]
fn roundtrip_opacity_zero_element() {
    check_roundtrip("opacity_zero", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 100, height = 100, background = #000000 }
  elements::
    { type = "rect", x = 0, y = 0, width = 100, height = 100,
      style = { fill = #ffffff, stroke = "none", stroke_width = 0, opacity = 0.0 } }
)
"#);
}

#[test]
fn roundtrip_stroke_dasharray() {
    check_roundtrip("stroke_dasharray", r#"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 200, background = #ffffff }
  elements::
    { type = "line", x1 = 20, y1 = 100, x2 = 380, y2 = 100,
      style = { fill = "none", stroke = #000000, stroke_width = 3.0,
                stroke_dasharray = [10, 5, 3, 5], stroke_dashoffset = 2.0, opacity = 1.0 } }
)
"#);
}

#[test]
fn roundtrip_use_element() {
    // BUGFIX: `href = "#tile"` below embeds a literal `"#`, which is the
    // close delimiter for a single-hash raw string — this terminated the
    // string mid-source and the file didn't compile. Bumped to double-hash;
    // verified against rustc directly.
    check_roundtrip("use_element", r##"
@CONFIG( version -> "1.0.0" )
@DATA(
  scene = { width = 400, height = 200, background = #f0f0f0 }
  defs::
    { type = "linear_gradient", id = "base",
      x1 = 0.0, y1 = 0.0, x2 = 1.0, y2 = 0.0,
      stops = [
        { offset = 0.0, color = #007bff, opacity = 1.0 },
        { offset = 1.0, color = #00c9ff, opacity = 1.0 }
      ] }
  elements::
    { type = "rect", id = "tile", x = 0, y = 0, width = 80, height = 80,
      style = { fill = "url(#base)", stroke = "none", stroke_width = 0, opacity = 1.0 } }
    { type = "use", href = "#tile", x = 100, y = 60 }
    { type = "use", href = "#tile", x = 210, y = 60 }
)
"##);
        }
