# Translating GTW

The interface text lives in `assets/locales/`, one file per language:
`eng.lng` (English, the reference), `fr.lng`, `ko.lng`… The game reads these
files when it starts, so you can edit a translation, or add a new one, without
rebuilding. In the settings, **General › Language › Rescan** reads them again
while the game is running.

## The format

```text
# A comment
meta.language_name = "Deutsch"   # the name shown in the language picker
meta.locale = "de"               # matched against LANG; sent to map services
meta.plural = "one"              # "one", "zero_one" (French) or "none" (Korean)

settings.interface_language = "Sprache der Oberfläche"
graphics.scenery_reach = "Gebäude und Bäume bis {:.1} km"
mp.pilots.one = "{n} Pilot"
mp.pilots.other = "{n} Piloten"
```

- Each line is `key = "value"`. Blank lines and lines starting with `#` are
  ignored, and so is a `#` comment after the closing quote.
- The escapes in values are `\n`, `\t`, `\"` and `\\`.
- `{}`, `{0}` and `{name}` are filled in by the game. Keep every placeholder
  the English uses, although you may move them around. Numbers take the same
  specs as Rust: `{:.1}` gives one decimal, `{:+.0}` adds a sign, `{:03.0}` pads
  with zeros, and `{:>8}` right-aligns. Write `{{` and `}}` for literal braces.
- Plurals have two keys, `<key>.one` and `<key>.other`. `meta.plural` sets
  which one a count uses.
- The `damage.*` keys translate the flight model's damage messages. In those,
  `{s}` stands for the side (`damage.side.left`/`right`) and `{sf}` for its
  feminine form (`damage.side.left_f`/`right_f`).
- The `text.*` keys hold text that the game looks up by its English, such as
  place names and the controls list.

## Adding a language

1. Copy `eng.lng` to `<code>.lng` in the same folder, for example `de.lng`.
2. Set its `meta.*` keys and translate the values. You can delete any line you
   don't translate: a missing key shows the English.
3. Start the game. The new language appears in the language picker.

To make the language ship with the game, add it to `BUILTIN` in `src/i18n.rs`.
This also compiles it in as a safety net. Then run `cargo test i18n`, which
checks that each file parses, has its `meta` keys and only uses placeholders
the English has.

The game looks for `.lng` files in these places, and each one overrides the
ones before it:

1. the copies compiled into the game;
2. `assets/locales/` in the source tree, and in the folder the game runs from;
3. `assets/locales/` and `locales/` next to the executable;
4. `$GTW_LOCALES` (the old `$FABLEFLIGHT_LOCALES` still works);
5. `<config dir>/gtw/locales/` (`~/.config/gtw/locales` on Linux),
   for your own languages and fixes.

## In the code

```rust
use crate::i18n::t;

t!("settings.interface_language")             // &'static str
t!("graphics.scenery_reach", 7.5 * reach)     // String, fills {}
t!("hud.speed", val = speed)                  // String, fills {val}
i18n::plural(n, "mp.pilots")                  // "3 pilots"
i18n::tr_en(name)                             // English that arrives at run time
```

Every new user-facing string gets a key in `eng.lng`, with a French line in
`fr.lng` where you can write one. `cargo test` fails if code uses a `t!` key
that `eng.lng` lacks. Don't translate log messages, protocol names or file
paths.
