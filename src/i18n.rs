//! Interface language, read from the `.lng` files in `assets/locales/`.
//!
//! Each language is one file, `<id>.lng`: `eng.lng` (English, the reference
//! and the fallback), `fr.lng`, `ko.lng`… Each line is `key = "value"`, with
//! `#` comments and blank lines; values take the escapes `\n`, `\t`, `\"` and
//! `\\`. Keys are grouped by the part of the game that shows them
//! (`settings.language`, `hud.…`, `action.gear`…). The `meta.*` keys
//! describe the file itself:
//!
//! ```text
//! meta.language_name = "Français"   # shown in the language picker
//! meta.locale = "fr"                # matched against LANG, sent to map services
//! meta.plural = "zero_one"          # "one" (n = 1 singular), "zero_one", "none"
//! ```
//!
//! The files are compiled in as a safety net, and the copies on disk are laid
//! over them at start-up: an edited or new `.lng` file shows up with no
//! rebuild. They are looked for in `$GTW_LOCALES` (or the old
//! `$FABLEFLIGHT_LOCALES`), next to the executable (`locales/`,
//! `assets/locales/`), in `./assets/locales`, in the source tree, and in
//! `<config dir>/gtw/locales` for the player's own.
//!
//! In the code:
//! - `t!("settings.language")` is the text for a key (`&'static str`);
//! - `t!("hud.speed", val = speed)` or `t!("hud.dist", d, unit)` fills in
//!   `{val}`, `{}`, `{0}`, with the specs `{:.1}`, `{:+.0}`, `{:03.0}`,
//!   `{:>8}` (fill, align, sign, zero, width, precision);
//! - `tr`/`tr_args` are the same as functions;
//! - `tr_en("London")` translates text that arrives in English at run time
//!   (place names, the controls list): it finds the key whose English that is;
//! - `plural(n, "mp.pilots")` uses `mp.pilots.one` / `mp.pilots.other`;
//! - `caps(..)` translates the damage messages the flight model writes in
//!   capitals, from the `damage.*` keys.
//!
//! A key missing from the current language falls back to English; one missing
//! from English too shows the key itself, with a warning in the log, once.

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};

/// The id of the reference language, whose file every other falls back to.
pub const BASE: &str = "eng";

/// The languages that ship with the game, compiled in.
const BUILTIN: &[(&str, &str)] = &[
    ("eng", include_str!("../assets/locales/eng.lng")),
    ("fr", include_str!("../assets/locales/fr.lng")),
    ("ko", include_str!("../assets/locales/ko.lng")),
];

// ---------------------------------------------------------------- parsing

/// A line of a `.lng` file that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub msg: &'static str,
}

/// The `key = "value"` pairs of a `.lng` file, in order, and the lines that
/// were not understood (they are skipped, so one typo costs one string).
pub fn parse_lng(src: &str) -> (Vec<(String, String)>, Vec<ParseError>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for (i, raw) in src.lines().enumerate() {
        let line = raw.trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(line) {
            Ok(kv) => out.push(kv),
            Err(msg) => errors.push(ParseError { line: i + 1, msg }),
        }
    }
    (out, errors)
}

fn parse_line(line: &str) -> Result<(String, String), &'static str> {
    let (key, rest) = line.split_once('=').ok_or("expected key = \"value\"")?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')) {
        return Err("bad key: use letters, digits, '_', '-' and '.'");
    }
    let rest = rest.trim_start();
    let body = rest.strip_prefix('"').ok_or("the value must be in double quotes")?;
    let mut value = String::with_capacity(body.len());
    let mut chars = body.chars();
    loop {
        match chars.next().ok_or("missing closing quote")? {
            '"' => break,
            '\\' => value.push(match chars.next().ok_or("missing closing quote")? {
                'n' => '\n',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                _ => return Err("unknown escape: use \\n, \\t, \\\" or \\\\"),
            }),
            c => value.push(c),
        }
    }
    let tail = chars.as_str().trim();
    if !tail.is_empty() && !tail.starts_with('#') {
        return Err("unexpected text after the closing quote");
    }
    Ok((key.to_string(), value))
}

// ---------------------------------------------------------------- catalogs

/// How a language picks the singular in `plural`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PluralRule {
    /// 1 is singular (English)
    One,
    /// 0 and 1 are singular (French)
    ZeroOne,
    /// no plural (Korean)
    None,
}

/// One language's strings.
#[derive(Debug)]
pub struct Catalog {
    pub id: &'static str,
    pub name: &'static str,
    pub locale: &'static str,
    pub plural: PluralRule,
    map: HashMap<&'static str, &'static str>,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl Catalog {
    fn new(id: &str) -> Catalog {
        Catalog { id: leak(id.to_string()), name: "", locale: "", plural: PluralRule::One, map: HashMap::new() }
    }

    /// Lays the pairs of a `.lng` file over this catalog. The strings live as
    /// long as the game: lookups hand out `&'static str` with no copying.
    fn merge(&mut self, pairs: Vec<(String, String)>) {
        for (k, v) in pairs {
            let v = leak(v);
            match k.as_str() {
                "meta.language_name" => self.name = v,
                "meta.locale" => self.locale = v,
                "meta.plural" => {
                    self.plural = match v {
                        "zero_one" => PluralRule::ZeroOne,
                        "none" => PluralRule::None,
                        _ => PluralRule::One,
                    }
                }
                _ => {}
            }
            self.map.insert(leak(k), v);
        }
    }

    /// A catalog from the text of a `.lng` file (for tests and tools).
    pub fn from_source(id: &str, src: &str) -> Catalog {
        let mut c = Catalog::new(id);
        c.merge(parse_lng(src).0);
        c.finish();
        c
    }

    fn finish(&mut self) {
        if self.name.is_empty() {
            self.name = self.id;
        }
        if self.locale.is_empty() {
            self.locale = self.id;
        }
    }

    pub fn get(&self, key: &str) -> Option<&'static str> {
        self.map.get(key).copied()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Every language found, and which English string is which key.
#[derive(Debug)]
pub struct Registry {
    langs: Vec<Catalog>,
    /// English text → its key, for `tr_en` and `caps`
    by_english: HashMap<&'static str, &'static str>,
    /// the damage phrases (English, key), longest first
    damage: Vec<(&'static str, &'static str)>,
}

impl Registry {
    /// A registry from catalogs; the first one with the id `eng` is the base.
    pub fn new(mut langs: Vec<Catalog>) -> Registry {
        if !langs.iter().any(|c| c.id == BASE) {
            langs.push(Catalog::from_source(BASE, ""));
        }
        langs.sort_by(|a, b| (a.id != BASE).cmp(&(b.id != BASE)).then(a.id.cmp(b.id)));
        let mut by_english = HashMap::new();
        let mut damage = Vec::new();
        // The `text.*` keys are the ones written for `tr_en`: they win.
        let mut keys: Vec<(&&str, &&str)> = langs[0].map.iter().collect();
        keys.sort_by_key(|(k, _)| (k.starts_with("text."), **k));
        for (k, v) in keys {
            by_english.insert(*v, *k);
            if k.starts_with("damage.") && !k.starts_with("damage.side.") && !k.starts_with("damage.cause.") && **k != *"damage.oil_leak" {
                damage.push((*v, *k));
            }
        }
        damage.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(a.1.cmp(b.1)));
        Registry { langs, by_english, damage }
    }

    /// The built-in files with the ones on disk laid over them.
    pub fn load(dirs: &[PathBuf]) -> Registry {
        let mut langs: Vec<Catalog> = Vec::new();
        let mut add = |id: &str, src: &str, from: &dyn Display| {
            let (pairs, errors) = parse_lng(src);
            for e in errors {
                log::warn!("{from}:{}: {}", e.line, e.msg);
            }
            let i = match langs.iter().position(|c| c.id == id) {
                Some(i) => i,
                None => {
                    langs.push(Catalog::new(id));
                    langs.len() - 1
                }
            };
            langs[i].merge(pairs);
        };
        for (id, src) in BUILTIN {
            add(id, src, &format_args!("built-in {id}.lng"));
        }
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "lng")).collect();
            files.sort();
            for f in files {
                let Some(id) = f.file_stem().and_then(|s| s.to_str()) else { continue };
                match std::fs::read_to_string(&f) {
                    Ok(src) => add(id, &src, &f.display()),
                    Err(e) => log::warn!("{}: {e}", f.display()),
                }
            }
        }
        for c in &mut langs {
            c.finish();
        }
        Registry::new(langs)
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.langs.iter().position(|c| c.id == id)
    }

    /// The text of a key in a language, or in English, or none.
    pub fn lookup(&self, lang: usize, key: &str) -> Option<&'static str> {
        self.langs.get(lang).and_then(|c| c.get(key)).or_else(|| self.langs[0].get(key))
    }

    /// The translation of an English string into a language, if it has one.
    pub fn english(&self, lang: usize, en: &str) -> Option<&'static str> {
        let key = self.by_english.get(en)?;
        self.langs.get(lang)?.get(key)
    }
}

// ---------------------------------------------------------------- state

static REGISTRY: RwLock<Option<&'static Registry>> = RwLock::new(None);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Where `.lng` files are read from, later ones laid over earlier ones.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/locales")), PathBuf::from("assets/locales")];
    if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(exe.join("assets/locales"));
        dirs.push(exe.join("locales"));
    }
    if let Some(d) = std::env::var_os("GTW_LOCALES").or_else(|| std::env::var_os("FABLEFLIGHT_LOCALES")) {
        dirs.push(PathBuf::from(d));
    }
    if let Some(d) = crate::paths::config() {
        dirs.push(d.join("locales"));
    }
    // The same folder reached two ways is read once.
    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.canonicalize().unwrap_or_else(|_| d.clone())));
    dirs
}

fn registry() -> &'static Registry {
    if let Some(r) = *REGISTRY.read().unwrap_or_else(|e| e.into_inner()) {
        return r;
    }
    let mut w = REGISTRY.write().unwrap_or_else(|e| e.into_inner());
    w.get_or_insert_with(|| Box::leak(Box::new(Registry::load(&search_dirs()))))
}

/// Reads the `.lng` files again (new languages, edited strings), keeping the
/// current language when it is still there. The old strings stay in memory:
/// this is for a player's "rescan", not for every frame.
pub fn reload() {
    let cur = lang();
    let r: &'static Registry = Box::leak(Box::new(Registry::load(&search_dirs())));
    *REGISTRY.write().unwrap_or_else(|e| e.into_inner()) = Some(r);
    ACTIVE.store(r.index(cur.0).unwrap_or(0), Ordering::Relaxed);
    log::info!("languages: {}", r.langs.iter().map(|c| format!("{} ({}, {} strings)", c.id, c.name, c.len())).collect::<Vec<_>>().join(", "));
}

// ---------------------------------------------------------------- languages

/// A language that has a `.lng` file (its id is the file's name).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Lang(&'static str);

impl Lang {
    pub const EN: Lang = Lang(BASE);

    fn catalog(self) -> &'static Catalog {
        let r = registry();
        &r.langs[r.index(self.0).unwrap_or(0)]
    }

    /// The text for a key in this language (not necessarily the current one).
    pub fn text(self, key: &str) -> &'static str {
        let r = registry();
        r.lookup(r.index(self.0).unwrap_or(0), key).unwrap_or_else(|| warn_once("no text for", key))
    }

    /// Every language found, English first.
    pub fn all() -> Vec<Lang> {
        registry().langs.iter().map(|c| Lang(c.id)).collect()
    }

    /// The language's own name, for the language picker.
    pub fn name(self) -> &'static str {
        self.catalog().name
    }

    /// The file's id ("eng", "fr", "ko"): what the settings keep.
    pub fn code(self) -> &'static str {
        self.0
    }

    /// The language code ("en", "fr", "ko") for the system and web services.
    pub fn locale(self) -> &'static str {
        self.catalog().locale
    }

    /// A language from an id ("fr"), a locale ("fr_BE.UTF-8", "en") or a
    /// file's name; None when there is no file for it.
    pub fn parse(s: &str) -> Option<Lang> {
        let s = s.trim().to_ascii_lowercase();
        if s.is_empty() {
            return None;
        }
        let r = registry();
        let exact = r.langs.iter().find(|c| c.id.eq_ignore_ascii_case(&s));
        let by_locale = || {
            r.langs.iter().find(|c| {
                let l = c.locale.to_ascii_lowercase();
                s == l || s.strip_prefix(&l).is_some_and(|rest| rest.starts_with(['_', '-', '.', '@']))
            })
        };
        exact.or_else(by_locale).map(|c| Lang(c.id))
    }

    /// The system's language, from the usual locale variables (Linux, macOS);
    /// English when there is no file for it.
    pub fn detect() -> Lang {
        ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
            .iter()
            .filter_map(|k| std::env::var(k).ok())
            .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
            .and_then(|v| v.split(':').find_map(Lang::parse))
            .unwrap_or(Lang::EN)
    }
}

pub fn lang() -> Lang {
    let r = registry();
    Lang(r.langs.get(ACTIVE.load(Ordering::Relaxed)).unwrap_or(&r.langs[0]).id)
}

/// Switches the language; everything drawn after this uses it.
pub fn set_lang(l: Lang) {
    ACTIVE.store(registry().index(l.0).unwrap_or(0), Ordering::Relaxed);
}

/// The current language's code ("en", "fr"…), for grammar that only one
/// language needs (French "de"/"d'" before a place name).
pub fn locale() -> &'static str {
    lang().locale()
}

// ---------------------------------------------------------------- lookups

/// Keys already reported missing, so the log says it once.
fn warn_once(what: &str, key: &str) -> &'static str {
    static WARNED: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut w = WARNED.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(k) = w.get(key) {
        return k;
    }
    log::warn!("i18n: {what} \"{key}\"");
    let k = leak(key.to_string());
    w.insert(key.to_string(), k);
    k
}

/// The text for a key in the current language, or in English, or the key.
pub fn tr(key: &str) -> &'static str {
    let r = registry();
    r.lookup(ACTIVE.load(Ordering::Relaxed), key).unwrap_or_else(|| warn_once("no text for", key))
}

/// The text for a key, if any language has it (no warning).
pub fn try_tr(key: &str) -> Option<&'static str> {
    registry().lookup(ACTIVE.load(Ordering::Relaxed), key)
}

/// The text for a key with its `{placeholders}` filled in.
pub fn tr_args(key: &str, args: &[(&str, &dyn Display)]) -> String {
    format_template(tr(key), args)
}

/// Text that arrives in English at run time (a place name, a table entry),
/// in the current language: the English itself when there is no translation.
pub fn tr_en(en: &'static str) -> &'static str {
    registry().english(ACTIVE.load(Ordering::Relaxed), en).unwrap_or(en)
}

/// The same for text that is not `'static`.
pub fn tr_en_owned(en: &str) -> String {
    registry().english(ACTIVE.load(Ordering::Relaxed), en).unwrap_or(en).to_string()
}

/// The translation of an English string into a given language, if it has one.
pub fn tr_en_in(lang: Lang, en: &str) -> Option<&'static str> {
    let r = registry();
    r.english(r.index(lang.0)?, en)
}

/// "3 pilots": `{key}.one` or `{key}.other`, with `{n}` filled in.
pub fn plural(n: u64, key: &str) -> String {
    let rule = lang().catalog().plural;
    let one = match rule {
        PluralRule::One => n == 1,
        PluralRule::ZeroOne => n <= 1,
        PluralRule::None => false,
    };
    let form = if one { "one" } else { "other" };
    format_template(tr(&format!("{key}.{form}")), &[("n", &n)])
}

/// `t!("key")`, or with arguments `t!("key", a, b, name = c)`.
macro_rules! t {
    ($key:literal $(,)?) => {
        $crate::i18n::tr($key)
    };
    ($key:literal, $($args:tt)+) => {
        $crate::i18n::tr_args($key, &$crate::i18n::targs!([] $($args)+))
    };
}
pub(crate) use t;

/// The argument list of `t!`: `name = value` or a plain value, in order.
macro_rules! targs {
    ([$($acc:tt)*]) => { [$($acc)*] };
    ([$($acc:tt)*] $name:ident = $val:expr $(, $($rest:tt)*)?) => {
        $crate::i18n::targs!([$($acc)* (stringify!($name), &$val as &dyn ::std::fmt::Display),] $($($rest)*)?)
    };
    ([$($acc:tt)*] $val:expr $(, $($rest:tt)*)?) => {
        $crate::i18n::targs!([$($acc)* ("", &$val as &dyn ::std::fmt::Display),] $($($rest)*)?)
    };
}
pub(crate) use targs;

// ---------------------------------------------------------------- formatting

/// Fills in a template the way `format!` does, at run time: `{}` takes the
/// next unnamed argument, `{0}` one by position, `{name}` one by name, and
/// `{{` / `}}` are braces. Specs: `[[fill]align][+][0][width][.precision]`.
/// An unknown placeholder is left as written.
pub fn format_template(tpl: &str, args: &[(&str, &dyn Display)]) -> String {
    let positional: Vec<&dyn Display> = args.iter().filter(|(n, _)| n.is_empty()).map(|(_, v)| *v).collect();
    let mut next = 0;
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut rest = tpl;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let c = rest.as_bytes()[i];
        let after = &rest[i + 1..];
        if after.as_bytes().first() == Some(&c) {
            out.push(c as char);
            rest = &after[1..];
            continue;
        }
        if c == b'}' {
            out.push('}');
            rest = after;
            continue;
        }
        let Some(end) = after.find('}') else {
            out.push_str(&rest[i..]);
            return out;
        };
        let inner = &after[..end];
        let (name, spec) = inner.split_once(':').unwrap_or((inner, ""));
        let value = if name.is_empty() {
            next += 1;
            positional.get(next - 1).copied()
        } else if let Ok(k) = name.parse::<usize>() {
            positional.get(k).copied()
        } else {
            args.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
        };
        match (value, Spec::parse(spec)) {
            (Some(v), Some(s)) => s.write(&mut out, v),
            _ => out.push_str(&rest[i..i + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Default, PartialEq)]
struct Spec {
    fill: Option<char>,
    align: Option<char>,
    plus: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
}

impl Spec {
    fn parse(s: &str) -> Option<Spec> {
        let mut spec = Spec::default();
        let mut c: Vec<char> = s.chars().collect();
        if c.len() >= 2 && matches!(c[1], '<' | '>' | '^') {
            spec.fill = Some(c[0]);
            spec.align = Some(c[1]);
            c.drain(..2);
        } else if c.first().is_some_and(|a| matches!(a, '<' | '>' | '^')) {
            spec.align = Some(c[0]);
            c.remove(0);
        }
        let mut s: &[char] = &c;
        if s.first() == Some(&'+') {
            spec.plus = true;
            s = &s[1..];
        }
        if s.first() == Some(&'0') {
            spec.zero = true;
            s = &s[1..];
        }
        let digits = |s: &mut &[char]| {
            let n = s.iter().take_while(|c| c.is_ascii_digit()).count();
            let v: String = s[..n].iter().collect();
            *s = &s[n..];
            v.parse::<usize>().ok()
        };
        spec.width = digits(&mut s).unwrap_or(0);
        if s.first() == Some(&'.') {
            s = &s[1..];
            spec.precision = Some(digits(&mut s)?);
        }
        s.is_empty().then_some(spec)
    }

    fn write(&self, out: &mut String, v: &dyn Display) {
        let (w, p) = (self.width, self.precision.unwrap_or(0));
        // Without an explicit alignment, the value's own `Display` pads it
        // (numbers to the right, text to the left), as `format!` would.
        let custom = self.align.is_some();
        let w0 = if custom { 0 } else { w };
        let s = match (self.plus, self.zero && !custom, self.precision.is_some()) {
            (false, false, false) => format!("{v:w0$}"),
            (false, false, true) => format!("{v:w0$.p$}"),
            (false, true, false) => format!("{v:0w0$}"),
            (false, true, true) => format!("{v:0w0$.p$}"),
            (true, false, false) => format!("{v:+w0$}"),
            (true, false, true) => format!("{v:+w0$.p$}"),
            (true, true, false) => format!("{v:+0w0$}"),
            (true, true, true) => format!("{v:+0w0$.p$}"),
        };
        let pad = w.saturating_sub(s.chars().count());
        if !custom || pad == 0 {
            out.push_str(&s);
            return;
        }
        let fill = self.fill.unwrap_or(' ');
        let (left, right) = match self.align {
            Some('>') => (pad, 0),
            Some('^') => (pad / 2, pad - pad / 2),
            _ => (0, pad),
        };
        out.extend(std::iter::repeat_n(fill, left));
        out.push_str(&s);
        out.extend(std::iter::repeat_n(fill, right));
    }
}

// ---------------------------------------------------------------- grammar

/// French "de" before a place name, as it is written: "d'Amsterdam",
/// "du Havre", "des Sables-d'Olonne", "de la Rochelle", "de Paris".
/// (No elision before an H: "de Hambourg"; it depends on the word.)
pub fn de_place(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("Le ") {
        return format!("du {rest}");
    }
    if let Some(rest) = name.strip_prefix("Les ") {
        return format!("des {rest}");
    }
    if let Some(rest) = name.strip_prefix("La ") {
        return format!("de la {rest}");
    }
    let vowel = name.chars().next().is_some_and(|c| "AEIOUYÀÂÄÉÈÊËÎÏÔÖÙÛÜaeiouyàâäéèêëîïôöùûü".contains(c));
    if vowel { format!("d'{name}") } else { format!("de {name}") }
}

/// Damage messages ("LEFT WING TORN OFF (FLUTTER)", "R AILERON GONE",
/// "ENGINE 40%  OIL LEAK") in the current language. The flight model writes
/// them in English; the phrases are the `damage.*` keys, and a translation
/// puts the side where its grammar wants it: `{s}` takes
/// `damage.side.left`/`right`, `{sf}` the feminine `damage.side.left_f`/`right_f`.
pub fn caps(s: &str) -> String {
    caps_in(registry(), ACTIVE.load(Ordering::Relaxed), s)
}

fn caps_in(r: &Registry, lang: usize, s: &str) -> String {
    let Some(cat) = r.langs.get(lang) else { return s.to_string() };
    if lang == 0 {
        return s.to_string();
    }
    // The cause, in brackets at the end.
    let (body, why) = match s.strip_suffix(')').and_then(|b| b.rsplit_once(" (")) {
        Some((b, w)) => (b, Some(w)),
        None => (s, None),
    };
    // Which side, at the front.
    let (side, body) = [("LEFT ", "left"), ("RIGHT ", "right"), ("L ", "left"), ("R ", "right")]
        .iter()
        .find_map(|(p, k)| body.strip_prefix(p).map(|b| (Some(*k), b)))
        .unwrap_or((None, body));
    // The longest phrase it starts with, that this language translates.
    let Some((en, text)) = r.damage.iter().find(|(en, _)| body.starts_with(en)).and_then(|(en, k)| cat.get(k).map(|t| (*en, t))) else {
        return s.to_string();
    };
    let word = |fem: bool| match side {
        Some(k) => cat.get(&format!("damage.side.{k}{}", if fem { "_f" } else { "" })).map(|w| format!(" {w}")).unwrap_or_default(),
        None => String::new(),
    };
    let mut rest = body[en.len()..].to_string();
    if let (Some(en_oil), Some(oil)) = (r.langs[0].get("damage.oil_leak"), cat.get("damage.oil_leak")) {
        rest = rest.replace(en_oil, oil);
    }
    let mut out = format!("{}{rest}", text.replace(" {sf}", &word(true)).replace(" {s}", &word(false)));
    if let Some(w) = why {
        let key = r.langs[0].map.iter().find(|(k, v)| k.starts_with("damage.cause.") && **v == w).map(|(k, _)| *k);
        out = format!("{out} ({})", key.and_then(|k| cat.get(k)).unwrap_or(w));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENG: &str = r#"
# English, the reference
meta.language_name = "English"
meta.locale = "en"
meta.plural = "one"

menu.start = "Start flight"
hud.speed = "{val} kt"
hud.only_english = "Only in English"
mp.pilots.one = "{n} pilot"
mp.pilots.other = "{n} pilots"
text.place.london = "London"
damage.wing_torn_off = "WING TORN OFF"
damage.wing = "WING"
damage.engine = "ENGINE"
damage.oil_leak = "OIL LEAK"
damage.cause.flutter = "FLUTTER"
"#;

    const FR: &str = r#"
meta.language_name = "Français"   # the native name
meta.locale = "fr"
meta.plural = "zero_one"

menu.start = "Démarrer le vol"
hud.speed = "{val} nœuds"
mp.pilots.one = "{n} pilote"
mp.pilots.other = "{n} pilotes"
text.place.london = "Londres"
damage.wing_torn_off = "AILE {sf} ARRACHÉE"
damage.engine = "MOTEUR {s}"
damage.oil_leak = "FUITE D'HUILE"
damage.side.left = "GAUCHE"
damage.side.right = "DROIT"
damage.side.left_f = "GAUCHE"
damage.side.right_f = "DROITE"
damage.cause.flutter = "FLOTTEMENT"
"#;

    fn reg() -> Registry {
        Registry::new(vec![Catalog::from_source("fr", FR), Catalog::from_source("eng", ENG)])
    }

    #[test]
    fn parses_comments_blank_lines_and_escapes() {
        let src = "# a comment\n\n  key.a = \"one\"  # trailing\nkey.b=\"line\\nbreak \\\"quoted\\\" back\\\\slash\\ttab\"\n";
        let (pairs, errors) = parse_lng(src);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(pairs, vec![("key.a".into(), "one".into()), ("key.b".into(), "line\nbreak \"quoted\" back\\slash\ttab".into())]);
    }

    #[test]
    fn bad_lines_are_reported_and_skipped() {
        let (pairs, errors) = parse_lng("good = \"yes\"\nno equals sign\nbad key = \"x\"\nopen = \"never closed\nesc = \"\\q\"\nextra = \"a\" b\n");
        assert_eq!(pairs, vec![("good".into(), "yes".into())]);
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), vec![2, 3, 4, 5, 6]);
    }

    #[test]
    fn metadata_and_order() {
        let r = reg();
        let ids: Vec<_> = r.langs.iter().map(|c| (c.id, c.name, c.locale)).collect();
        assert_eq!(ids, vec![("eng", "English", "en"), ("fr", "Français", "fr")]);
        assert_eq!(r.langs[1].plural, PluralRule::ZeroOne);
    }

    #[test]
    fn falls_back_to_english_then_none() {
        let r = reg();
        assert_eq!(r.lookup(1, "menu.start"), Some("Démarrer le vol"));
        assert_eq!(r.lookup(1, "hud.only_english"), Some("Only in English"));
        assert_eq!(r.lookup(1, "no.such.key"), None);
        assert_eq!(r.lookup(0, "menu.start"), Some("Start flight"));
    }

    #[test]
    fn english_text_lookup() {
        let r = reg();
        assert_eq!(r.english(1, "London"), Some("Londres"));
        assert_eq!(r.english(0, "London"), Some("London"));
        assert_eq!(r.english(1, "Atlantis"), None);
    }

    #[test]
    fn placeholders() {
        let n = 3;
        assert_eq!(format_template("{val} kt", &[("val", &"120")]), "120 kt");
        assert_eq!(format_template("{} of {}", &[("", &1), ("", &2)]), "1 of 2");
        assert_eq!(format_template("{1} before {0}", &[("", &"a"), ("", &"b")]), "b before a");
        assert_eq!(format_template("{n} km, {} m", &[("", &500), ("n", &n)]), "3 km, 500 m");
        assert_eq!(format_template("{{literal}} {missing} {", &[]), "{literal} {missing} {");
        // The specs give what `format!` gives.
        let x = -4.56_f64;
        let cases: [(&str, &dyn Display, String); 10] = [
            ("{:.1}", &x, format!("{x:.1}")),
            ("{:+.0}", &x, format!("{x:+.0}")),
            ("{:+.0}", &4.4, format!("{:+.0}", 4.4)),
            ("{:03.0}", &7.2, format!("{:03.0}", 7.2)),
            ("{:05.1}", &x, format!("{x:05.1}")),
            ("{:6}", &42, format!("{:6}", 42)),
            ("{:6}", &"ab", format!("{:6}", "ab")),
            ("{:>6}", &"ab", format!("{:>6}", "ab")),
            ("{:*^7}", &"ab", format!("{:*^7}", "ab")),
            ("{:02}", &5, format!("{:02}", 5)),
        ];
        for (tpl, v, want) in cases {
            assert_eq!(format_template(tpl, &[("", v)]), want, "{tpl}");
        }
    }

    #[test]
    fn plural_rules() {
        let r = reg();
        let form = |lang: usize, n: u64| {
            let one = match r.langs[lang].plural {
                PluralRule::One => n == 1,
                PluralRule::ZeroOne => n <= 1,
                PluralRule::None => false,
            };
            let key = format!("mp.pilots.{}", if one { "one" } else { "other" });
            format_template(r.lookup(lang, &key).unwrap(), &[("n", &n)])
        };
        assert_eq!(form(0, 0), "0 pilots");
        assert_eq!(form(0, 1), "1 pilot");
        assert_eq!(form(1, 0), "0 pilote");
        assert_eq!(form(1, 2), "2 pilotes");
    }

    #[test]
    fn damage_messages() {
        let r = reg();
        assert_eq!(caps_in(&r, 1, "LEFT WING TORN OFF (FLUTTER)"), "AILE GAUCHE ARRACHÉE (FLOTTEMENT)");
        assert_eq!(caps_in(&r, 1, "R WING TORN OFF"), "AILE DROITE ARRACHÉE");
        assert_eq!(caps_in(&r, 1, "R ENGINE 40%  OIL LEAK"), "MOTEUR DROIT 40%  FUITE D'HUILE");
        // English, and phrases this language does not have, stay as written.
        assert_eq!(caps_in(&r, 0, "LEFT WING TORN OFF (FLUTTER)"), "LEFT WING TORN OFF (FLUTTER)");
        assert_eq!(caps_in(&r, 1, "L WING  -1.3 m"), "L WING  -1.3 m");
        assert_eq!(caps_in(&r, 1, "SOMETHING NEW"), "SOMETHING NEW");
    }

    #[test]
    fn de_before_place_names() {
        assert_eq!(de_place("Amsterdam"), "d'Amsterdam");
        assert_eq!(de_place("Évreux"), "d'Évreux");
        assert_eq!(de_place("Paris"), "de Paris");
        assert_eq!(de_place("Le Havre"), "du Havre");
        assert_eq!(de_place("Les Mureaux"), "des Mureaux");
        assert_eq!(de_place("La Rochelle"), "de la Rochelle");
        assert_eq!(de_place("Hambourg"), "de Hambourg");
    }

    // The only test that touches the global language, so the others cannot race it.
    #[test]
    fn shipped_languages() {
        let fr = Lang::parse("fr_BE.UTF-8").unwrap();
        let ko = Lang::parse("ko").unwrap();
        assert_eq!((Lang::parse("en"), Lang::parse("eng"), Lang::parse("xx")), (Some(Lang::EN), Some(Lang::EN), None));
        assert_eq!((fr.name(), fr.code(), fr.locale(), ko.name()), ("Français", "fr", "fr", "한국어"));
        assert_eq!(caps("LEFT WING TORN OFF (FLUTTER)"), "LEFT WING TORN OFF (FLUTTER)");
        set_lang(fr);
        assert_eq!(caps("LEFT WING TORN OFF (FLUTTER)"), "AILE GAUCHE ARRACHÉE (FLOTTEMENT)");
        assert_eq!(caps("R AILERON GONE"), "AILERON DROIT PERDU");
        assert_eq!(caps("RIGHT ELEVATOR TORN OFF (BUILDING)"), "PROFONDEUR DROITE ARRACHÉE (BÂTIMENT)");
        assert_eq!(caps("L WING  -1.3 m"), "AILE GAUCHE  -1.3 m");
        assert_eq!(caps("ENGINE 40%  OIL LEAK"), "MOTEUR 40%  FUITE D'HUILE");
        assert_eq!(caps("LEFT MAIN GEAR GONE"), "TRAIN PRINCIPAL GAUCHE PERDU");
        assert_eq!(caps("NOSE GEAR TORN OFF (HARD IMPACT)"), "TRAIN AVANT ARRACHÉ (CHOC VIOLENT)");
        assert_eq!(caps("NOSE TORN OFF (OVERSPEED)"), "NEZ ARRACHÉ (SURVITESSE)");
        assert_eq!(caps("VISOR GONE"), "VISIÈRE PERDUE");
        assert_eq!(caps("DRAG CHUTE TORN AWAY (OVERSPEED)"), "PARACHUTE-FREIN ARRACHÉ (SURVITESSE)");
        assert_eq!(caps("TAIL BROKEN OFF (OVER G)"), "QUEUE ARRACHÉE (FACTEUR DE CHARGE)");
        assert_eq!(caps("SOMETHING NEW"), "SOMETHING NEW");
        assert_eq!(tr_en("London"), "Londres");
        assert_eq!(tr_en("Paris"), "Paris");
        assert_eq!(tr("action.gear"), "Train d'atterrissage");
        let (active, wind) = ("live", "calm");
        assert_eq!(t!("game.wind_elevation_ft_aircraft_moving", 1234.56, 7, active = active, wind = wind), "live  ·  vent calm  ·  altitude 1235 ft  ·  7 avions en mouvement");
        assert_eq!(plural(1, "settings.requests"), "1 requête");
        assert_eq!(plural(0, "settings.requests"), "0 requête");
        assert_eq!(crate::weather::date_label(171.0), "21 juin");
        // A key only English has, and one nobody has.
        assert_eq!(tr("no.such.key"), "no.such.key");
        set_lang(ko);
        assert_eq!(tr_en("London"), "런던");
        assert_eq!(tr_en("Settings"), "설정");
        assert_eq!(tr_en("a string with no Korean yet"), "a string with no Korean yet");
        assert_eq!(caps("LEFT WING TORN OFF"), "LEFT WING TORN OFF");
        assert_eq!(plural(1, "settings.requests"), "1 requests");
        set_lang(Lang::EN);
        assert_eq!(t!("game.wind_elevation_ft_aircraft_moving", 1234.56, 7, active = "live", wind = "calm"), "live  ·  wind calm  ·  elevation 1235 ft  ·  7 aircraft moving");
        assert_eq!(plural(0, "settings.requests"), "0 requests");
        assert_eq!(plural(1, "settings.requests"), "1 request");
        assert_eq!(crate::weather::date_label(171.0), "Jun 21");
    }

    /// The shipped files: every one parses cleanly, has its metadata, and
    /// uses only placeholders the English has.
    #[test]
    fn shipped_files() {
        let names = |s: &str| {
            let mut v: Vec<String> = Vec::new();
            let mut rest = s;
            while let Some(i) = rest.find('{') {
                if rest[i + 1..].starts_with('{') {
                    rest = &rest[i + 2..];
                    continue;
                }
                let Some(e) = rest[i..].find('}') else { break };
                v.push(rest[i + 1..i + e].split(':').next().unwrap().to_string());
                rest = &rest[i + e + 1..];
            }
            v.sort();
            v
        };
        let eng = Catalog::from_source("eng", BUILTIN[0].1);
        for (id, src) in BUILTIN {
            let (_, errors) = parse_lng(src);
            assert!(errors.is_empty(), "{id}.lng: {errors:?}");
            let c = Catalog::from_source(id, src);
            assert!(c.get("meta.language_name").is_some() && c.get("meta.locale").is_some(), "{id}.lng: meta");
            for (k, v) in &c.map {
                if k.starts_with("meta.") || k.starts_with("damage.") {
                    continue;
                }
                let en = eng.get(k).unwrap_or_else(|| panic!("{id}.lng: {k} is not in eng.lng"));
                // Every placeholder the translation uses, the code passes.
                let (want, got) = (names(en), names(v));
                for n in &got {
                    assert!(want.contains(n) || n.is_empty() && want.iter().filter(|w| w.is_empty()).count() >= got.iter().filter(|g| g.is_empty()).count(), "{id}.lng: {k} uses {{{n}}}");
                }
            }
        }
    }

    /// `tr_en` finds a text by its English, so no two `text.*` keys may share it.
    #[test]
    fn english_table_text_is_unique() {
        let eng = Catalog::from_source("eng", BUILTIN[0].1);
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for (k, v) in eng.map.iter().filter(|(k, _)| k.starts_with("text.")) {
            if let Some(other) = seen.insert(v, k) {
                panic!("{k} and {other} are both \"{v}\"");
            }
        }
    }

    /// Every `t!("key")` in the code has an English text.
    #[test]
    fn every_key_in_the_code_is_in_english() {
        let eng = Catalog::from_source("eng", BUILTIN[0].1);
        let mut missing = Vec::new();
        let mut stack = vec![PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
        while let Some(p) = stack.pop() {
            if p.is_dir() {
                stack.extend(std::fs::read_dir(&p).unwrap().flatten().map(|e| e.path()));
                continue;
            }
            if p.extension().is_none_or(|e| e != "rs") || p.ends_with("i18n.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap();
            for (i, _) in src.match_indices("t!(\"") {
                if src[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &src[i + 4..];
                let key = &rest[..rest.find('"').unwrap()];
                if eng.get(key).is_none() {
                    missing.push(format!("{}: {key}", p.display()));
                }
            }
        }
        assert!(missing.is_empty(), "keys missing from eng.lng:\n{}", missing.join("\n"));
    }
}
