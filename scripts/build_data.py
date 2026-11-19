#!/usr/bin/env python3
"""Convert Natural Earth GeoJSON into Rascal's compact embedded data files.

Usage:
    python3 -I scripts/build_data.py COUNTRIES.geojson PLACES.geojson OUT_DIR

Inputs (public domain, https://www.naturalearthdata.com):
    ne_50m_admin_0_countries.geojson
    ne_50m_populated_places_simple.geojson
"""
import json
import sys


def clean(s):
    return " ".join(str(s).split())


def ring(coords, prec):
    out = []
    last = None
    for lon, lat, *_ in coords:
        pt = (round(lon, prec), round(lat, prec))
        if pt != last:
            out.extend(pt)
            last = pt
    return out


def polys(geom, prec):
    if geom["type"] == "Polygon":
        parts = [geom["coordinates"]]
    elif geom["type"] == "MultiPolygon":
        parts = geom["coordinates"]
    else:
        return []
    res = []
    for poly in parts:
        rings = [ring(r, prec) for r in poly]
        rings = [r for r in rings if len(r) >= 6]
        if rings:
            res.append(rings)
    return res


def main(countries_path, places_path, out_dir):
    with open(countries_path, encoding="utf-8") as f:
        countries = json.load(f)
    out = []
    for feat in countries["features"]:
        p = feat["properties"]
        code = p.get("ADM0_A3") or p.get("ISO_A3")
        iso2 = p.get("ISO_A2_EH") or p.get("ISO_A2") or ""
        if iso2 == "-99":
            iso2 = ""
        out.append({
            "name": clean(p["NAME"]),
            "long_name": clean(p.get("NAME_LONG") or p["NAME"]),
            "code": code,
            "iso2": iso2,
            "continent": p.get("CONTINENT", ""),
            "subregion": p.get("SUBREGION", ""),
            "polys": polys(feat["geometry"], 3),
        })
    out.sort(key=lambda c: c["name"])
    with open(f"{out_dir}/countries.json", "w", encoding="utf-8") as f:
        json.dump(out, f, separators=(",", ":"), ensure_ascii=False)

    with open(places_path, encoding="utf-8") as f:
        places = json.load(f)
    pl = []
    for feat in places["features"]:
        p = feat["properties"]
        pl.append({
            "name": clean(p["name"]),
            "ascii": clean(p.get("nameascii") or p["name"]),
            "country": p.get("adm0_a3", ""),
            "lat": round(p["latitude"], 4),
            "lon": round(p["longitude"], 4),
            "pop": int(p.get("pop_max") or 0),
            "capital": bool(p.get("adm0cap")),
        })
    pl.sort(key=lambda x: -x["pop"])
    with open(f"{out_dir}/places.json", "w", encoding="utf-8") as f:
        json.dump(pl, f, separators=(",", ":"), ensure_ascii=False)
    print(f"{len(out)} countries, {len(pl)} places")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
