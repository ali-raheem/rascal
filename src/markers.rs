//! Marker specs: `LAT,LON[,SYMBOL[,LABEL]]` or `PLACE[,SYMBOL[,LABEL]]`.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::MarkerConfig;
use crate::data::Place;
use crate::select::norm;

#[derive(Debug, Clone)]
pub struct Marker {
    pub lat: f64,
    pub lon: f64,
    pub symbol: Option<String>,
    pub label: Option<String>,
}

/// Parse a coordinate like `51.5`, `-0.12`, `51.5N` or `0.12W`.
fn coord(s: &str, pos: char, neg: char) -> Option<f64> {
    let s = s.trim().trim_end_matches('°');
    let up = s.to_ascii_uppercase();
    if let Some(v) = up.strip_suffix(pos) {
        return v.trim().trim_end_matches('°').parse().ok();
    }
    if let Some(v) = up.strip_suffix(neg) {
        return v
            .trim()
            .trim_end_matches('°')
            .parse::<f64>()
            .ok()
            .map(|x| -x);
    }
    s.parse().ok()
}

fn nonempty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn find_place<'a>(places: &'a [Place], name: &str) -> Option<&'a Place> {
    // "Name/CODE" disambiguates by country code, e.g. "Cordoba/ESP".
    let (name, country) = match name.rsplit_once('/') {
        Some((n, c)) => (n, Some(norm(c))),
        None => (name, None),
    };
    let n = norm(name);
    // Places are sorted by population, so the first match is the biggest.
    places.iter().find(|p| {
        (norm(&p.name) == n || norm(&p.ascii) == n)
            && country.as_ref().is_none_or(|c| norm(&p.country) == *c)
    })
}

pub fn parse_spec(spec: &str, places: &[Place]) -> Result<Marker> {
    let parts: Vec<&str> = spec.split(',').collect();
    if parts.len() >= 2
        && let (Some(lat), Some(lon)) = (coord(parts[0], 'N', 'S'), coord(parts[1], 'E', 'W'))
    {
        check_range(lat, lon, spec)?;
        return Ok(Marker {
            lat,
            lon,
            symbol: nonempty(parts.get(2).copied()),
            label: nonempty(parts.get(3..).map(|p| p.join(",")).as_deref()),
        });
    }
    let name = parts[0].trim();
    let place = find_place(places, name).with_context(|| {
        format!(
            "marker '{spec}': '{name}' is not a known place; use LAT,LON[,SYMBOL[,LABEL]] instead"
        )
    })?;
    Ok(Marker {
        lat: place.lat,
        lon: place.lon,
        symbol: nonempty(parts.get(1).copied()),
        label: nonempty(parts.get(2..).map(|p| p.join(",")).as_deref())
            .or(Some(place.name.clone())),
    })
}

fn check_range(lat: f64, lon: f64, spec: &str) -> Result<()> {
    if !(-90.0..=90.0).contains(&lat) {
        if (-90.0..=90.0).contains(&lon) {
            bail!("marker '{spec}': latitude {lat} out of range (order is LAT,LON - swapped?)");
        }
        bail!("marker '{spec}': latitude {lat} out of range");
    }
    if !(-180.0..=180.0).contains(&lon) {
        bail!("marker '{spec}': longitude {lon} out of range");
    }
    Ok(())
}

/// One spec per line; blank lines and `#` comments are ignored, as is a
/// leading CSV header such as `lat,lon,symbol,label`.
pub fn load_file(path: &Path, places: &[Place]) -> Result<Vec<Marker>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading markers file {}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if out.is_empty() && i < 2 && line.to_ascii_lowercase().starts_with("lat") {
            continue;
        }
        out.push(
            parse_spec(line, places).with_context(|| format!("{}:{}", path.display(), i + 1))?,
        );
    }
    Ok(out)
}

pub fn from_config(m: &MarkerConfig, places: &[Place]) -> Result<Marker> {
    let (lat, lon, default_label) = match (&m.place, m.lat, m.lon) {
        (_, Some(lat), Some(lon)) => (lat, lon, None),
        (Some(name), _, _) => {
            let p = find_place(places, name)
                .with_context(|| format!("[[marker]]: '{name}' is not a known place"))?;
            (p.lat, p.lon, Some(p.name.clone()))
        }
        _ => bail!("[[marker]] needs either `place` or both `lat` and `lon`"),
    };
    check_range(lat, lon, "[[marker]]")?;
    Ok(Marker {
        lat,
        lon,
        symbol: nonempty(m.symbol.as_deref()),
        label: m.label.clone().or(default_label),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::builtin_places;

    #[test]
    fn parses_coordinate_specs() {
        let m = parse_spec("51.5,-0.12,%,Big Smoke, UK", &[]).unwrap();
        assert_eq!((m.lat, m.lon), (51.5, -0.12));
        assert_eq!(m.symbol.as_deref(), Some("%"));
        assert_eq!(m.label.as_deref(), Some("Big Smoke, UK"));
        let m = parse_spec("33.9S, 18.4E", &[]).unwrap();
        assert_eq!((m.lat, m.lon), (-33.9, 18.4));
        assert!(m.symbol.is_none() && m.label.is_none());
        assert!(parse_spec("10,200", &[]).is_err());
    }

    #[test]
    fn parses_place_specs() {
        let places = builtin_places().unwrap();
        let m = parse_spec("tokyo,@", &places).unwrap();
        assert!((m.lat - 35.7).abs() < 0.5);
        assert_eq!(m.symbol.as_deref(), Some("@"));
        assert_eq!(m.label.as_deref(), Some("Tokyo"));
        assert!(parse_spec("Cordoba", &places).unwrap().lon < -50.0);
        let es = parse_spec("Cordoba/ESP", &places).unwrap();
        assert!(es.lon > -10.0 && es.lon < 0.0);
        assert!(parse_spec("Atlantis", &places).is_err());
    }
}
