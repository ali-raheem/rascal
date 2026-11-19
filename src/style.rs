//! Character styles and ANSI colour palettes.

use anyhow::{Result, bail};

use crate::config::{Chars, Colors};
use crate::render::Kind;

/// A fill pattern: one or more characters, cycled diagonally across the
/// map. `"#"` fills solid, `"~ "` gives sparse waves.
#[derive(Clone, Debug)]
pub struct Pattern(Vec<char>);

impl Pattern {
    pub fn new(s: &str) -> Self {
        // Allow "\t"-free plain strings; an empty pattern means blank.
        let v: Vec<char> = s.chars().collect();
        Pattern(if v.is_empty() { vec![' '] } else { v })
    }

    pub fn at(&self, r: usize, c: usize) -> char {
        self.0[(r + c) % self.0.len()]
    }

    pub fn first(&self) -> char {
        self.0[0]
    }
}

pub struct StyleDef {
    pub name: &'static str,
    pub about: &'static str,
    pub land: &'static str,
    pub other: &'static str,
    pub water: &'static str,
    pub highlight: &'static str,
    pub marker: &'static str,
    pub grid: &'static str,
}

pub const STYLES: &[StyleDef] = &[
    StyleDef {
        name: "classic",
        about: "plain ASCII: # land, : neighbours",
        land: "#",
        other: ":",
        water: " ",
        highlight: "@",
        marker: "*",
        grid: ".",
    },
    StyleDef {
        name: "sea",
        about: "ASCII with ~ water",
        land: "#",
        other: "+",
        water: "~",
        highlight: "@",
        marker: "X",
        grid: "-",
    },
    StyleDef {
        name: "dots",
        about: "light dotted land",
        land: ".",
        other: "'",
        water: " ",
        highlight: "o",
        marker: "X",
        grid: "+",
    },
    StyleDef {
        name: "blocks",
        about: "unicode block shading, blank sea",
        land: "▓",
        other: "░",
        water: " ",
        highlight: "█",
        marker: "●",
        grid: "·",
    },
    StyleDef {
        name: "shade",
        about: "unicode blocks with shaded sea",
        land: "▓",
        other: "▒",
        water: "░",
        highlight: "█",
        marker: "◆",
        grid: " ",
    },
    StyleDef {
        name: "braille",
        about: "braille-dot texture",
        land: "⣿",
        other: "⠶",
        water: " ",
        highlight: "⣶",
        marker: "◉",
        grid: "⠂",
    },
];

pub fn find_style(name: &str) -> Result<&'static StyleDef> {
    let n = name.to_ascii_lowercase();
    match STYLES.iter().find(|s| s.name == n) {
        Some(s) => Ok(s),
        None => {
            let names: Vec<&str> = STYLES.iter().map(|s| s.name).collect();
            bail!("unknown style '{name}' (available: {})", names.join(", "))
        }
    }
}

/// The effective characters for every map element.
pub struct Resolved {
    pub land: Pattern,
    pub other: Pattern,
    pub water: Pattern,
    pub highlight: Pattern,
    pub marker: String,
    pub grid: Pattern,
    pub border: Option<Pattern>,
    pub coast: Option<Pattern>,
}

pub fn resolve(style: &StyleDef, chars: &Chars) -> Resolved {
    let pick = |v: &Option<String>, d: &str| Pattern::new(v.as_deref().unwrap_or(d));
    let opt = |v: &Option<String>| v.as_deref().filter(|s| !s.is_empty()).map(Pattern::new);
    Resolved {
        land: pick(&chars.land, style.land),
        other: pick(&chars.other, style.other),
        water: pick(&chars.water, style.water),
        highlight: pick(&chars.highlight, style.highlight),
        marker: chars
            .marker
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| style.marker.to_string()),
        grid: pick(&chars.grid, style.grid),
        border: opt(&chars.border),
        coast: opt(&chars.coast),
    }
}

/// SGR parameter strings per element kind.
pub struct Palette {
    sgr: Vec<(Kind, Option<String>)>,
}

impl Palette {
    pub fn get(&self, k: Kind) -> Option<&str> {
        self.sgr
            .iter()
            .find(|(kk, _)| *kk == k)
            .and_then(|(_, s)| s.as_deref())
    }

    pub fn build(c: &Colors) -> Result<Palette> {
        let entries: [(Kind, &Option<String>, &str); 11] = [
            (Kind::Water, &c.water, "blue"),
            (Kind::Land, &c.land, "green"),
            (Kind::Other, &c.other, "bright-black"),
            (Kind::Highlight, &c.highlight, "bright-yellow"),
            (Kind::Border, &c.border, "yellow"),
            (Kind::Coast, &c.coast, "bright-green"),
            (Kind::Grid, &c.grid, "bright-black"),
            (Kind::Marker, &c.marker, "bold bright-red"),
            (Kind::Label, &c.label, "bold bright-white"),
            (Kind::Frame, &c.frame, "none"),
            (Kind::Title, &c.title, "bold"),
        ];
        let mut sgr = Vec::new();
        for (k, v, d) in entries {
            sgr.push((k, parse_color(v.as_deref().unwrap_or(d))?));
        }
        Ok(Palette { sgr })
    }
}

/// Parse a colour spec like `"green"`, `"bold bright-red"`, `"on-blue"`,
/// `"208"`, `"#ff8800"` or `"none"` into an SGR parameter string.
pub fn parse_color(spec: &str) -> Result<Option<String>> {
    let mut codes: Vec<String> = Vec::new();
    for tok in spec.split_whitespace() {
        let t = tok.to_ascii_lowercase();
        match t.as_str() {
            "none" | "default" | "plain" => continue,
            "bold" => codes.push("1".into()),
            "dim" => codes.push("2".into()),
            "italic" => codes.push("3".into()),
            "underline" => codes.push("4".into()),
            "reverse" => codes.push("7".into()),
            _ => {
                let (bg, name) = match t.strip_prefix("on-") {
                    Some(n) => (true, n),
                    None => (false, t.as_str()),
                };
                codes.push(color_code(name, bg).ok_or_else(|| {
                    anyhow::anyhow!(
                        "unknown colour '{tok}' (use names like red, bright-blue, on-blue, 0-255, #rrggbb, bold)"
                    )
                })?);
            }
        }
    }
    Ok(if codes.is_empty() {
        None
    } else {
        Some(codes.join(";"))
    })
}

fn color_code(name: &str, bg: bool) -> Option<String> {
    const NAMES: [&str; 8] = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    let base = if bg { 40 } else { 30 };
    if let Some(i) = NAMES.iter().position(|n| *n == name) {
        return Some((base + i).to_string());
    }
    if let Some(i) = name
        .strip_prefix("bright-")
        .and_then(|n| NAMES.iter().position(|x| *x == n))
    {
        return Some((base + 60 + i).to_string());
    }
    if matches!(name, "grey" | "gray") {
        return Some((base + 60).to_string());
    }
    let ext = if bg { 48 } else { 38 };
    if let Ok(n) = name.parse::<u8>() {
        return Some(format!("{ext};5;{n}"));
    }
    if let Some(hex) = name.strip_prefix('#')
        && hex.len() == 6
        && let Ok(v) = u32::from_str_radix(hex, 16)
    {
        return Some(format!(
            "{ext};2;{};{};{}",
            v >> 16,
            (v >> 8) & 0xff,
            v & 0xff
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours() {
        assert_eq!(parse_color("green").unwrap().as_deref(), Some("32"));
        assert_eq!(
            parse_color("bold bright-red").unwrap().as_deref(),
            Some("1;91")
        );
        assert_eq!(parse_color("on-blue").unwrap().as_deref(), Some("44"));
        assert_eq!(parse_color("208").unwrap().as_deref(), Some("38;5;208"));
        assert_eq!(
            parse_color("#ff8000").unwrap().as_deref(),
            Some("38;2;255;128;0")
        );
        assert_eq!(parse_color("none").unwrap(), None);
        assert!(parse_color("blurple").is_err());
    }

    #[test]
    fn patterns_cycle() {
        let p = Pattern::new("~-");
        assert_eq!((p.at(0, 0), p.at(0, 1), p.at(1, 1)), ('~', '-', '~'));
        assert_eq!(Pattern::new("").at(3, 4), ' ');
    }
}
