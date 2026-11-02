import base64, os, sys

media = "/tmp/media"
out   = sys.argv[1]
os.makedirs(out, exist_ok=True)

def b64(name):
    return base64.b64encode(open(os.path.join(media, name), "rb").read()).decode("ascii")

tour = r'''// ───────────────────────────────────────────────────────────────────────────
//  MDIX Studio Explorer tour
//
//  Open this file in MDIX Studio, switch to the Explorer tab and click values.
//  Every value below that is written out as a plain literal can be edited.
//  Nothing is written to disk until you press Save.
//  See README.md next to this file for what each value should do.
// ───────────────────────────────────────────────────────────────────────────

@CONFIG(
  version -> "1.0.0"
)

@DATA(
  // ── Text ───────────────────────────────────────────────────────────────
  // Short text edits in place. Long text, or text with line breaks, opens a text area.
  title<string>         = "MDIX Studio Explorer tour"
  short_text<string>    = "Click me to edit in place."
  empty_text<string>    = ""
  unicode_text<string>  = "Zażółć gęślą jaźń, 世界, café, 😀"
  escaped_text<string>  = "Line one\nLine two\twith a tab, a \"quote\" and a back\\slash"
  long_text<string>     = "This string is longer than one hundred characters, so clicking it opens a text area instead of the one-line field. Ctrl or Cmd plus Enter applies the change."

  // A raw string is written exactly as typed and may span lines. It cannot contain a backtick.
  raw_notes<string> = `Raw string: nothing is escaped.
Backslashes stay as they are: C:\Games\Grimoire\save.dat
Quotes too: "like this" and 'like this'.`

  // ── Dates and timestamps ───────────────────────────────────────────────
  // Click one to open the picker. A timestamp keeps its zone and its fraction of a second.
  release_date<date>     = 2026-01-15
  leap_day<date>         = 2024-02-29
  year_start<date>       = 2025-01-01
  saved_at<timestamp>    = 2025-01-15T10:30:00
  utc_time<timestamp>    = 2025-01-15T10:30:00Z
  india_time<timestamp>  = 2025-01-15T10:30:00.250+05:30
  pacific_time<timestamp> = 2025-01-15T10:30:00-08:00
  precise_time<timestamp> = 2025-06-30T23:59:59.1234567Z

  // ── Regular expressions ────────────────────────────────────────────────
  // Click one for the editor with live matching. The pattern is shown unescaped.
  username_rule  = r:("^[a-z][a-z0-9_]{2,15}$")
  digits_only    = r:("^\\d+$")
  iso_date       = r:("(?P<year>\\d{4})-(?P<month>\\d{2})-(?P<day>\\d{2})")
  case_blind     = r:("(?i)^(red|green|blue)$")
  email_like     = r:(`^\w+@\w+\.[a-z]{2,}$`)
  empty_pattern  = r:("")

  // ── Blobs ──────────────────────────────────────────────────────────────
  // Click one to see what it is. PNG and JPEG show as images, WAV and Ogg can be played,
  // text shows as text, anything else as a hex dump. Replace, Save as, Open and Clear are in the popup.
  icon_png      = b:("__PNG__")
  icon_jpeg     = b:("__JPG__")
  icon_gif      = b:("__GIF__")
  icon_bmp      = b:("__BMP__")
  icon_webp     = b:("__WEBP__")
  chime_wav     = b:("__WAV__")
  chime_ogg     = b:("__OGG__")
  chime_mp3     = b:("__MP3__")
  readme_text   = b:("__TXT__")
  noise_binary  = b:("__BIN__")
  empty_blob    = b:("")

  // ── The same kinds inside an object and a list ─────────────────────────
  profile = {
    name        = "Ada",
    joined      = 2024-03-09,
    last_seen   = 2025-06-01T08:15:30Z,
    handle_rule = r:("^[a-z]+$"),
    avatar      = b:("__PNG__"),
    bio         = `Written by hand.
Second line of the bio.`
  }

  holidays = [2026-01-01, 2026-07-04, 2026-12-25]
  blocked_names = [r:("^admin"), r:("^root$"), r:("(?i)^test")]

  // ── Computed rows are not literals: they stay read-only ────────────────
  // (Change the arguments in the Editor tab instead.)
  items::
    makeItem(1, "Iron Sword",    50),
    makeItem(2, "Health Potion", 25),
    makeItem(3, "Dragon Scale",  5000)
)

@QUICKFUNCS(
  ~makeItem<object>(id, name, value) {
    return {
      id    = id
      name  = name
      value = value
    }
  }
)
'''
for tag, f in (("PNG","icon.png"),("JPG","icon.jpg"),("GIF","icon.gif"),("BMP","icon.bmp"),("WEBP","icon.webp"),
               ("WAV","tone.wav"),("OGG","tone.ogg"),("MP3","tone.mp3"),("TXT","note.txt"),("BIN","noise.bin")):
    tour = tour.replace("__%s__" % tag, b64(f))
open(os.path.join(out, "ExplorerTour.mdix"), "w", newline="\n").write(tour)

numbers = r'''// ───────────────────────────────────────────────────────────────────────────
//  Numbers, enums, arrays and tuples
//
//  Plain numbers, hex and binary integers, colours, enum members, array items and the
//  elements of tuples can all be edited in the Explorer. A tuple's header shows all its
//  values; unfold it to edit one element at a time.
// ───────────────────────────────────────────────────────────────────────────

@CONFIG(
  version -> "1.0.0"
)

@ENUMS(
  Difficulty { EASY, NORMAL, HARD, NIGHTMARE }
  Rarity     { COMMON = 1, UNCOMMON = 2, RARE = 3, EPIC = 4, LEGENDARY = 5 }
  Mood       { CALM, ANGRY = -1, SLEEPY = 10 }
)

@DATA(
  // ── Plain numbers ──────────────────────────────────────────────────────
  level<int>        = 7
  gold<long>        = 9_000_000_000L
  speed<float>      = 1.5f
  gravity<double>   = 9.81
  alive<bool>       = true
  temperature       = -12

  // ── Hex and binary integers keep their notation when edited ───────────
  // Type any notation (255, 0xFF, 0b1111_1111); it is written back in the original one.
  flags_hex         = 0xFF
  mask_binary       = 0b1111_0000
  big_hex           = 0xDEAD_BEEF
  long_hex          = 0xFFL
  grouped_hex       = 0xFF_FF_FF

  // ── Colours: #RGB, #RGBA, #RRGGBB and #RRGGBBAA ────────────────────────
  tint_rgb          = #F80
  tint_rgba         = #F80C
  tint_rrggbb       = #FF8800
  tint_rrggbbaa     = #FF8800CC

  // ── Enums: click one to pick another member ────────────────────────────
  difficulty<enum>  = Difficulty.NORMAL
  drop_rarity       = Rarity.RARE
  mood              = Mood.ANGRY

  // ── Arrays ─────────────────────────────────────────────────────────────
  scores            = [10, 20, 30]
  names             = ["Ann", "Bob", "Cy"]
  switches          = [true, false, true]
  ratios            = [0.25, 0.5, 1.0]
  hex_list          = [0x01, 0x02, 0xFF]
  palette           = [#F00, #0F0, #00F, #FF08]
  nothing           = []
  rarities          = [Rarity.COMMON, Rarity.EPIC]
  grid              = [[1, 2], [3, 4]]

  // ── Tuples ─────────────────────────────────────────────────────────────
  spawn_point       = t:(1.5, 2.5, 0.0)
  labelled          = t:("Sword", 3, true)
  nested_tuple      = t:(1, t:(2, 3))
  enum_tuple        = t:(Difficulty.HARD, 14, "boss")
  gradient          = t:(#F00, #00F8, #FF8800)
  schedule          = t:(2026-01-15, 2025-01-15T10:30:00Z, "kickoff")
  list_in_tuple     = t:([1, 2, 3], "tag")

  // ── An object that mixes them ──────────────────────────────────────────
  player = {
    name       = "Hero",
    hp         = 100,
    rarity     = Rarity.RARE,
    flags      = 0b0000_1010,
    tint       = #3B7EF6,
    inventory  = ["Sword", "Shield"]
  }

  // A "name::" list is written without brackets. It runs to the end of the section, so it comes last.
  testArray::
    1, 2, 3, 4, 5
)
'''
open(os.path.join(out, "NumbersEnumsAndLists.mdix"), "w", newline="\n").write(numbers)
print("wrote", os.listdir(out))

# ── README and the embedded C# data class ───────────────────────────────────
readme = r'''# MDIX Studio Explorer tour

Two sample files that exercise every value editor in MDIX Studio's **Explorer** tab.

Open one (double-click it, or right-click it and choose *MDIX > Open in MDIX Studio*), switch to the
**Explorer** tab, and click values. Anything written out as a plain literal can be edited. The change is
made in the **Editor** tab's text and compiled straight away, and nothing reaches the file on disk until you
press **Save**. Esc cancels, and a change the compiler would not accept is refused with a message.

## ExplorerTour.mdix

| Value | Click it to... |
|---|---|
| `short_text`, `empty_text`, `unicode_text`, `escaped_text` | edit in place. Enter applies, Esc cancels. Quotes, backslashes and line breaks in `escaped_text` are escaped for you. |
| `long_text`, `raw_notes`, `profile.bio` | open a text area (Ctrl or Cmd + Enter applies). A raw string refuses a backtick. |
| `release_date`, `leap_day`, `year_start`, `holidays` | open the date picker: month grid, a text box for the whole value, and Today. |
| `saved_at`, `utc_time`, `india_time`, `pacific_time`, `precise_time`, `profile.last_seen` | open the timestamp picker. The zone (none, UTC or an offset) and the fraction of a second are kept unless you change them. |
| `username_rule`, `digits_only`, `iso_date`, `case_blind`, `email_like`, `empty_pattern`, `blocked_names` | open the regex editor. Type some test text to see the matches and their groups. |
| `icon_png`, `icon_jpeg`, `profile.avatar` | show the image. |
| `icon_gif`, `icon_bmp`, `icon_webp` | explain that Unity 2022.3 cannot show that type here, and show the first bytes. **Open** shows it in another app. |
| `chime_wav`, `chime_ogg` | play the sound with **Play**. |
| `chime_mp3` | show the first bytes. Use **Open** to hear it in another app. |
| `readme_text` | show the text. |
| `noise_binary` | show a hex dump. |
| `empty_blob` | let you **Replace from file** (up to 1 MB), then **Apply**. |

Things to try that should be **refused**:

- In a date box, type `2026-02-30`. The box turns red and Apply is disabled.
- In `raw_notes`, type a backtick.
- In a regex, type `[unclosed`. The language server explains the error and Apply is disabled. If the server cannot be reached, the check falls back to .NET's engine and says it is approximate.
- In **Replace from file**, choose a file larger than 1 MB.

The `items` rows come from a function call (`makeItem(...)`), so they are computed, not literals. They stay read-only; change the arguments in the Editor tab.

## NumbersEnumsAndLists.mdix

| Value | Click it to... |
|---|---|
| `level`, `gold`, `speed`, `gravity`, `alive`, `temperature` | edit in place. |
| `flags_hex`, `mask_binary`, `big_hex`, `long_hex`, `grouped_hex` | edit in place. Type the number in any notation (`255`, `0xFF`, `0b1111_1111`); it is written back in the notation the file used. `big_hex` is a 64-bit long (it does not fit in 32 bits); an int refuses a value that would become a long. |
| `difficulty`, `drop_rarity`, `mood`, `rarities`, `player.rarity` | open the enum picker, which lists the members from the `@ENUMS` section (with their numbers when the declaration gave one). |
| `scores`, `names`, `switches`, `ratios`, `hex_list`, `palette`, `grid`, `testArray`, `player.inventory` | edit each item on its own. |
| `tint_rgb`, `tint_rgba`, `tint_rrggbb`, `tint_rrggbbaa`, `palette`, `gradient`, `player.tint` | open **Unity's colour picker** from the colour swatch (a wheel or box, sliders, an eyedropper), with the hex value beside it. A colour that was written in the short form (`#F80`) stays short when the picked colour fits it exactly, and is written in full otherwise. |
| `spawn_point`, `labelled`, `nested_tuple`, `enum_tuple`, `gradient`, `schedule`, `list_in_tuple` | show all their values in the header. Unfold one to edit its elements one at a time, each with the editor for its kind: a date opens the picker, an enum its member list, a colour the colour picker. A tuple inside a tuple unfolds again. |

Not editable yet: adding, removing and reordering array items (each existing item can be edited).

Things to try that should be refused:

- In a tuple, change an element to text that is not valid for its kind (a date `2026-02-30`, a number that does not fit an int).
- In the colour box, type `#12` or `#GGG`. The box turns red and Apply is disabled.
'''
open(os.path.join(out, "README.md"), "w", newline="\n").write(readme)

def verbatim(s):
    return '@"' + s.replace('"', '""') + '"'

cs = '''namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The text of the Explorer tour samples, so a menu item can write them into a project (see
    /// <see cref="MdixSampleMenu"/>). No Unity types, so the text can be checked on its own.
    ///
    /// They are embedded here rather than shipped as a Samples~ folder because this repository's .gitignore ignores
    /// anything ending in "~", which would keep such a folder out of version control.
    ///
    /// <c>ExplorerTour</c> and <c>NumbersEnumsAndLists</c> compile with the DixScript compiler, and every plain
    /// literal in them is covered by the Explorer's editors. The blobs are real files (a PNG, a JPEG, a GIF, a BMP,
    /// a WebP, a WAV, an Ogg Vorbis, an MP3, a text and a binary one) with their data inline as base64.
    /// </summary>
    internal static class MdixSampleData
    {
        public const string FolderName     = "MDIX Explorer Tour";

        public const string TourFileName    = "ExplorerTour.mdix";
        public const string NumbersFileName = "NumbersEnumsAndLists.mdix";
        public const string ReadmeFileName  = "README.md";

        public const string Tour = %s;

        public const string Numbers = %s;

        public const string Readme = %s;
    }
}
''' % (verbatim(tour), verbatim(numbers), verbatim(readme))
open(os.path.join(out, "MdixSampleData.cs"), "w", newline="\n").write(cs)
print("wrote README.md and MdixSampleData.cs")
