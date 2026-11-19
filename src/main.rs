mod cli;
mod config;
mod data;
mod markers;
mod render;
mod select;
mod style;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::IsTerminal;

use anyhow::{Context, Result, bail};
use clap::Parser;

use cli::{Cli, ListKind};
use config::Config;
use data::{BBox, Country, Place};
use render::{Frame, Paint, PlacedMarker, Projection, View};
use select::{Selection, WORLD_VIEW};
use style::{Palette, Pattern};

fn main() {
    if let Err(e) = run() {
        eprintln!("rascal: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let mut cfg = Config::default();
    if let Some(path) = &cli.config {
        cfg = Config::load(path)?;
    } else if !cli.no_config
        && let Some(path) = Config::default_path().filter(|p| p.exists())
    {
        cfg = Config::load(&path)?;
    }
    let cfg = cfg.merge(cli.to_config()?);

    if cli.print_config {
        print!(
            "{}",
            toml::to_string_pretty(&cfg).context("serialising config")?
        );
        return Ok(());
    }

    let countries = match &cfg.geojson {
        Some(p) => data::load_geojson(p)?,
        None => data::builtin_countries()?,
    };
    let places = data::builtin_places()?;

    if let Some(kind) = cli.list {
        return list(kind, &countries, &places, &cfg.region.to_vec());
    }

    draw(&cfg, &countries, &places)
}

fn draw(cfg: &Config, countries: &[Country], places: &[Place]) -> Result<()> {
    let proj_name = cfg.projection.as_deref().unwrap_or("equirectangular");
    let proj = Projection::parse(proj_name).with_context(|| {
        format!("unknown projection '{proj_name}' (use equirectangular or mercator)")
    })?;
    let style = style::find_style(cfg.style.as_deref().unwrap_or("classic"))?;
    let chars = style::resolve(style, &cfg.chars);
    let frame_name = cfg.frame.as_deref().unwrap_or("none");
    let frame = Frame::parse(frame_name).with_context(|| {
        format!("unknown frame '{frame_name}' (use none, ascii, line, rounded, double, heavy)")
    })?;

    // Regions
    let selectors = cfg.region.to_vec();
    let mut selected = vec![selectors.is_empty(); countries.len()];
    let mut region_view: Option<(BBox, bool)> = None;
    for sel in &selectors {
        let s: Selection = select::resolve(countries, sel)?;
        for &i in &s.indices {
            selected[i] = true;
        }
        region_view = Some(match region_view {
            None => (s.view, s.preset),
            Some((v, p)) => (v.union(&s.view), p && s.preset),
        });
    }

    // Highlights
    let mut highlight: HashMap<u32, Pattern> = HashMap::new();
    let mut highlight_legend: Vec<(char, String)> = Vec::new();
    for (sel, ch) in &cfg.highlight {
        let s = select::resolve(countries, sel)?;
        let pat = if ch.is_empty() {
            chars.highlight.clone()
        } else {
            Pattern::new(ch)
        };
        highlight_legend.push((pat.first(), sel.clone()));
        for &i in &s.indices {
            highlight.insert(i as u32, pat.clone());
        }
    }

    // Markers: capitals first so explicit markers draw on top.
    let mut marks: Vec<markers::Marker> = Vec::new();
    if cfg.capitals == Some(true) {
        let codes: HashSet<&str> = countries
            .iter()
            .zip(&selected)
            .filter(|(_, s)| **s)
            .map(|(c, _)| c.code.as_str())
            .collect();
        marks.extend(
            places
                .iter()
                .filter(|p| p.capital && codes.contains(p.country.as_str()))
                .map(|p| markers::Marker {
                    lat: p.lat,
                    lon: p.lon,
                    symbol: None,
                    label: Some(p.name.clone()),
                }),
        );
    }
    let n_capitals = marks.len();
    for spec in &cfg.markers {
        marks.push(markers::parse_spec(spec, places)?);
    }
    if let Some(f) = &cfg.markers_file {
        marks.extend(markers::load_file(f, places)?);
    }
    for m in &cfg.marker_table {
        marks.push(markers::from_config(m, places)?);
    }

    // Viewport
    let fit = cfg.fit == Some(true) && marks.len() > n_capitals;
    let (bbox, preset) = if let Some([w, s, e, n]) = cfg.bbox {
        if w >= e || s >= n {
            bail!("--bbox must be WEST,SOUTH,EAST,NORTH with west < east and south < north");
        }
        (BBox::new(w, s, e, n), true)
    } else if fit {
        let mut b = BBox::empty();
        for m in &marks[n_capitals..] {
            b.add_point(m.lon, m.lat);
        }
        match region_view {
            Some((v, _)) => (b.union(&v), false),
            None => (b, false),
        }
    } else {
        region_view.unwrap_or((WORLD_VIEW, true))
    };
    let mut bbox = bbox;
    if !preset || cfg.pad.is_some() {
        let auto = (0.08 * bbox.width().max(bbox.height())).max(0.5);
        bbox = bbox.padded(cfg.pad.unwrap_or(auto));
    }
    let bbox = bbox.with_min_span(1.0).clamped(proj.max_lat());
    if bbox.width() <= 0.0 || bbox.height() <= 0.0 {
        bail!("the view is empty");
    }

    // Size
    let aspect = cfg.aspect.unwrap_or(0.5);
    if aspect <= 0.0 {
        bail!("--aspect must be positive");
    }
    let ratio = View::projected_ratio(&bbox, proj);
    let extra = if frame == Frame::None { 0 } else { 2 };
    let (w, h) = match (cfg.width, cfg.height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, (w as f64 / ratio * aspect).round() as usize),
        (None, Some(h)) => ((h as f64 / aspect * ratio).round() as usize, h),
        (None, None) => {
            let (tw, th) = terminal_size::terminal_size()
                .map(|(w, h)| (w.0 as usize, h.0 as usize))
                .unwrap_or((100, 50));
            let max_w = tw.saturating_sub(extra).max(10);
            let max_h = th.saturating_sub(extra + 2).max(5);
            let w = max_w;
            let h = (w as f64 / ratio * aspect).round() as usize;
            if h > max_h {
                ((max_h as f64 / aspect * ratio).round() as usize, max_h)
            } else {
                (w, h)
            }
        }
    };
    let (w, h) = (w.max(1), h.max(1));
    if w * h > 4_000_000 {
        bail!("map of {w}x{h} characters is too large");
    }
    let view = View::new(bbox, proj, w, h);

    // Draw order: context land, then the selection, then highlights.
    let isolate = cfg.isolate == Some(true);
    let hl = |i: usize| highlight.contains_key(&(i as u32));
    let mut order: Vec<usize> = Vec::new();
    if !isolate {
        order.extend((0..countries.len()).filter(|&i| !selected[i] && !hl(i)));
    }
    order.extend((0..countries.len()).filter(|&i| selected[i] && !hl(i)));
    order.extend((0..countries.len()).filter(|&i| hl(i)));

    // Small selected or highlighted countries (Monaco, Malta, ...) should
    // still appear; for a whole-world map that would just add noise.
    let ensure: Vec<usize> = (0..countries.len())
        .filter(|&i| {
            hl(i) || (selected[i] && !selectors.is_empty() && region_view.is_some_and(|(_, p)| !p))
        })
        .collect();
    let grid = render::rasterize(
        countries,
        &order,
        &ensure,
        &view,
        cfg.supersample.unwrap_or(3).clamp(1, 16),
        cfg.threshold.unwrap_or(0.5),
    );
    let mut canvas = render::compose(
        &grid,
        &view,
        &Paint {
            land: &chars.land,
            other: &chars.other,
            water: &chars.water,
            border: chars.border.as_ref(),
            coast: chars.coast.as_ref(),
            grid: &chars.grid,
            graticule: cfg.graticule,
            selected: &selected,
            highlight: &highlight,
        },
    );

    let placed: Vec<PlacedMarker> = marks
        .iter()
        .map(|m| PlacedMarker {
            lon: m.lon,
            lat: m.lat,
            symbol: m
                .symbol
                .as_deref()
                .unwrap_or(&chars.marker)
                .chars()
                .collect(),
            label: m.label.clone(),
        })
        .collect();
    let outside = canvas.place_markers(&view, &placed, cfg.labels != Some(false));
    if outside > 0 {
        eprintln!("rascal: {outside} marker(s) outside the map view (try --fit)");
    }

    let palette = match cfg.color.as_deref().unwrap_or("never") {
        "always" | "yes" | "true" | "on" => true,
        "never" | "no" | "false" | "off" => false,
        "auto" => std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        other => bail!("unknown --color value '{other}' (use auto, always or never)"),
    }
    .then(|| Palette::build(&cfg.colors))
    .transpose()?;

    print!(
        "{}",
        render::to_text(&canvas, frame, cfg.title.as_deref(), palette.as_ref())
    );

    if cfg.legend == Some(true) {
        let mut lines = Vec::new();
        for (ch, sel) in &highlight_legend {
            lines.push(format!("  {ch}  {sel}"));
        }
        for m in &marks {
            let sym = m.symbol.as_deref().unwrap_or(&chars.marker);
            let label = m.label.as_deref().unwrap_or("");
            lines.push(format!(
                "  {sym}  {label}{}({:.3}, {:.3})",
                if label.is_empty() { "" } else { " " },
                m.lat,
                m.lon
            ));
        }
        if !lines.is_empty() {
            println!();
            for l in lines {
                println!("{l}");
            }
        }
    }
    Ok(())
}

fn list(kind: ListKind, countries: &[Country], places: &[Place], regions: &[String]) -> Result<()> {
    let mut keep = vec![regions.is_empty(); countries.len()];
    for r in regions {
        for i in select::resolve(countries, r)?.indices {
            keep[i] = true;
        }
    }
    let kept = || {
        countries
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(c, _)| c)
    };
    match kind {
        ListKind::Countries => {
            for c in kept() {
                let iso2 = if c.iso2.is_empty() { "--" } else { &c.iso2 };
                println!(
                    "{:<4} {:<3} {:<34} {} / {}",
                    c.code, iso2, c.name, c.continent, c.subregion
                );
            }
        }
        ListKind::Continents => {
            let mut m: BTreeMap<&str, usize> = BTreeMap::new();
            for c in kept() {
                *m.entry(c.continent.as_str()).or_default() += 1;
            }
            for (k, n) in m {
                println!("{:<26} {n:>3} countries   (select with: {})", k, slug(k));
            }
        }
        ListKind::Subregions => {
            let mut m: BTreeMap<(&str, &str), usize> = BTreeMap::new();
            for c in kept() {
                *m.entry((c.continent.as_str(), c.subregion.as_str()))
                    .or_default() += 1;
            }
            for ((cont, sub), n) in m {
                println!("{:<16} {:<28} {n:>3}   ({})", cont, sub, slug(sub));
            }
        }
        ListKind::Places => {
            let codes: HashSet<&str> = kept().map(|c| c.code.as_str()).collect();
            for p in places.iter().filter(|p| codes.contains(p.country.as_str())) {
                println!(
                    "{:<28} {:<4} {:>8.3} {:>9.3} {:>10}{}",
                    p.name,
                    p.country,
                    p.lat,
                    p.lon,
                    p.pop,
                    if p.capital { "  capital" } else { "" }
                );
            }
        }
        ListKind::Styles => {
            for s in style::STYLES {
                println!(
                    "{:<8} land {}  other {}  water {}  highlight {}  marker {}   {}",
                    s.name,
                    show(s.land),
                    show(s.other),
                    show(s.water),
                    show(s.highlight),
                    show(s.marker),
                    s.about
                );
            }
        }
    }
    Ok(())
}

fn slug(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

fn show(s: &str) -> String {
    if s.trim().is_empty() {
        "' '".into()
    } else {
        format!("{s:<3}")
    }
}
