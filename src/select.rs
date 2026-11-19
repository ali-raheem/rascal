//! Resolving region selectors ("europe", "FRA", "western-europe", "world")
//! into sets of countries and a viewport.

use anyhow::{Result, bail};

use crate::data::{BBox, Country};

/// The default world view crops Antarctica, which otherwise dominates
/// equirectangular and Mercator maps.
pub const WORLD_VIEW: BBox = BBox {
    west: -180.0,
    south: -58.0,
    east: 180.0,
    north: 84.0,
};

/// Hand-tuned continent viewports. Clustering by polygon would let e.g.
/// Russia drag "Europe" out to the Pacific, or French Guiana pull it to
/// South America.
/// East Asia and Australia on the left, the Americas on the right.
pub const PACIFIC_VIEW: BBox = BBox {
    west: 110.0,
    south: -55.0,
    east: 295.0,
    north: 66.0,
};

const CONTINENT_VIEWS: &[(&str, BBox)] = &[
    (
        "africa",
        BBox {
            west: -26.0,
            south: -36.0,
            east: 60.0,
            north: 38.0,
        },
    ),
    (
        "antarctica",
        BBox {
            west: -180.0,
            south: -90.0,
            east: 180.0,
            north: -60.0,
        },
    ),
    (
        "asia",
        BBox {
            west: 25.0,
            south: -12.0,
            east: 180.0,
            north: 78.0,
        },
    ),
    (
        "europe",
        BBox {
            west: -25.0,
            south: 34.0,
            east: 45.0,
            north: 72.0,
        },
    ),
    (
        "northamerica",
        BBox {
            west: -170.0,
            south: 5.0,
            east: -50.0,
            north: 84.0,
        },
    ),
    (
        "oceania",
        BBox {
            west: 110.0,
            south: -50.0,
            east: 192.0,
            north: 0.0,
        },
    ),
    (
        "southamerica",
        BBox {
            west: -92.0,
            south: -57.0,
            east: -33.0,
            north: 14.0,
        },
    ),
];

const ALIASES: &[(&str, &str)] = &[
    ("uk", "gbr"),
    ("britain", "gbr"),
    ("greatbritain", "gbr"),
    ("england", "gbr"),
    ("america", "usa"),
    ("unitedstatesofamerica", "usa"),
    ("uae", "are"),
    ("drc", "cod"),
    ("holland", "nld"),
    ("burma", "mmr"),
];

pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub indices: Vec<usize>,
    /// Viewport that frames the selection.
    pub view: BBox,
    /// True when the view is a preset (world/continent) and should not be padded.
    pub preset: bool,
}

pub fn resolve(countries: &[Country], selector: &str) -> Result<Selection> {
    let mut n = norm(selector);
    if n.is_empty() {
        bail!("empty region selector");
    }
    if matches!(n.as_str(), "world" | "all" | "earth" | "globe") {
        return Ok(Selection {
            indices: (0..countries.len()).collect(),
            view: WORLD_VIEW,
            preset: true,
        });
    }
    if matches!(n.as_str(), "pacific" | "pacificocean") {
        return Ok(Selection {
            indices: (0..countries.len()).collect(),
            view: PACIFIC_VIEW,
            preset: true,
        });
    }
    if n == "americas" {
        let indices = filter(countries, |c| {
            let k = norm(&c.continent);
            k == "northamerica" || k == "southamerica"
        });
        return Ok(Selection {
            indices,
            view: BBox::new(-170.0, -57.0, -33.0, 84.0),
            preset: true,
        });
    }
    if let Some((_, to)) = ALIASES.iter().find(|(from, _)| *from == n) {
        n = to.to_string();
    }

    // Continent
    let idx = filter(countries, |c| norm(&c.continent) == n);
    if !idx.is_empty() {
        if let Some((_, view)) = CONTINENT_VIEWS.iter().find(|(k, _)| *k == n) {
            return Ok(Selection {
                indices: idx,
                view: *view,
                preset: true,
            });
        }
        let view = cluster_view(countries, &idx);
        return Ok(Selection {
            indices: idx,
            view,
            preset: false,
        });
    }

    // Subregion
    let idx = filter(countries, |c| norm(&c.subregion) == n);
    if !idx.is_empty() {
        let view = cluster_view(countries, &idx);
        return Ok(Selection {
            indices: idx,
            view,
            preset: false,
        });
    }

    // Exact country match on code / name
    let idx = filter(countries, |c| {
        norm(&c.code) == n || norm(&c.iso2) == n || norm(&c.name) == n || norm(&c.long_name) == n
    });
    if !idx.is_empty() {
        let view = cluster_view(countries, &idx);
        return Ok(Selection {
            indices: idx,
            view,
            preset: false,
        });
    }

    // Unique prefix match on country names
    let idx = filter(countries, |c| {
        norm(&c.name).starts_with(&n) || norm(&c.long_name).starts_with(&n)
    });
    match idx.len() {
        1 => {
            let view = cluster_view(countries, &idx);
            Ok(Selection {
                indices: idx,
                view,
                preset: false,
            })
        }
        0 => bail!(
            "unknown region '{selector}' (try `rascal --list countries`, `--list continents` or `--list subregions`)"
        ),
        _ => {
            let names: Vec<&str> = idx
                .iter()
                .take(8)
                .map(|&i| countries[i].name.as_str())
                .collect();
            bail!("region '{selector}' is ambiguous: {}", names.join(", "))
        }
    }
}

fn filter(countries: &[Country], f: impl Fn(&Country) -> bool) -> Vec<usize> {
    countries
        .iter()
        .enumerate()
        .filter(|(_, c)| f(c))
        .map(|(i, _)| i)
        .collect()
}

/// Frame the "main" landmass of a set of countries: start from the largest
/// polygon and absorb any polygon lying near the growing cluster. This keeps
/// e.g. Corsica in a map of France but drops French Guiana and Réunion.
pub fn cluster_view(countries: &[Country], indices: &[usize]) -> BBox {
    let mut polys: Vec<(f64, BBox)> = indices
        .iter()
        .flat_map(|&i| countries[i].polygons.iter().map(|p| (p.area, p.bbox)))
        .collect();
    if polys.is_empty() {
        return WORLD_VIEW;
    }
    polys.sort_by(|a, b| b.0.total_cmp(&a.0));
    // Only sizeable landmasses may extend the cluster; otherwise chains of
    // tiny islands (e.g. Japan's southern outliers) drag the view out to sea.
    let significant = polys[0].0 * 0.002;
    let mut used = vec![false; polys.len()];
    used[0] = true;
    let mut view = polys[0].1;
    loop {
        let tol = 2.0 + 0.05 * view.width().max(view.height());
        let mut changed = false;
        for (i, (area, b)) in polys.iter().enumerate() {
            if used[i] || *area < significant {
                continue;
            }
            // Compare against the copy of the polygon nearest the cluster,
            // so e.g. Chukotka (at -170°) joins the rest of Russia.
            let (gap, b) = view.nearest_copy(b);
            if gap <= tol {
                used[i] = true;
                view = view.union(&b);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Small islands just off the coast still belong in the picture.
    let core = view;
    for (i, (_, b)) in polys.iter().enumerate() {
        let (gap, b) = core.nearest_copy(b);
        if !used[i] && gap <= 1.0 {
            view = view.union(&b);
        }
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::builtin_countries;

    #[test]
    fn resolves_names_codes_and_regions() {
        let cs = builtin_countries().unwrap();
        for s in ["France", "FRA", "fr", "france"] {
            let sel = resolve(&cs, s).unwrap();
            assert_eq!(sel.indices.len(), 1);
            assert_eq!(cs[sel.indices[0]].code, "FRA");
        }
        assert_eq!(cs[resolve(&cs, "uk").unwrap().indices[0]].code, "GBR");
        assert!(resolve(&cs, "europe").unwrap().indices.len() > 40);
        assert!(resolve(&cs, "Western Europe").unwrap().preset == false);
        assert!(resolve(&cs, "narnia").is_err());
    }

    #[test]
    fn country_view_skips_distant_territories() {
        let cs = builtin_countries().unwrap();
        // Mainland France plus Corsica, not French Guiana or Réunion.
        let v = resolve(&cs, "france").unwrap().view;
        assert!(
            v.west > -6.0 && v.east < 10.0 && v.south > 41.0 && v.north < 52.0,
            "{v:?}"
        );
    }

    #[test]
    fn country_view_joins_across_the_antimeridian() {
        let cs = builtin_countries().unwrap();
        // Chukotka lies at -170°; it should extend Russia eastwards past 180°.
        let v = resolve(&cs, "russia").unwrap().view;
        assert!(v.west > 15.0 && v.east > 185.0 && v.east < 200.0, "{v:?}");
        let v = resolve(&cs, "fiji").unwrap().view;
        assert!(v.width() < 10.0, "{v:?}");
    }

    #[test]
    fn pacific_view_crosses_the_antimeridian() {
        let cs = builtin_countries().unwrap();
        let s = resolve(&cs, "pacific").unwrap();
        assert!(s.preset && s.view.west < 180.0 && s.view.east > 180.0);
        assert_eq!(s.indices.len(), cs.len());
    }
}
