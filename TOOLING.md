# Explorer verification tooling (recreate under /home/claude/w in a new sandbox)

Layout expected: `w/DixScript-Rust` (fresh clone), `w/gtg` (MidManStudio/GrandTheftGrimoire), `w/mdix-scaffold`,
`w/harness/Program.cs`, `w/tools/*.py`, `w/build/{unity_stub.cs,window_stub.cs}`, `w/ucs` (UnityCsReference tag 2022.3.13f1, sparse).

Setup:
  apt-get update && apt-get install -y dotnet-sdk-8.0          # needs apt-get update first (stale index gives 404)
  pip install midmanstudio-mdix tree-sitter tree-sitter-c-sharp pillow --break-system-packages
  python3 tools/make_stub.py                                    # writes build/stub.cs from the REAL MdixDatabase.cs signatures

Compile + run the pure-logic harness (internal members visible because everything is one compilation):
  REF=$(ls -d /usr/lib/dotnet/packs/Microsoft.NETCore.App.Ref/*/ref/net8.0); REFS=$(ls $REF/*.dll | sed 's/^/-r:/' | tr '\n' ' ')
  ED=DixScript-Rust/com.midmanstudio.mdix/Editor; C=DixScript-Rust/mdix-csharp/src/MidManStudio.Mdix.Core
  dotnet /usr/lib/dotnet/sdk/*/Roslyn/bincore/csc.dll -nologo -langversion:10 -unsafe -target:exe -out:build/h.dll $REFS \
    build/stub.cs $C/MdixError.cs $C/MdixValue.cs $ED/MdixSourceMap.cs $ED/MdixLiteralEdit.cs \
    $ED/Popups/{MdixTemporalText,MdixBlobInfo,MdixBlobTemp,MdixDateTimeModel,MdixRegexTest,MdixIntegerText,MdixTupleText,MdixColorText}.cs \
    $ED/MdixSampleData.cs $ED/Lsp/MdixJsonValue.cs harness/Program.cs
  echo '{"runtimeOptions":{"tfm":"net8.0","framework":{"name":"Microsoft.NETCore.App","version":"8.0.0"}}}' > build/h.runtimeconfig.json
  (cd build && dotnet h.dll)

UI code cannot be compiled with Unity here. Compile it against build/unity_stub.cs + build/window_stub.cs instead
(catches wiring mistakes in our own code; every Unity member in the stubs was checked against UnityCsReference 2022.3.13f1).
Add the Popups/*.cs UI files, MdixEditorWindow.Popups.cs, MdixSampleMenu.cs, MdixSourceMap.cs, MdixLiteralEdit.cs to the list.
Run tree-sitter (python, tree_sitter_c_sharp) on MdixEditorWindow*.cs and Lsp/MdixLspClient.cs, which cannot be stub-compiled.

Differential tests against the REAL compiler (the pip package's MdixDatabase.load_str; it LOGS errors to stderr and returns an
EMPTY database instead of raising, so always run with 2>/dev/null and treat zero keys as failure):
  1. python3 tools/make_samples.py samples_out   (needs /tmp/media: PIL + ffmpeg make icon.png/jpg/gif/bmp/webp, tone.wav/ogg/mp3, note.txt, noise.bin)
  2. cp samples_out/MdixSampleData.cs $ED/ ; python3 tools/check_samples.py samples_out/*.mdix 2>/dev/null
  3. run the harness once (writes /tmp/spans_real/* and /tmp/spans_<sample>.tsv), then:
     python3 tools/check_spans.py samples_out/*.mdix 2>/dev/null      # every C# span vs the compiled value (replica of Matches)
     python3 tools/check_real.py 2>/dev/null                           # same over all real .mdix files (expect 0 mismatches)
     python3 tools/dump_tuples.py 2>/dev/null; run the harness again   # tuple elements vs the engine's tuple JSON
