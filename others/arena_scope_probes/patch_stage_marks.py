#!/usr/bin/env python3
"""Copies dixscript/ to a scratch directory and adds per-stage marks to
`compile_source_from_bytes`, so stage_probe_main.rs can report time and
allocations per pipeline stage. Nothing in the repo is modified.

    python3 others/arena_scope_probes/patch_stage_marks.py /tmp/probe_dix

Optional second argument `--cap-capacity` also caps the token-length-derived
`Vec::with_capacity` reservations in the section parsers at 16 (the experiment
described in docs/dixscript/arena-ast.md).
"""
import os, re, shutil, sys

repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
dest = sys.argv[1]
cap = '--cap-capacity' in sys.argv
if os.path.exists(dest):
    shutil.rmtree(dest)
shutil.copytree(os.path.join(repo, 'dixscript'), os.path.join(dest, 'dixscript'))

p = os.path.join(dest, 'dixscript/src/Runtime/loader.rs')
s = open(p, 'rb').read().decode('utf-8')
marks = [('        // Stage 1: tokenize', 'start'), ('        // Stage 2: split @CONFIG', 'tokenized'),
         ('        // Stage 3: process @CONFIG', 'config_split'), ('        // Stage 4: parse the rest', 'config_done'),
         ('        // Stage 5: semantic analysis.', 'parsed'), ('        // Stage 6: AST enhancement.', 'analyzed'),
         ('        // Stage 7: value resolution', 'enhanced'), ('        // Stage 8: numeric array homogenization.', 'resolved'),
         ('        // Stage 9: @SCHEMA validation', 'homogenized')]
for anchor, name in marks:
    assert s.count(anchor) == 1, anchor
    s = s.replace(anchor, f'        crate::probe::mark("{name}");\n' + anchor)
i = s.index('// Stage 9: @SCHEMA')
j = s.index('        Ok(resolved_ast)\n', i)
s = s[:j] + '        crate::probe::mark("schema_done");\n' + s[j:]
open(p, 'wb').write(s.encode('utf-8'))

lib = os.path.join(dest, 'dixscript/src/lib.rs')
t = open(lib, 'rb').read().decode('utf-8')
t += '''
/// Scratch instrumentation (probe only).
pub mod probe {
    pub static HOOK: std::sync::OnceLock<fn(&'static str)> = std::sync::OnceLock::new();
    pub fn mark(s: &'static str) { if let Some(f) = HOOK.get() { f(s) } }
}
'''
open(lib, 'wb').write(t.encode('utf-8'))

if cap:
    sp = os.path.join(dest, 'dixscript/src/Compiler/Core/SectionParsers')
    n = 0
    for f in os.listdir(sp):
        if not f.endswith('.rs'):
            continue
        q = os.path.join(sp, f)
        x = open(q, 'rb').read().decode('utf-8')
        y, a = re.subn(r'Vec::with_capacity\((usize::max\(\d+, self\.tokens\.len\(\) / \d+\)|estimate_\w+\(self\.tokens\.len\(\)\))\)',
                       r'Vec::with_capacity(usize::min(16, \1))', x)
        y, b = re.subn(r'Vec::with_capacity\((estimated_(?:entries|props|items|args))\)',
                       r'Vec::with_capacity(usize::min(16, \1))', y)
        if a or b:
            open(q, 'wb').write(y.encode('utf-8'))
            n += a + b
    print('capacity reservations capped:', n)
print('patched copy at', dest)
