//! Projection, rasterisation and text output.

use std::collections::HashMap;
use std::f64::consts::PI;

use crate::data::{BBox, Country};
use crate::style::{Palette, Pattern};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    Equirectangular,
    Mercator,
}

const MERCATOR_MAX_LAT: f64 = 85.0;

impl Projection {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "equirectangular" | "equirect" | "plate-carree" | "platecarree" | "latlon" | "e" => {
                Some(Projection::Equirectangular)
            }
            "mercator" | "merc" | "m" => Some(Projection::Mercator),
            _ => None,
        }
    }

    pub fn max_lat(self) -> f64 {
        match self {
            Projection::Equirectangular => 90.0,
            Projection::Mercator => MERCATOR_MAX_LAT,
        }
    }

    /// Projected y (in degree-like units) for a latitude.
    pub fn y(self, lat: f64) -> f64 {
        match self {
            Projection::Equirectangular => lat,
            Projection::Mercator => {
                let lat = lat.clamp(-MERCATOR_MAX_LAT, MERCATOR_MAX_LAT).to_radians();
                (PI / 4.0 + lat / 2.0).tan().ln().to_degrees()
            }
        }
    }

    pub fn lat(self, y: f64) -> f64 {
        match self {
            Projection::Equirectangular => y,
            Projection::Mercator => (2.0 * y.to_radians().exp().atan() - PI / 2.0).to_degrees(),
        }
    }
}

/// Maps lon/lat onto a `w` x `h` character grid.
#[derive(Clone, Debug)]
pub struct View {
    pub bbox: BBox,
    pub proj: Projection,
    pub w: usize,
    pub h: usize,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

impl View {
    pub fn new(bbox: BBox, proj: Projection, w: usize, h: usize) -> Self {
        View {
            bbox,
            proj,
            w,
            h,
            x0: bbox.west,
            x1: bbox.east,
            y0: proj.y(bbox.south),
            y1: proj.y(bbox.north),
        }
    }

    /// Projected width / height ratio of the bbox.
    pub fn projected_ratio(bbox: &BBox, proj: Projection) -> f64 {
        bbox.width() / (proj.y(bbox.north) - proj.y(bbox.south))
    }

    /// Fractional (col, row) for a lon/lat.
    pub fn project(&self, lon: f64, lat: f64) -> (f64, f64) {
        let c = (lon - self.x0) / (self.x1 - self.x0) * self.w as f64;
        let r = (self.y1 - self.proj.y(lat)) / (self.y1 - self.y0) * self.h as f64;
        (c, r)
    }

    pub fn cell(&self, lon: f64, lat: f64) -> Option<(usize, usize)> {
        if lat < self.bbox.south
            || lat > self.bbox.north
            || lon < self.bbox.west
            || lon > self.bbox.east
        {
            return None;
        }
        let (c, r) = self.project(lon, lat);
        let c = (c.floor() as isize).clamp(0, self.w as isize - 1) as usize;
        let r = (r.floor() as isize).clamp(0, self.h as isize - 1) as usize;
        Some((c, r))
    }

    fn lon_at(&self, col: f64) -> f64 {
        self.x0 + col / self.w as f64 * (self.x1 - self.x0)
    }

    fn lat_at(&self, row: f64) -> f64 {
        self.proj
            .lat(self.y1 - row / self.h as f64 * (self.y1 - self.y0))
    }
}

/// Which country (if any) owns each character cell.
pub struct Grid {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<Option<u32>>,
}

impl Grid {
    pub fn get(&self, c: isize, r: isize) -> Option<Option<u32>> {
        if c < 0 || r < 0 || c >= self.w as isize || r >= self.h as isize {
            None
        } else {
            Some(self.cells[r as usize * self.w + c as usize])
        }
    }
}

/// Scanline-fill every polygon onto a supersampled grid (even-odd rule),
/// then give each character cell to the country covering most of its
/// samples, provided land covers at least `threshold` of the cell.
/// Countries later in `order` win where polygons overlap. Countries in
/// `ensure` get at least one cell even if they are too small to register.
pub fn rasterize(
    countries: &[Country],
    order: &[usize],
    ensure: &[usize],
    view: &View,
    ss: usize,
    threshold: f64,
) -> Grid {
    let ss = ss.max(1);
    let (sw, sh) = (view.w * ss, view.h * ss);
    let mut sub = vec![u32::MAX; sw * sh];
    let kx = sw as f64 / (view.x1 - view.x0);
    let ky = sh as f64 / (view.y1 - view.y0);

    let mut edges: Vec<[f64; 4]> = Vec::new();
    let mut xs: Vec<f64> = Vec::new();
    for &ci in order {
        let country = &countries[ci];
        if !country.bbox.intersects(&view.bbox) {
            continue;
        }
        for poly in &country.polygons {
            if !poly.bbox.intersects(&view.bbox) {
                continue;
            }
            edges.clear();
            let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
            for ring in &poly.rings {
                let pts: Vec<(f64, f64)> = ring
                    .iter()
                    .map(|p| ((p[0] - view.x0) * kx, (view.y1 - view.proj.y(p[1])) * ky))
                    .collect();
                for i in 0..pts.len() {
                    let a = pts[i];
                    let b = pts[(i + 1) % pts.len()];
                    ymin = ymin.min(a.1);
                    ymax = ymax.max(a.1);
                    if a.1 != b.1 {
                        edges.push([a.0, a.1, b.0, b.1]);
                    }
                }
            }
            let r0 = (ymin - 0.5).ceil().max(0.0);
            let r1 = (ymax - 0.5).floor().min(sh as f64 - 1.0);
            if r1 < r0 {
                continue;
            }
            for r in r0 as usize..=r1 as usize {
                let yc = r as f64 + 0.5;
                xs.clear();
                for e in &edges {
                    if (e[1] <= yc) != (e[3] <= yc) {
                        xs.push(e[0] + (yc - e[1]) * (e[2] - e[0]) / (e[3] - e[1]));
                    }
                }
                xs.sort_by(f64::total_cmp);
                let row = &mut sub[r * sw..(r + 1) * sw];
                for pair in xs.as_chunks::<2>().0 {
                    let c0 = (pair[0] - 0.5).ceil().max(0.0);
                    let c1 = (pair[1] - 0.5).ceil().min(sw as f64);
                    if c1 > c0 {
                        row[c0 as usize..c1 as usize].fill(ci as u32);
                    }
                }
            }
        }
    }

    let need = ((threshold.clamp(0.0, 1.0) * (ss * ss) as f64).ceil() as usize).max(1);
    let mut cells = vec![None; view.w * view.h];
    let mut votes: Vec<(u32, usize)> = Vec::new();
    for r in 0..view.h {
        for c in 0..view.w {
            votes.clear();
            let mut land = 0;
            for sr in r * ss..(r + 1) * ss {
                for sc in c * ss..(c + 1) * ss {
                    let id = sub[sr * sw + sc];
                    if id == u32::MAX {
                        continue;
                    }
                    land += 1;
                    match votes.iter_mut().find(|v| v.0 == id) {
                        Some(v) => v.1 += 1,
                        None => votes.push((id, 1)),
                    }
                }
            }
            if land >= need {
                let best = votes
                    .iter()
                    .fold(votes[0], |b, v| if v.1 > b.1 { *v } else { b });
                cells[r * view.w + c] = Some(best.0);
            }
        }
    }
    let mut grid = Grid {
        w: view.w,
        h: view.h,
        cells,
    };
    for &ci in ensure {
        grid.ensure_visible(&countries[ci], ci as u32, view);
    }
    grid
}

impl Grid {
    /// Give a country too small to win any cell a single cell at the
    /// centre of its largest polygon, so it does not vanish from the map.
    fn ensure_visible(&mut self, country: &Country, id: u32, view: &View) {
        if self.cells.contains(&Some(id)) {
            return;
        }
        let Some(main) = country
            .polygons
            .iter()
            .max_by(|a, b| a.area.total_cmp(&b.area))
        else {
            return;
        };
        let lon = (main.bbox.west + main.bbox.east) / 2.0;
        let lat = (main.bbox.south + main.bbox.north) / 2.0;
        if let Some((c, r)) = view.cell(lon, lat) {
            self.cells[r * self.w + c] = Some(id);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Water,
    Land,
    Other,
    Highlight,
    Border,
    Coast,
    Grid,
    Marker,
    Label,
    Frame,
    Title,
}

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub ch: char,
    pub kind: Kind,
}

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub g: Vec<Glyph>,
    /// Cells claimed by markers/labels, so labels do not overwrite each other.
    taken: Vec<bool>,
}

pub struct Paint<'a> {
    pub land: &'a Pattern,
    pub other: &'a Pattern,
    pub water: &'a Pattern,
    pub border: Option<&'a Pattern>,
    pub coast: Option<&'a Pattern>,
    pub grid: &'a Pattern,
    pub graticule: Option<f64>,
    /// Per-country: drawn as primary land?
    pub selected: &'a [bool],
    pub highlight: &'a HashMap<u32, Pattern>,
}

pub fn compose(grid: &Grid, view: &View, p: &Paint) -> Canvas {
    let (w, h) = (grid.w, grid.h);
    let step = p.graticule.filter(|s| *s > 0.0);
    let vlines: Vec<bool> = (0..w)
        .map(|c| {
            step.is_some_and(|s| crosses(view.lon_at(c as f64), view.lon_at(c as f64 + 1.0), s))
        })
        .collect();
    let hlines: Vec<bool> = (0..h)
        .map(|r| {
            step.is_some_and(|s| crosses(view.lat_at(r as f64 + 1.0), view.lat_at(r as f64), s))
        })
        .collect();

    let mut g = Vec::with_capacity(w * h);
    for r in 0..h {
        for c in 0..w {
            let (ci, ri) = (c as isize, r as isize);
            let glyph = match grid.cells[r * w + c] {
                None => {
                    if vlines[c] || hlines[r] {
                        Glyph {
                            ch: p.grid.at(r, c),
                            kind: Kind::Grid,
                        }
                    } else {
                        Glyph {
                            ch: p.water.at(r, c),
                            kind: Kind::Water,
                        }
                    }
                }
                Some(id) => {
                    let n4 = [(ci + 1, ri), (ci - 1, ri), (ci, ri + 1), (ci, ri - 1)];
                    let is_coast =
                        p.coast.is_some() && n4.iter().any(|&(x, y)| grid.get(x, y) == Some(None));
                    let is_border = p.border.is_some()
                        && [(ci + 1, ri), (ci, ri + 1)]
                            .iter()
                            .any(|&(x, y)| matches!(grid.get(x, y), Some(Some(o)) if o != id));
                    if is_coast {
                        Glyph {
                            ch: p.coast.unwrap().at(r, c),
                            kind: Kind::Coast,
                        }
                    } else if is_border {
                        Glyph {
                            ch: p.border.unwrap().at(r, c),
                            kind: Kind::Border,
                        }
                    } else if let Some(hp) = p.highlight.get(&id) {
                        Glyph {
                            ch: hp.at(r, c),
                            kind: Kind::Highlight,
                        }
                    } else if p.selected[id as usize] {
                        Glyph {
                            ch: p.land.at(r, c),
                            kind: Kind::Land,
                        }
                    } else {
                        Glyph {
                            ch: p.other.at(r, c),
                            kind: Kind::Other,
                        }
                    }
                }
            };
            g.push(glyph);
        }
    }
    Canvas {
        w,
        h,
        g,
        taken: vec![false; w * h],
    }
}

/// Does a multiple of `step` fall in [a, b)?
fn crosses(a: f64, b: f64, step: f64) -> bool {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    (a / step).ceil() * step < b
}

pub struct PlacedMarker {
    pub lon: f64,
    pub lat: f64,
    pub symbol: Vec<char>,
    pub label: Option<String>,
}

impl Canvas {
    fn put(&mut self, c: usize, r: usize, ch: char, kind: Kind) {
        let i = r * self.w + c;
        self.g[i] = Glyph { ch, kind };
        self.taken[i] = true;
    }

    /// Draw markers, then their labels where they fit. Returns the number of
    /// markers that fell outside the view.
    pub fn place_markers(&mut self, view: &View, markers: &[PlacedMarker], labels: bool) -> usize {
        let mut spots = Vec::new();
        let mut outside = 0;
        for m in markers {
            match view.cell(m.lon, m.lat) {
                Some((c, r)) => {
                    let len = m.symbol.len().max(1);
                    let start = c.saturating_sub(len / 2).min(self.w.saturating_sub(len));
                    for (k, &ch) in m.symbol.iter().enumerate() {
                        if start + k < self.w {
                            self.put(start + k, r, ch, Kind::Marker);
                        }
                    }
                    spots.push((start, start + len, r, m));
                }
                None => outside += 1,
            }
        }
        if labels {
            for (s, e, r, m) in spots {
                let Some(label) = m.label.as_deref().filter(|l| !l.is_empty()) else {
                    continue;
                };
                // Label plus one blank cell separating it from the marker:
                // try right of the marker, then left of it.
                let n = label.chars().count() + 1;
                let right = (e + n <= self.w).then_some(e);
                let left = (s >= n).then(|| s - n);
                let free = |st: usize| (st..st + n).all(|c| !self.taken[r * self.w + c]);
                let text: Option<(usize, String)> = if let Some(st) = right.filter(|&st| free(st)) {
                    Some((st, format!(" {label}")))
                } else {
                    left.filter(|&st| free(st))
                        .map(|st| (st, format!("{label} ")))
                };
                if let Some((st, text)) = text {
                    for (k, ch) in text.chars().enumerate() {
                        self.put(st + k, r, ch, Kind::Label);
                    }
                }
            }
        }
        outside
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Frame {
    None,
    Ascii,
    Line,
    Rounded,
    Double,
    Heavy,
}

impl Frame {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "none" | "off" | "false" => Frame::None,
            "ascii" => Frame::Ascii,
            "line" | "unicode" | "single" | "true" | "on" => Frame::Line,
            "rounded" | "round" => Frame::Rounded,
            "double" => Frame::Double,
            "heavy" | "bold" => Frame::Heavy,
            _ => return None,
        })
    }

    /// [top-left, top-right, bottom-left, bottom-right, horizontal, vertical]
    fn chars(self) -> [char; 6] {
        match self {
            Frame::None => [' '; 6],
            Frame::Ascii => ['+', '+', '+', '+', '-', '|'],
            Frame::Line => ['┌', '┐', '└', '┘', '─', '│'],
            Frame::Rounded => ['╭', '╮', '╰', '╯', '─', '│'],
            Frame::Double => ['╔', '╗', '╚', '╝', '═', '║'],
            Frame::Heavy => ['┏', '┓', '┗', '┛', '━', '┃'],
        }
    }
}

/// Render the canvas (plus optional frame and title) to a string.
pub fn to_text(
    canvas: &Canvas,
    frame: Frame,
    title: Option<&str>,
    palette: Option<&Palette>,
) -> String {
    let mut rows: Vec<Vec<Glyph>> = Vec::new();
    let w = canvas.w;
    let title: Option<Vec<char>> = title.filter(|t| !t.is_empty()).map(|t| t.chars().collect());

    if frame == Frame::None {
        if let Some(t) = &title {
            let pad = w.saturating_sub(t.len()) / 2;
            let mut row = vec![
                Glyph {
                    ch: ' ',
                    kind: Kind::Title
                };
                pad
            ];
            row.extend(t.iter().map(|&ch| Glyph {
                ch,
                kind: Kind::Title,
            }));
            rows.push(row);
        }
        for r in 0..canvas.h {
            rows.push(canvas.g[r * w..(r + 1) * w].to_vec());
        }
    } else {
        let [tl, tr, bl, br, hz, vt] = frame.chars();
        let fg = |ch| Glyph {
            ch,
            kind: Kind::Frame,
        };
        let mut top = vec![fg(tl)];
        let mut inner: Vec<Glyph> = vec![fg(hz); w];
        if let Some(t) = &title {
            // "─ Title ─────"
            let t: Vec<char> = t.iter().copied().take(w.saturating_sub(4)).collect();
            if !t.is_empty() {
                inner[1] = Glyph {
                    ch: ' ',
                    kind: Kind::Frame,
                };
                for (k, &ch) in t.iter().enumerate() {
                    inner[2 + k] = Glyph {
                        ch,
                        kind: Kind::Title,
                    };
                }
                inner[2 + t.len()] = Glyph {
                    ch: ' ',
                    kind: Kind::Frame,
                };
            }
        }
        top.extend(inner);
        top.push(fg(tr));
        rows.push(top);
        for r in 0..canvas.h {
            let mut row = vec![fg(vt)];
            row.extend_from_slice(&canvas.g[r * w..(r + 1) * w]);
            row.push(fg(vt));
            rows.push(row);
        }
        let mut bottom = vec![fg(bl)];
        bottom.extend(std::iter::repeat_n(fg(hz), w));
        bottom.push(fg(br));
        rows.push(bottom);
    }

    let mut out = String::new();
    for mut row in rows {
        if frame == Frame::None {
            // Trailing blanks carry no information without a frame.
            while row.last().is_some_and(|g| g.ch == ' ') {
                row.pop();
            }
        }
        match palette {
            None => out.extend(row.iter().map(|g| g.ch)),
            Some(pal) => {
                let mut cur: Option<Kind> = None;
                for g in &row {
                    if cur != Some(g.kind) {
                        if cur.is_some_and(|k| pal.get(k).is_some()) {
                            out.push_str("\x1b[0m");
                        }
                        if let Some(sgr) = pal.get(g.kind) {
                            out.push_str(&format!("\x1b[{sgr}m"));
                        }
                        cur = Some(g.kind);
                    }
                    out.push(g.ch);
                }
                if cur.is_some_and(|k| pal.get(k).is_some()) {
                    out.push_str("\x1b[0m");
                }
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Polygon;

    fn square(name: &str, w: f64, s: f64, e: f64, n: f64) -> Country {
        let poly = Polygon::new(vec![vec![[w, s], [e, s], [e, n], [w, n], [w, s]]]);
        Country {
            name: name.into(),
            long_name: name.into(),
            code: name.into(),
            iso2: String::new(),
            continent: String::new(),
            subregion: String::new(),
            bbox: poly.bbox,
            polygons: vec![poly],
        }
    }

    #[test]
    fn rasterizes_a_square_into_the_right_cells() {
        // 10x10 degree view on a 10x10 grid: one degree per cell.
        let view = View::new(
            BBox::new(0.0, 0.0, 10.0, 10.0),
            Projection::Equirectangular,
            10,
            10,
        );
        let countries = vec![square("A", 2.0, 2.0, 5.0, 6.0)];
        let g = rasterize(&countries, &[0], &[], &view, 3, 0.5);
        let land: usize = g.cells.iter().filter(|c| c.is_some()).count();
        assert_eq!(land, 3 * 4);
        // Row 4 from the top spans latitudes 5..6, columns 2..5.
        assert_eq!(g.cells[4 * 10 + 2], Some(0));
        assert_eq!(g.cells[4 * 10 + 5], None);
    }

    #[test]
    fn holes_are_left_empty() {
        let poly = Polygon::new(vec![
            vec![[0.0, 0.0], [9.0, 0.0], [9.0, 9.0], [0.0, 9.0]],
            vec![[3.0, 3.0], [6.0, 3.0], [6.0, 6.0], [3.0, 6.0]],
        ]);
        let mut c = square("A", 0.0, 0.0, 1.0, 1.0);
        c.bbox = poly.bbox;
        c.polygons = vec![poly];
        let view = View::new(
            BBox::new(0.0, 0.0, 9.0, 9.0),
            Projection::Equirectangular,
            9,
            9,
        );
        let g = rasterize(&[c], &[0], &[], &view, 2, 0.5);
        assert_eq!(g.cells[4 * 9 + 4], None);
        assert_eq!(g.cells[0], Some(0));
        assert_eq!(g.cells.iter().filter(|c| c.is_some()).count(), 81 - 9);
    }

    #[test]
    fn later_countries_win_overlaps_and_tiny_ones_survive() {
        let view = View::new(
            BBox::new(0.0, 0.0, 10.0, 10.0),
            Projection::Equirectangular,
            10,
            10,
        );
        let countries = vec![
            square("A", 0.0, 0.0, 10.0, 10.0),
            square("B", 5.0, 5.0, 5.01, 5.01),
        ];
        let g = rasterize(&countries, &[0, 1], &[], &view, 3, 0.5);
        assert!(!g.cells.contains(&Some(1)));
        let g = rasterize(&countries, &[0, 1], &[1], &view, 3, 0.5);
        assert_eq!(g.cells.iter().filter(|c| **c == Some(1)).count(), 1);
    }

    #[test]
    fn mercator_round_trips() {
        let p = Projection::Mercator;
        for lat in [-80.0, -33.3, 0.0, 51.5, 84.0] {
            assert!((p.lat(p.y(lat)) - lat).abs() < 1e-9);
        }
        assert!(p.y(60.0) > 60.0);
    }

    #[test]
    fn labels_avoid_the_right_edge() {
        let view = View::new(
            BBox::new(0.0, 0.0, 20.0, 10.0),
            Projection::Equirectangular,
            20,
            1,
        );
        let grid = Grid {
            w: 20,
            h: 1,
            cells: vec![None; 20],
        };
        let pat = crate::style::Pattern::new(" ");
        let hl = HashMap::new();
        let mut canvas = compose(
            &grid,
            &view,
            &Paint {
                land: &pat,
                other: &pat,
                water: &pat,
                border: None,
                coast: None,
                grid: &pat,
                graticule: None,
                selected: &[],
                highlight: &hl,
            },
        );
        let m = PlacedMarker {
            lon: 18.5,
            lat: 5.0,
            symbol: vec!['*'],
            label: Some("Far".into()),
        };
        assert_eq!(canvas.place_markers(&view, &[m], true), 0);
        let text = to_text(&canvas, Frame::None, None, None);
        assert_eq!(text, "              Far *\n");
    }
}
