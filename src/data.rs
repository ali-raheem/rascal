//! Geographic data: country polygons and populated places.
//!
//! The built-in data is derived from Natural Earth (public domain) 1:50m
//! admin-0 countries and populated places, regenerated with
//! `scripts/build_data.py`. Users can also load their own GeoJSON.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

static COUNTRIES_JSON: &str = include_str!("../data/countries.json");
static PLACES_JSON: &str = include_str!("../data/places.json");

/// Axis-aligned lon/lat bounding box, in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl BBox {
    pub fn new(west: f64, south: f64, east: f64, north: f64) -> Self {
        BBox {
            west,
            south,
            east,
            north,
        }
    }

    pub fn empty() -> Self {
        BBox {
            west: f64::INFINITY,
            south: f64::INFINITY,
            east: f64::NEG_INFINITY,
            north: f64::NEG_INFINITY,
        }
    }

    pub fn add_point(&mut self, lon: f64, lat: f64) {
        self.west = self.west.min(lon);
        self.east = self.east.max(lon);
        self.south = self.south.min(lat);
        self.north = self.north.max(lat);
    }

    pub fn union(&self, o: &BBox) -> BBox {
        BBox {
            west: self.west.min(o.west),
            south: self.south.min(o.south),
            east: self.east.max(o.east),
            north: self.north.max(o.north),
        }
    }

    pub fn width(&self) -> f64 {
        self.east - self.west
    }

    pub fn height(&self) -> f64 {
        self.north - self.south
    }

    pub fn intersects(&self, o: &BBox) -> bool {
        self.west <= o.east && o.west <= self.east && self.south <= o.north && o.south <= self.north
    }

    /// Chebyshev distance between two boxes (0 if they overlap).
    pub fn gap(&self, o: &BBox) -> f64 {
        let dx = (o.west - self.east).max(self.west - o.east).max(0.0);
        let dy = (o.south - self.north).max(self.south - o.north).max(0.0);
        dx.max(dy)
    }

    pub fn padded(&self, pad: f64) -> BBox {
        BBox {
            west: self.west - pad,
            south: self.south - pad,
            east: self.east + pad,
            north: self.north + pad,
        }
    }

    /// Grow the box around its centre so each side spans at least `min` degrees.
    pub fn with_min_span(&self, min: f64) -> BBox {
        let mut b = *self;
        if b.width() < min {
            let c = (b.west + b.east) / 2.0;
            b.west = c - min / 2.0;
            b.east = c + min / 2.0;
        }
        if b.height() < min {
            let c = (b.south + b.north) / 2.0;
            b.south = c - min / 2.0;
            b.north = c + min / 2.0;
        }
        b
    }

    pub fn shifted(&self, dlon: f64) -> BBox {
        BBox {
            west: self.west + dlon,
            east: self.east + dlon,
            ..*self
        }
    }

    /// The copy of `o` (shifted by 0 or ±360°) closest to `self`, and its gap.
    pub fn nearest_copy(&self, o: &BBox) -> (f64, BBox) {
        [0.0, 360.0, -360.0]
            .iter()
            .map(|d| o.shifted(*d))
            .map(|b| (self.gap(&b), b))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap()
    }

    /// Union with whichever copy of `o` (0 or ±360°) gives the narrowest
    /// result, so boxes either side of the antimeridian join up across it.
    pub fn union_wrapped(&self, o: &BBox) -> BBox {
        [0.0, 360.0, -360.0]
            .iter()
            .map(|d| self.union(&o.shifted(*d)))
            .min_by(|a, b| a.width().total_cmp(&b.width()))
            .unwrap()
    }

    /// Clamp latitudes, limit the longitude span to 360° and bring the
    /// west edge into [-180, 180). Views may extend past ±180°; the
    /// renderer draws the world repeatedly to fill them.
    pub fn clamped(&self, max_lat: f64) -> BBox {
        let mut b = BBox {
            south: self.south.max(-max_lat),
            north: self.north.min(max_lat),
            ..*self
        };
        if b.width() > 360.0 {
            let c = (b.west + b.east) / 2.0;
            b.west = c - 180.0;
            b.east = c + 180.0;
        }
        let shift = ((b.west + 180.0) / 360.0).floor() * 360.0;
        b.shifted(-shift)
    }
}

pub type Ring = Vec<[f64; 2]>;

/// A polygon: first ring is the outer boundary, the rest are holes.
/// Rendering uses the even-odd rule, so ring orientation does not matter.
#[derive(Debug, Clone)]
pub struct Polygon {
    pub rings: Vec<Ring>,
    pub bbox: BBox,
    pub area: f64,
}

impl Polygon {
    pub fn new(rings: Vec<Ring>) -> Self {
        let mut bbox = BBox::empty();
        for r in &rings {
            for p in r {
                bbox.add_point(p[0], p[1]);
            }
        }
        let area = rings.first().map(ring_area).unwrap_or(0.0);
        Polygon { rings, bbox, area }
    }
}

fn ring_area(r: &Ring) -> f64 {
    let n = r.len();
    let mut s = 0.0;
    for i in 0..n {
        let a = r[i];
        let b = r[(i + 1) % n];
        s += a[0] * b[1] - b[0] * a[1];
    }
    (s / 2.0).abs()
}

#[derive(Debug, Clone)]
pub struct Country {
    pub name: String,
    pub long_name: String,
    /// ISO-3166 alpha-3 style code (Natural Earth ADM0_A3).
    pub code: String,
    pub iso2: String,
    pub continent: String,
    pub subregion: String,
    pub polygons: Vec<Polygon>,
    pub bbox: BBox,
}

impl Country {
    fn from_parts(
        name: String,
        long_name: String,
        code: String,
        iso2: String,
        continent: String,
        subregion: String,
        polygons: Vec<Polygon>,
    ) -> Self {
        let bbox = polygons.iter().fold(BBox::empty(), |b, p| b.union(&p.bbox));
        Country {
            name,
            long_name,
            code,
            iso2,
            continent,
            subregion,
            polygons,
            bbox,
        }
    }
}

#[derive(Deserialize)]
struct RawCountry {
    name: String,
    long_name: String,
    code: String,
    iso2: String,
    continent: String,
    subregion: String,
    /// polygons -> rings -> flat [lon, lat, lon, lat, ...]
    polys: Vec<Vec<Vec<f64>>>,
}

pub fn builtin_countries() -> Result<Vec<Country>> {
    let raw: Vec<RawCountry> =
        serde_json::from_str(COUNTRIES_JSON).context("parsing built-in country data")?;
    Ok(raw
        .into_iter()
        .map(|r| {
            let polygons = r
                .polys
                .into_iter()
                .map(|rings| {
                    Polygon::new(
                        rings
                            .into_iter()
                            .map(|flat| {
                                flat.as_chunks::<2>()
                                    .0
                                    .iter()
                                    .map(|c| [c[0], c[1]])
                                    .collect()
                            })
                            .collect(),
                    )
                })
                .collect();
            Country::from_parts(
                r.name,
                r.long_name,
                r.code,
                r.iso2,
                r.continent,
                r.subregion,
                polygons,
            )
        })
        .collect())
}

#[derive(Debug, Clone, Deserialize)]
pub struct Place {
    pub name: String,
    pub ascii: String,
    /// ADM0_A3 code of the country the place is in.
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    pub pop: u64,
    pub capital: bool,
}

/// Built-in populated places, sorted by population (largest first).
pub fn builtin_places() -> Result<Vec<Place>> {
    serde_json::from_str(PLACES_JSON).context("parsing built-in place data")
}

/// Load polygons from a GeoJSON file (FeatureCollection, Feature or bare
/// Polygon/MultiPolygon geometry). Each feature becomes one "country".
pub fn load_geojson(path: &Path) -> Result<Vec<Country>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading GeoJSON {}", path.display()))?;
    let v: Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing GeoJSON {}", path.display()))?;

    let features: Vec<&Value> = match v.get("type").and_then(Value::as_str) {
        Some("FeatureCollection") => v
            .get("features")
            .and_then(Value::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        Some("Feature") => vec![&v],
        Some("Polygon") | Some("MultiPolygon") => vec![&v],
        other => bail!("{}: unsupported GeoJSON type {:?}", path.display(), other),
    };

    let mut out = Vec::new();
    for (i, f) in features.iter().enumerate() {
        let (geom, props) = if f.get("type").and_then(Value::as_str) == Some("Feature") {
            (
                f.get("geometry").unwrap_or(&Value::Null),
                f.get("properties"),
            )
        } else {
            (*f, None)
        };
        let polygons = geometry_polygons(geom)?;
        if polygons.is_empty() {
            continue;
        }
        let prop = |keys: &[&str]| -> String {
            props
                .and_then(|p| {
                    keys.iter().find_map(|k| match p.get(*k) {
                        Some(Value::String(s)) if !s.is_empty() && s != "-99" => Some(s.clone()),
                        Some(Value::Number(n)) => Some(n.to_string()),
                        _ => None,
                    })
                })
                .unwrap_or_default()
        };
        let mut name = prop(&[
            "NAME", "name", "ADMIN", "admin", "NAME_EN", "name_en", "title",
        ]);
        if name.is_empty() {
            name = format!("feature-{}", i + 1);
        }
        let long_name = {
            let l = prop(&["NAME_LONG", "name_long", "ADMIN", "admin"]);
            if l.is_empty() { name.clone() } else { l }
        };
        let code = prop(&["ADM0_A3", "ISO_A3", "iso_a3", "adm0_a3", "code", "id"]);
        let iso2 = prop(&["ISO_A2_EH", "ISO_A2", "iso_a2"]);
        let continent = prop(&["CONTINENT", "continent"]);
        let subregion = prop(&["SUBREGION", "subregion", "region"]);
        out.push(Country::from_parts(
            name, long_name, code, iso2, continent, subregion, polygons,
        ));
    }
    if out.is_empty() {
        bail!(
            "{}: no Polygon or MultiPolygon features found",
            path.display()
        );
    }
    Ok(out)
}

fn geometry_polygons(geom: &Value) -> Result<Vec<Polygon>> {
    let coords = geom.get("coordinates");
    match geom.get("type").and_then(Value::as_str) {
        Some("Polygon") => Ok(vec![parse_polygon(
            coords.context("Polygon without coordinates")?,
        )?]),
        Some("MultiPolygon") => coords
            .and_then(Value::as_array)
            .context("MultiPolygon without coordinates")?
            .iter()
            .map(parse_polygon)
            .collect(),
        Some("GeometryCollection") => {
            let mut out = Vec::new();
            for g in geom
                .get("geometries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                out.extend(geometry_polygons(g)?);
            }
            Ok(out)
        }
        _ => Ok(Vec::new()),
    }
}

fn parse_polygon(v: &Value) -> Result<Polygon> {
    let rings = v
        .as_array()
        .context("polygon is not an array of rings")?
        .iter()
        .map(|ring| {
            ring.as_array()
                .context("ring is not an array")?
                .iter()
                .map(|pt| {
                    let lon = pt
                        .get(0)
                        .and_then(Value::as_f64)
                        .context("bad coordinate")?;
                    let lat = pt
                        .get(1)
                        .and_then(Value::as_f64)
                        .context("bad coordinate")?;
                    Ok([lon, lat])
                })
                .collect::<Result<Ring>>()
        })
        .collect::<Result<Vec<Ring>>>()?;
    Ok(Polygon::new(rings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamped_normalises_wrapped_views() {
        let b = BBox::new(200.0, -10.0, 260.0, 10.0).clamped(90.0);
        assert_eq!((b.west, b.east), (-160.0, -100.0));
        let b = BBox::new(150.0, -95.0, 210.0, 95.0).clamped(85.0);
        assert_eq!(
            (b.west, b.east, b.south, b.north),
            (150.0, 210.0, -85.0, 85.0)
        );
        let b = BBox::new(-200.0, 0.0, 200.0, 1.0).clamped(90.0);
        assert_eq!((b.west, b.east), (-180.0, 180.0));
    }

    #[test]
    fn union_wrapped_takes_the_short_way_round() {
        let fiji_west = BBox::new(177.0, -19.0, 180.0, -16.0);
        let fiji_east = BBox::new(-180.0, -19.0, -178.0, -16.0);
        let u = fiji_west.union_wrapped(&fiji_east);
        assert_eq!((u.west, u.east), (177.0, 182.0));
    }
}
