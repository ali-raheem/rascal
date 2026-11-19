//! Settings shared by the config file and the command line. Both are parsed
//! into a [`Config`]; command-line values are layered on top.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            OneOrMany::None => vec![],
            OneOrMany::One(s) => s
                .split(',')
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect(),
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Chars {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub land: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coast: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub land: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coast: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A marker in the config file. Give either `place` or `lat` + `lon`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MarkerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(skip_serializing_if = "is_none")]
    pub region: OneOrMany,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub isolate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<[f64; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pad: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graticule: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capitals: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersample: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geojson: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markers_file: Option<PathBuf>,
    /// Command-line style marker specs, e.g. `"51.5,-0.12,*,London"`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
    #[serde(skip_serializing_if = "Chars::is_empty")]
    pub chars: Chars,
    #[serde(skip_serializing_if = "Colors::is_empty")]
    pub colors: Colors,
    /// Region selector -> characters.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub highlight: BTreeMap<String, String>,
    #[serde(rename = "marker", skip_serializing_if = "Vec::is_empty")]
    pub marker_table: Vec<MarkerConfig>,
}

impl Chars {
    fn is_empty(&self) -> bool {
        toml::Value::try_from(self).is_ok_and(|v| v.as_table().is_some_and(|t| t.is_empty()))
    }
}

impl Colors {
    fn is_empty(&self) -> bool {
        toml::Value::try_from(self).is_ok_and(|v| v.as_table().is_some_and(|t| t.is_empty()))
    }
}

fn is_none(v: &OneOrMany) -> bool {
    matches!(v, OneOrMany::None)
}

macro_rules! overlay {
    ($base:expr, $top:expr; $($f:ident),* $(,)?) => {
        $( if $top.$f.is_some() { $base.$f = $top.$f; } )*
    };
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config =
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
        // Relative paths in a config file are relative to the file itself.
        let dir = path.parent().unwrap_or(Path::new("."));
        for p in [&mut cfg.geojson, &mut cfg.markers_file]
            .into_iter()
            .flatten()
        {
            if p.is_relative() {
                *p = dir.join(&*p);
            }
        }
        Ok(cfg)
    }

    /// `$XDG_CONFIG_HOME/rascal/config.toml` or `~/.config/rascal/config.toml`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("rascal").join("config.toml"))
    }

    /// Layer `top` over `self`: set values win, lists are appended.
    pub fn merge(mut self, top: Config) -> Config {
        if !is_none(&top.region) {
            self.region = top.region;
        }
        // Size is set as a unit: `--width` alone should not be paired with a
        // height from the config file (which would stretch the map).
        if top.width.is_some() || top.height.is_some() {
            self.width = None;
            self.height = None;
        }
        overlay!(self, top; width, height, aspect, projection, style, isolate, bbox, fit, pad,
                 frame, title, color, graticule, legend, labels, capitals, supersample, threshold,
                 geojson, markers_file);
        overlay!(self.chars, top.chars; land, other, water, border, coast, highlight, marker, grid);
        overlay!(self.colors, top.colors; land, other, water, border, coast, highlight, grid,
                 marker, label, frame, title);
        self.markers.extend(top.markers);
        self.marker_table.extend(top.marker_table);
        self.highlight.extend(top.highlight);
        self
    }
}
