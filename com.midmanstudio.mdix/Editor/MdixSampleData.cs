namespace MidManStudio.Mdix.Unity.Editor
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

        public const string Tour = @"// ───────────────────────────────────────────────────────────────────────────
//  MDIX Studio Explorer tour
//
//  Open this file in MDIX Studio, switch to the Explorer tab and click values.
//  Every value below that is written out as a plain literal can be edited.
//  Nothing is written to disk until you press Save.
//  See README.md next to this file for what each value should do.
// ───────────────────────────────────────────────────────────────────────────

@CONFIG(
  version -> ""1.0.0""
)

@DATA(
  // ── Text ───────────────────────────────────────────────────────────────
  // Short text edits in place. Long text, or text with line breaks, opens a text area.
  title<string>         = ""MDIX Studio Explorer tour""
  short_text<string>    = ""Click me to edit in place.""
  empty_text<string>    = """"
  unicode_text<string>  = ""Zażółć gęślą jaźń, 世界, café, 😀""
  escaped_text<string>  = ""Line one\nLine two\twith a tab, a \""quote\"" and a back\\slash""
  long_text<string>     = ""This string is longer than one hundred characters, so clicking it opens a text area instead of the one-line field. Ctrl or Cmd plus Enter applies the change.""

  // A raw string is written exactly as typed and may span lines. It cannot contain a backtick.
  raw_notes<string> = `Raw string: nothing is escaped.
Backslashes stay as they are: C:\Games\Grimoire\save.dat
Quotes too: ""like this"" and 'like this'.`

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
  username_rule  = r:(""^[a-z][a-z0-9_]{2,15}$"")
  digits_only    = r:(""^\\d+$"")
  iso_date       = r:(""(?P<year>\\d{4})-(?P<month>\\d{2})-(?P<day>\\d{2})"")
  case_blind     = r:(""(?i)^(red|green|blue)$"")
  email_like     = r:(`^\w+@\w+\.[a-z]{2,}$`)
  empty_pattern  = r:("""")

  // ── Blobs ──────────────────────────────────────────────────────────────
  // Click one to see what it is. PNG and JPEG show as images, WAV and Ogg can be played,
  // text shows as text, anything else as a hex dump. Replace, Save as, Open and Clear are in the popup.
  icon_png      = b:(""iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAABnklEQVR42u2Wv2sCMRTHv5YOyd/h6ugkTo6NSzkcijjI4VJxkUMyOTiIiLiIkxwOIg4luPjoIrdUbuvm6uDg/9GhUTyMvR/U7UKGHO/l873k5eUlA2wYcOn8zjiZiQPPYHhoSwUiCPB0i/5NYOSJ60+3RJdx/TBYZaVxVgYv4Ym2JHFPVQlq7we/43VOJkm0pdJ0x6LrmU0lAFzoq7xMskVqKQDYVWIAD3q6VertNH1elEliQK4AULWJmXx6W00/Hr8A+DYlDbLJYbjR9HFZ/uEG4Akcxu5NBQDLoVvT5EzvVyQ4yCEAhakwcsJWcGOdLTS9W5NR1hpPYDHT9E5DRtzMGAIfE01vtWT0aIVddmfrZqjpjY4MdQ4GmcHYSy4BUE0Bhu2ZXuvKW0/RFAB8l4yc8GO662l6pS/jnuaQGAhF+7aml8dmumUJAJ4yJ2OIwP49kKsOBXLVFvqOIiLO4le0Q13Ti3P97yNhuFOVR4zHrwenN03PryQA2ycGTAvBeuATA1hYCDP4NteD0+sgt5YPfHhlP2X6qkhfdtHaD6GBd8gTY3LRAAAAAElFTkSuQmCC"")
  icon_jpeg     = b:(""/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAoHBwgHBgoICAgLCgoLDhgQDg0NDh0VFhEYIx8lJCIfIiEmKzcvJik0KSEiMEExNDk7Pj4+JS5ESUM8SDc9Pjv/2wBDAQoLCw4NDhwQEBw7KCIoOzs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozv/wAARCAAgACADASIAAhEBAxEB/8QAGQABAQADAQAAAAAAAAAAAAAABAUBAwYH/8QAKBAAAgEDAwMCBwAAAAAAAAAAAQIDBAURACExEhNhBkEUFSJRcYGh/8QAFwEAAwEAAAAAAAAAAAAAAAAAAwUGB//EABwRAAICAgMAAAAAAAAAAAAAAAEEABECAwUhMf/aAAwDAQACEQMRAD8A89hovGqENH40+Gi8a7QqfSJjo6GGL5mY1epqpIw7wORntpnIACnBO+er2xtW7266HsAi2aucZDRcbafDRcbapG413r6vitE9RElN3Qs1zWnHcbY4iUrjKkn+545elsamc07qQ0R6GBbqII2599LcmyfZYce8TlRk2Gi8aTc/ivX10mezxTCjjSNbjUIB9cmMFIhkkrgc7+dsZqJb1kQo6BkYYZWGQR9jp9styW6KKKjUwLEMJ0Egj96X7WL7mT8c9jjjRmbHYBauxNNTtS0tKwKqRhmYbgAHckn3/O+t3YaoqJJ3UBpHLHHGSc6csEszBpXeRsYy5JOmw0XG2l2xmWCTYuxP/9k="")
  icon_gif      = b:(""R0lGODdhEAAQAIMAAP/99sTutNO7tMyRtGXqtBrmtFyitBGitOhUtJZUtN0RtJERtCtntDM8tDYWhwkRtCwAAAAAEAAQAEAImQAfCHzgwMGCgwcVKFR4oCEDAAYiOhhAEQDFAQ0yakwAoGOCBA4QiCxAsoADAgQ6AgggoGXLgRoRJlyogIFNmwA+fnQAQKTIhg0NAHAQsaJLASULoFwaoKnTAAMfZJS5gKYCgRqnLvi4UORNBg08fhzqE8FXiAZ2Jhhg8SdQtEQvWqQI9IABnhEFdDyadCnKpyyRlvRLAHDTgAA7"")
  icon_bmp      = b:(""Qk32AAAAAAAAADYAAAAoAAAACAAAAAgAAAABABgAAAAAAMAAAADEDgAAxA4AAAAAAAAAAAAAtP8AtP8ktP9ItP9ttP+RtP+2tP/atP//tNoAtNokPB4e////////////tNratNr/tLYAtLYk////PB4etLaRtLa2////tLb/tJEAtJEk////tJFtPB4etJG2////tJH/tG0AtG0k////tG1ttG2RPB4e////tG3/tEgAtEgktEhI////////////PB4etEj/tCQAtCQktCRItCRttCSRtCS2tCTatCT/tAAAtAAktABItABttACRtAC2tADatAD/"")
  icon_webp     = b:(""UklGRpQAAABXRUJQVlA4IIgAAACQAgCdASoQABAAAoBCJbACdAaYDmAb7vc+zoTeAAD++JToTTZNuW02snTrJtWX+5lkPvDEjli2Y0FJ5n9m6QeuZeUTuh43K+uPMjIsj46bG7OFGuIvvFbWTbRzgf6/3SG4igxdj/8MwgHna7s//y29N/otPFBXDWT3YbX+nqG//2xrUp9YAAAA"")
  chime_wav     = b:(""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"")
  chime_ogg     = b:(""T2dnUwACAAAAAAAAAACQgqHMAAAAAB4OW4sBHgF2b3JiaXMAAAAAAUAfAAAAAAAAgFcAAAAAAACZAU9nZ1MAAAAAAAAAAAAAkIKhzAEAAACfEvDkC0D///////////+1A3ZvcmJpcw0AAABMYXZmNjAuMTYuMTAwAQAAAB8AAABlbmNvZGVyPUxhdmM2MC4zMS4xMDIgbGlidm9yYmlzAQV2b3JiaXMSQkNWAQAAAQAMUhQhJRlTSmMIlVJSKQUdY1BbRx1j1DlGIWQQU4hJGaV7TyqVWErIEVJYKUUdU0xTSZVSlilFHWMUU0ghU9YxZaFzFEuGSQklbE2udBZL6JljljFGHWPOWkqdY9YxRR1jUlJJoXMYOmYlZBQ6RsXoYnwwOpWiQii+x95S6S2FiluKvdcaU+sthBhLacEIYXPttdXcSmrFGGOMMcbF4lMogtCQVQAAAQAAQAQBQkNWAQAKAADCUAxFUYDQkFUAQAYAgAAURXEUx3EcR5IkywJCQ1YBAEAAAAIAACiO4SiSI0mSZFmWZVmWpnmWqLmqL/uuLuuu7eq6DoSGrAQAyAAAGIYhh95JzJBTkEkmKVXMOQih9Q455RRk0lLGmGKMUc6QUwwxBTGG0CmFENROOaUMIghDSJ1kziBLPejgYuc4EBqyIgCIAgAAjEGMIcaQcwxKBiFyjknIIETOOSmdlExKKK20lkkJLZXWIueclE5KJqW0FlLLpJTWQisFAAAEOAAABFgIhYasCACiAAAQg5BSSCnElGJOMYeUUo4px5BSzDnFmHKMMeggVMwxyByESCnFGHNOOeYgZAwq5hyEDDIBAAABDgAAARZCoSErAoA4AQCDJGmapWmiaGmaKHqmqKqiKKqq5Xmm6ZmmqnqiqaqmqrquqaqubHmeaXqmqKqeKaqqqaqua6qq64qqasumq9q26aq27MqybruyrNueqsq2qbqybqqubbuybOuuLNu65Hmq6pmm63qm6bqq69qy6rqy7Zmm64qqK9um68qy68q2rcqyrmum6bqiq9quqbqy7cqubbuyrPum6+q26sq6rsqy7tu2rvuyrQu76Lq2rsqurquyrOuyLeu2bNtCyfNU1TNN1/VM03VV17Vt1XVtWzNN1zVdV5ZF1XVl1ZV1XXVlW/dM03VNV5Vl01VlWZVl3XZlV5dF17VtVZZ9XXVlX5dt3fdlWdd903V1W5Vl21dlWfdlXfeFWbd93VNVWzddV9dN19V9W9d9YbZt3xddV9dV2daFVZZ139Z9ZZh1nTC6rq6rtuzrqizrvq7rxjDrujCsum38rq0Lw6vrxrHrvq7cvo9q277w6rYxvLpuHLuwG7/t+8axqaptm66r66Yr67ps675v67pxjK6r66os+7rqyr5v67rw674vDKPr6roqy7qw2rKvy7ouDLuuG8Nq28Lu2rpwzLIuDLfvK8evC0PVtoXh1XWjq9vGbwvD0jd2vgAAgAEHAIAAE8pAoSErAoA4AQAGIQgVYxAqxiCEEFIKIaRUMQYhYw5KxhyUEEpJIZTSKsYgZI5JyByTEEpoqZTQSiilpVBKS6GU1lJqLabUWgyhtBRKaa2U0lpqKbbUUmwVYxAy56RkjkkopbRWSmkpc0xKxqCkDkIqpaTSSkmtZc5JyaCj0jlIqaTSUkmptVBKa6GU1kpKsaXSSm2txRpKaS2k0lpJqbXUUm2ttVojxiBkjEHJnJNSSkmplNJa5pyUDjoqmYOSSimplZJSrJiT0kEoJYOMSkmltZJKK6GU1kpKsYVSWmut1ZhSSzWUklpJqcVQSmuttRpTKzWFUFILpbQWSmmttVZrai22UEJroaQWSyoxtRZjba3FGEppraQSWympxRZbja21WFNLNZaSYmyt1dhKLTnWWmtKLdbSUoyttZhbTLnFWGsNJbQWSmmtlNJaSq3F1lqtoZTWSiqxlZJabK3V2FqMNZTSYikptZBKbK21WFtsNaaWYmyx1VhSizHGWHNLtdWUWouttVhLKzXGGGtuNeVSAADAgAMAQIAJZaDQkJUAQBQAAGAMY4xBaBRyzDkpjVLOOSclcw5CCCllzkEIIaXOOQiltNQ5B6GUlEIpKaUUWyglpdZaLAAAoMABACDABk2JxQEKDVkJAEQBACDGKMUYhMYgpRiD0BijFGMQKqUYcw5CpRRjzkHIGHPOQSkZY85BJyWEEEIppYQQQiillAIAAAocAAACbNCUWByg0JAVAUAUAABgDGIMMYYgdFI6KRGETEonpZESWgspZZZKiiXGzFqJrcTYSAmthdYyayXG0mJGrcRYYioAAOzAAQDswEIoNGQlAJAHAEAYoxRjzjlnEGLMOQghNAgx5hyEECrGnHMOQggVY845ByGEzjnnIIQQQueccxBCCKGDEEIIpZTSQQghhFJK6SCEEEIppXQQQgihlFIKAAAqcAAACLBRZHOCkaBCQ1YCAHkAAIAxSjknJaVGKcYgpBRboxRjEFJqrWIMQkqtxVgxBiGl1mLsIKTUWoy1dhBSai3GWkNKrcVYa84hpdZirDXX1FqMtebce2otxlpzzrkAANwFBwCwAxtFNicYCSo0ZCUAkAcAQCCkFGOMOYeUYowx55xDSjHGmHPOKcYYc8455xRjjDnnnHOMMeecc845xphzzjnnnHPOOeegg5A555xz0EHonHPOOQghdM455xyEEAoAACpwAAAIsFFkc4KRoEJDVgIA4QAAgDGUUkoppZRSSqijlFJKKaWUUgIhpZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoplVJKKaWUUkoppZRSSimlACDfCgcA/wcbZ1hJOiscDS40ZCUAEA4AABjDGISMOSclpYYxCKV0TkpJJTWMQSilcxJSSimD0FpqpaTSUkoZhJRiCyGVlFoKpbRWaymptZRSKCnFGktKqaXWMuckpJJaS622mDkHpaTWWmqtxRBCSrG11lJrsXVSUkmttdZabS2klFprLcbWYmwlpZZaa6nF1lpMqbUWW0stxtZiS63F2GKLMcYaCwDgbnAAgEiwcYaVpLPC0eBCQ1YCACEBAAQySjnnnIMQQgghUoox56CDEEIIIURKMeacgxBCCCGEjDHnIIQQQgihlJAx5hyEEEIIIYRSOucghFBKCaWUUkrnHIQQQgillFJKCSGEEEIopZRSSikhhBBKKaWUUkopJYQQQiillFJKKaWEEEIopZRSSimllBBCKKWUUkoppZQSQgihlFJKKaWUUkIIpZRSSimllFJKKCGEUkoppZRSSgkllFJKKaWUUkopIZRSSimllFJKKaUAAIADBwCAACPoJKPKImw04cIDEAAAAAIAAkwAgQGCglEIAoQRCAAAAAAACAD4AABICoCIiGjmDA4QEhQWGBocHiAiJAAAAAAAAAAAAAAAAARPZ2dTAATvCgAAAAAAAJCCocwCAAAAgF0wugwbFRAWHhURIhgSEz2K2FC79QPcEAAAAFAxp6zHdIoJ0t17EwAAAACa3NBhWjMkAACCCQAAAAD1o9ERPAaa29C4md0AAPB7GwAAAAC+jtnQYbtdAwDAXXugEAAAsIfnlbGpZ5bdKHj3AvfPAADAuOpYzX5XEAAAACR04c66hXDuDp7eSDlHQgAAAADHMgDAyIYy0wAAAJrdO4SIJJggAAAAhHDYAAAAhtnr4LkXyHu6rgMAANasx14br7a9Jk0AAEDu0w/CIVIDAJrf2Ln7kQno/5gIAABYIGr2rKQBgPGKMZ7e2KnzIQnwewEAAADuDjwAAJbczqm5SAJ8lgAAAABb3rdVAAB6zBNr0F4A/ZW7JgBUBiGFE2zzq9XSCAkAALnt/DdeGrrUROabUZamuYfuoDLNw5c7oRxn5tjrD1t4F5gw"")
  chime_mp3     = b:(""SUQzBAAAAAAAI1RTU0UAAAAPAAADTGF2ZjYwLjE2LjEwMAAAAAAAAAAAAAAA/+M4wAAAAAAAAAAAAEluZm8AAAAPAAAABwAABMgAS0tLS0tLS0tLS0tLS0tpaWlpaWlpaWlpaWlpaYeHh4eHh4eHh4eHh4eHpaWlpaWlpaWlpaWlpaWlw8PDw8PDw8PDw8PDw8Ph4eHh4eHh4eHh4eHh4f//////////////////AAAAAExhdmM2MC4zMQAAAAAAAAAAAAAAACQCkQAAAAAAAATICn4YBAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/+MoxAAciSo8AVtgAf//zzzzzsUlJSUkojEYjD/v+/7ltbYesOqdU6RCEgAAhhwwZEWGZGRlgwSFJsbyeRlHmYR1kobwtGcCRVDzJS8zMtMfCwwIS8LuIoLEYgzhnDXAPg3EcSzMzMzM/MDBYsWLFixxfcPDw8MAAAAAA8PDx6qcidh3su44//+/jT14z2rt/+MoxBAfSVJMAZ3wANlv//iQGAwHDCcCTI1+2RIIguHJinLnu2YAAoaFPSbraMJjqAJFABYGBBFgC1iG7gUGYeoG5gRAPNKgeWmu6nEYMgJABAH//Yk8rrx2VTlJXxrZ7xy13/////tW7F3l8EgGE1f4sGGjkI//YPRVwz1jnU53Cpjbwu1rFPcrUUboKWjf/+MoxBUfSUZYAd14Acguei7qMzVuXgRAEgQAAMmB4CGApcm8UfmtQomCAwiw8GCwBiMLjDIADEcWTDIcTUuLTq+hzbMfTH4mAEQIKCuWlqwkIEunFOZheDnfNayh8KLCZG+fUR/aJuks9KUlUe05nlntO7v9KieaK7qEQkRDCS1R1a818Zv35ZDUB1oXBL8M/+MoxBoauTJUAA+0bRW3EQFpgWAAmAkBaYEIIBhCDamnUEoYboOphBAGlUAQwEwAhEAWYBoBhtzBygByYpjoxmjotTMQAgOdXQzvCWUVPPWOVOXegAerqeivev+/eeQAOU2/AA+r6ur26KnqdFSTmKCLGRsTJaD0hRhHIqQXUgYVEBpRwrcPqSA4AAgwgGBk/+MoxDIZkUZ9n1TgAFFZwCVGJBOYwBBaZnMNy19bk9cksgh7HdNKcO6u3rXliSiUs2j/V/+dkaWyP5mv//Z+j/oV/0zjFH7vJccAxhi+9RoMsRYSAC74DgAMAED0AwN0B5NHNBHAGALgBQGDHgJQW5CxIBoAIAGAKAMAvAcQMDUA7AMU5C1wMjgAbwMDjAGf/+MoxE4veuo4AY24AIGAlFo4GJxAn4GAPgGwthPk4aLAwiQMDAODOQMAZAHQBABkCIAcFrIIgAkLe//c3ZPtYLnBPQtI9DCGQFzkNIcWv/+/+SJEyKFIony2ThfMTI9/v//Q+eL5cMkT580M0UkDyf//4JBsCA6HAGCQbAiAAAgICADpYRht/qUHiX+AkFoo/+MoxBMa6zIpWYegABX/jpFhAXBA22AxgsDX/v/wM26AxokAggIQhcyDewjog///k4UieHSTJoms1KX//+Xk0FGxRMUHWiZf///mSbKSMTFB1omST////+ykiiYoPRKRkm1IomKFTEFNRTMuMTAwVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVV"")
  readme_text   = b:(""SGVsbG8gZnJvbSBhIGJsb2IhClRoaXMgdGV4dCBpcyBzdG9yZWQgYXMgYmFzZTY0IGluc2lkZSB0aGUgLm1kaXggZmlsZS4KT3BlbiBpdCBmcm9tIHRoZSBFeHBsb3JlciB0byByZWFkIGl0Lgo="")
  noise_binary  = b:(""3q2+7wABAgOlTcoYJTC7HW0TLN7WI3su2R4/ch/LGXEXRJTWSTydXDRgvjEgHmn+2qDu6LmZf1x8KZn9r+WTJQ=="")
  empty_blob    = b:("""")

  // ── The same kinds inside an object and a list ─────────────────────────
  profile = {
    name        = ""Ada"",
    joined      = 2024-03-09,
    last_seen   = 2025-06-01T08:15:30Z,
    handle_rule = r:(""^[a-z]+$""),
    avatar      = b:(""iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAABnklEQVR42u2Wv2sCMRTHv5YOyd/h6ugkTo6NSzkcijjI4VJxkUMyOTiIiLiIkxwOIg4luPjoIrdUbuvm6uDg/9GhUTyMvR/U7UKGHO/l873k5eUlA2wYcOn8zjiZiQPPYHhoSwUiCPB0i/5NYOSJ60+3RJdx/TBYZaVxVgYv4Ym2JHFPVQlq7we/43VOJkm0pdJ0x6LrmU0lAFzoq7xMskVqKQDYVWIAD3q6VertNH1elEliQK4AULWJmXx6W00/Hr8A+DYlDbLJYbjR9HFZ/uEG4Akcxu5NBQDLoVvT5EzvVyQ4yCEAhakwcsJWcGOdLTS9W5NR1hpPYDHT9E5DRtzMGAIfE01vtWT0aIVddmfrZqjpjY4MdQ4GmcHYSy4BUE0Bhu2ZXuvKW0/RFAB8l4yc8GO662l6pS/jnuaQGAhF+7aml8dmumUJAJ4yJ2OIwP49kKsOBXLVFvqOIiLO4le0Q13Ti3P97yNhuFOVR4zHrwenN03PryQA2ycGTAvBeuATA1hYCDP4NteD0+sgt5YPfHhlP2X6qkhfdtHaD6GBd8gTY3LRAAAAAElFTkSuQmCC""),
    bio         = `Written by hand.
Second line of the bio.`
  }

  holidays = [2026-01-01, 2026-07-04, 2026-12-25]
  blocked_names = [r:(""^admin""), r:(""^root$""), r:(""(?i)^test"")]

  // ── Computed rows are not literals: they stay read-only ────────────────
  // (Change the arguments in the Editor tab instead.)
  items::
    makeItem(1, ""Iron Sword"",    50),
    makeItem(2, ""Health Potion"", 25),
    makeItem(3, ""Dragon Scale"",  5000)
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
";

        public const string Numbers = @"// ───────────────────────────────────────────────────────────────────────────
//  Numbers, enums, arrays and tuples
//
//  Plain numbers, hex and binary integers, colours and enum members can be edited in the
//  Explorer. Arrays and tuples are listed for the editors that are still to come: today they
//  show their items, and each item that is a plain literal is editable on its own.
// ───────────────────────────────────────────────────────────────────────────

@CONFIG(
  version -> ""1.0.0""
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
  names             = [""Ann"", ""Bob"", ""Cy""]
  switches          = [true, false, true]
  ratios            = [0.25, 0.5, 1.0]
  hex_list          = [0x01, 0x02, 0xFF]
  palette           = [#F00, #0F0, #00F, #FF08]
  nothing           = []
  rarities          = [Rarity.COMMON, Rarity.EPIC]
  grid              = [[1, 2], [3, 4]]

  // ── Tuples ─────────────────────────────────────────────────────────────
  spawn_point       = t:(1.5, 2.5, 0.0)
  labelled          = t:(""Sword"", 3, true)
  nested_tuple      = t:(1, t:(2, 3))
  enum_tuple        = t:(Difficulty.HARD, 14, ""boss"")

  // ── An object that mixes them ──────────────────────────────────────────
  player = {
    name       = ""Hero"",
    hp         = 100,
    rarity     = Rarity.RARE,
    flags      = 0b0000_1010,
    tint       = #3B7EF6,
    inventory  = [""Sword"", ""Shield""]
  }

  // A ""name::"" list is written without brackets. It runs to the end of the section, so it comes last.
  testArray::
    1, 2, 3, 4, 5
)
";

        public const string Readme = @"# MDIX Studio Explorer tour

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
| `tint_rgb`, `tint_rgba`, `tint_rrggbb`, `tint_rrggbbaa`, `palette`, `player.tint` | edit in place. `#RGBA` (4 digits) works too, and keeps its length. |
| `difficulty`, `drop_rarity`, `mood`, `rarities`, `player.rarity` | open the enum picker, which lists the members from the `@ENUMS` section (with their numbers when the declaration gave one). |
| `scores`, `names`, `switches`, `ratios`, `hex_list`, `palette`, `grid`, `testArray`, `player.inventory` | edit each item on its own. |

Not editable yet: adding, removing and reordering array items, and tuples (`spawn_point`, `labelled`, `nested_tuple`, `enum_tuple`). They are listed so you can see them; the editors for them are the next step.
";
    }
}
