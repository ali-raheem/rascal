use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};

use crate::config::{Chars, Colors, Config, OneOrMany};

const AFTER_HELP: &str = "\
REGIONS:
  world (default), a continent (europe, africa, asia, north-america,
  south-america, oceania, antarctica, americas, pacific), a subregion
  (western-europe, south-eastern-asia, ...), or a country by name or
  code (france, FRA, FR). Combine with commas: `rascal france,spain`.

MARKERS (-m):
  LAT,LON[,SYMBOL[,LABEL]]     -m 51.5,-0.12,*,London   -m 33.9S,18.4E
  PLACE[,SYMBOL[,LABEL]]       -m Tokyo   -m 'Paris,@'   -m Cordoba/ESP
  A markers file (-M) holds one spec per line; # starts a comment.

EXAMPLES:
  rascal                                    world map sized to the terminal
  rascal europe -s blocks --border '|'      Europe with country borders
  rascal --land '#' --water '-'             choose your own characters
  rascal france -i --coast '*'              France alone, outlined
  rascal -m London -m Tokyo,% -m 40.7,-74,@,NYC --legend
  rascal --highlight brazil=@ --highlight 'argentina=%' south-america
  rascal --bbox=-10,35,30,60 -w 80 -p mercator
  rascal --center 150                       world map centred on the Pacific
  rascal pacific -m Tokyo -m Honolulu -m 'Los Angeles'
  rascal --bbox 150,-50,-150,0              a view across the 180° meridian
  rascal --fit -m Paris -m Berlin -m Rome
  rascal --print-config > ~/.config/rascal/config.toml";

#[derive(Parser, Debug)]
#[command(
    name = "rascal",
    version,
    about = "Draw ASCII art maps of the world, continents and countries",
    after_help = AFTER_HELP
)]
pub struct Cli {
    /// Regions to draw (see REGIONS below)
    #[arg(value_name = "REGION", value_delimiter = ',')]
    pub region: Vec<String>,

    /// Map width in characters [default: fit terminal]
    #[arg(short, long)]
    pub width: Option<usize>,

    /// Map height in lines [default: from width and aspect]
    #[arg(short = 'H', long)]
    pub height: Option<usize>,

    /// Character cell width/height ratio [default: 0.5]
    #[arg(long)]
    pub aspect: Option<f64>,

    /// Projection: equirectangular (default) or mercator
    #[arg(short, long)]
    pub projection: Option<String>,

    /// Character style: classic, sea, dots, blocks, shade, braille (see --list styles)
    #[arg(short, long)]
    pub style: Option<String>,

    /// Characters for selected land (several chars form a pattern)
    #[arg(long, value_name = "CHARS")]
    pub land: Option<String>,

    /// Characters for water
    #[arg(long, value_name = "CHARS")]
    pub water: Option<String>,

    /// Characters for land outside the selected region
    #[arg(long, value_name = "CHARS")]
    pub other: Option<String>,

    /// Draw borders between countries with these characters
    #[arg(long, value_name = "CHARS")]
    pub border: Option<String>,

    /// Draw coastlines with these characters
    #[arg(long, value_name = "CHARS")]
    pub coast: Option<String>,

    /// Default marker symbol
    #[arg(long, value_name = "CHARS")]
    pub marker_char: Option<String>,

    /// Characters for graticule lines
    #[arg(long, value_name = "CHARS")]
    pub grid_char: Option<String>,

    /// Add a marker (repeatable, see MARKERS below)
    #[arg(
        short = 'm',
        long = "marker",
        value_name = "SPEC",
        allow_hyphen_values = true
    )]
    pub markers: Vec<String>,

    /// Read markers from a file
    #[arg(short = 'M', long, value_name = "FILE")]
    pub markers_file: Option<PathBuf>,

    /// Highlight a region: SELECTOR[=CHARS] (repeatable)
    #[arg(long = "highlight", value_name = "SEL[=CHARS]")]
    pub highlight: Vec<String>,

    /// Hide land outside the selected regions
    #[arg(short, long)]
    pub isolate: bool,

    /// Explicit view: WEST,SOUTH,EAST,NORTH in degrees
    #[arg(long, value_name = "W,S,E,N", allow_hyphen_values = true)]
    pub bbox: Option<String>,

    /// Zoom to include all markers
    #[arg(long)]
    pub fit: bool,

    /// Padding around the selection in degrees [default: auto]
    #[arg(long, value_name = "DEG")]
    pub pad: Option<f64>,

    /// Draw a frame: -f, or --frame=ascii|line|rounded|double|heavy|none
    #[arg(short, long, num_args = 0..=1, require_equals = true, default_missing_value = "line", value_name = "STYLE")]
    pub frame: Option<String>,

    /// Title shown above the map (or in the frame)
    #[arg(short, long)]
    pub title: Option<String>,

    /// Print a legend of highlights and markers under the map
    #[arg(short, long)]
    pub legend: bool,

    /// Do not print marker labels on the map
    #[arg(long)]
    pub no_labels: bool,

    /// Mark the capital cities of the selected countries
    #[arg(long)]
    pub capitals: bool,

    /// Draw lat/lon lines every DEG degrees
    #[arg(short, long, value_name = "DEG")]
    pub graticule: Option<f64>,

    /// Centre the view on this longitude (e.g. 150 for a Pacific-centred world)
    #[arg(long, value_name = "LON", allow_hyphen_values = true)]
    pub center: Option<f64>,

    /// ANSI colour: --color, or --color=auto|always|never [default: never]
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "always", value_name = "WHEN")]
    pub color: Option<String>,

    /// Samples per cell side when rasterising [default: 3]
    #[arg(long, value_name = "N")]
    pub supersample: Option<usize>,

    /// Fraction of a cell that must be land to draw it [default: 0.5]
    #[arg(long, value_name = "F")]
    pub threshold: Option<f64>,

    /// Use polygons from a GeoJSON file instead of the built-in world
    #[arg(long, value_name = "FILE")]
    pub geojson: Option<PathBuf>,

    /// Config file [default: ~/.config/rascal/config.toml]
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Ignore the default config file
    #[arg(long)]
    pub no_config: bool,

    /// Print the effective settings as TOML and exit
    #[arg(long)]
    pub print_config: bool,

    /// List available data and exit (filtered by REGION if given)
    #[arg(long, value_name = "WHAT")]
    pub list: Option<ListKind>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ListKind {
    Countries,
    Continents,
    Subregions,
    Places,
    Styles,
}

impl Cli {
    pub fn to_config(&self) -> Result<Config> {
        let bbox = match &self.bbox {
            None => None,
            Some(s) => {
                let v: Vec<f64> = s
                    .split(',')
                    .map(|x| x.trim().parse::<f64>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| anyhow::anyhow!("--bbox expects four numbers W,S,E,N"))?;
                let [w, s, e, n] = v[..] else {
                    bail!("--bbox expects four numbers W,S,E,N")
                };
                Some([w, s, e, n])
            }
        };
        let mut highlight = std::collections::BTreeMap::new();
        for h in &self.highlight {
            let (sel, chars) = h.split_once('=').unwrap_or((h.as_str(), ""));
            highlight.insert(sel.to_string(), chars.to_string());
        }
        let flag = |b: bool| b.then_some(true);
        Ok(Config {
            region: if self.region.is_empty() {
                OneOrMany::None
            } else {
                OneOrMany::Many(self.region.clone())
            },
            width: self.width,
            height: self.height,
            aspect: self.aspect,
            projection: self.projection.clone(),
            style: self.style.clone(),
            isolate: flag(self.isolate),
            bbox,
            fit: flag(self.fit),
            pad: self.pad,
            frame: self.frame.clone(),
            title: self.title.clone(),
            color: self.color.clone(),
            graticule: self.graticule,
            center: self.center,
            legend: flag(self.legend),
            labels: self.no_labels.then_some(false),
            capitals: flag(self.capitals),
            supersample: self.supersample,
            threshold: self.threshold,
            geojson: self.geojson.clone(),
            markers_file: self.markers_file.clone(),
            markers: self.markers.clone(),
            chars: Chars {
                land: self.land.clone(),
                other: self.other.clone(),
                water: self.water.clone(),
                border: self.border.clone(),
                coast: self.coast.clone(),
                highlight: None,
                marker: self.marker_char.clone(),
                grid: self.grid_char.clone(),
            },
            colors: Colors::default(),
            highlight,
            marker_table: Vec::new(),
        })
    }
}
