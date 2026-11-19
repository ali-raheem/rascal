# Rascal

ASCII art maps of the world, continents and countries, for the terminal.

```
$ rascal europe -w 60 -m London -m 'Paris,@' -m 41.9,12.5,X,Rome
```

Country polygons and city locations are compiled into the binary, so there is
nothing to download at runtime.

## Build

```
cargo build --release
./target/release/rascal --help
```

## Usage

```
rascal [OPTIONS] [REGION]...
```

### Regions

A region can be any of:

| kind       | examples                                                                     |
|------------|------------------------------------------------------------------------------|
| world      | `world` (the default)                                                        |
| continent  | `europe`, `africa`, `asia`, `north-america`, `south-america`, `oceania`, `antarctica`, `americas` |
| subregion  | `western-europe`, `south-eastern-asia`, `caribbean` (`rascal --list subregions`) |
| country    | `france`, `FRA`, `FR`, `"united kingdom"`, `uk` (`rascal --list countries`)  |

Several regions can be combined, e.g. `rascal france,spain,portugal`. The map
zooms to frame them, and draws other land in view with the "other" character
(`:` by default). Use `-i/--isolate` to show only the selected countries.

When zooming to a country, far-flung territories are left out of the frame
(France shows Corsica but not French Guiana). Use `--bbox W,S,E,N` to set the
view yourself.

### Characters

```
rascal --land '#' --water '-'        # your own characters
rascal --water '~ '                  # several characters form a pattern
rascal --border '|' --coast 'o'      # country borders and coastlines
rascal -s blocks                     # a preset style (--list styles)
rascal --highlight brazil=@ --highlight 'argentina=%' south-america
```

Any Unicode character can be used, as long as it occupies one terminal cell.

Styles: `classic`, `sea`, `dots`, `blocks`, `shade`, `braille`.

### Markers

```
-m LAT,LON[,SYMBOL[,LABEL]]    -m 51.5,-0.12,*,London    -m 33.9S,18.4E
-m PLACE[,SYMBOL[,LABEL]]      -m Tokyo    -m 'Paris,@'    -m Cordoba/ESP
-M FILE                        one spec per line, # comments
--capitals                     mark the capitals of the selected countries
--fit                          zoom to include all markers
--legend                       list highlights and markers under the map
--no-labels                    symbols only
```

Places are looked up among about 1,250 cities (`rascal --list places france`).
If two places share a name, the larger one wins; use `Name/CODE` to choose a
different one.

### Layout and output

```
-w/--width, -H/--height    size in characters (default: fit the terminal)
-p mercator                projection (default: equirectangular)
--aspect 0.5               character cell width/height ratio
-f, --frame=double         frame: ascii, line, rounded, double, heavy
-t "Title"                 title (inside the frame if there is one)
-g 30                      graticule every 30 degrees
--color[=auto|always]      ANSI colours (configurable per element)
--supersample N            samples per cell side when rasterising (default 3)
--threshold F              share of a cell that must be land (default 0.5)
```

### Config file

Rascal reads `~/.config/rascal/config.toml` if it exists (or `-c FILE`).
Command-line flags override it. `--no-config` skips the default file.
`--print-config` prints the settings in effect as TOML, which is an easy way
to start one:

```
rascal europe -s sea --border '|' --print-config > ~/.config/rascal/config.toml
```

See [`examples/rascal.toml`](examples/rascal.toml) for every option, including
`[chars]`, `[colors]`, `[highlight]` and `[[marker]]` tables.

### Your own polygons

`--geojson FILE` replaces the built-in countries with polygons from any
GeoJSON `FeatureCollection` (Polygon / MultiPolygon features). Name, code,
continent and subregion are read from the usual Natural Earth property names
(`NAME`/`name`, `ADM0_A3`/`ISO_A3`, `CONTINENT`, `SUBREGION`), so regions and
highlights work the same way. Use this for more detail (e.g. Natural Earth
1:10m) or for maps that aren't countries at all, such as provinces or states.

## Data

Built-in data comes from [Natural Earth](https://www.naturalearthdata.com/)
1:50m Admin 0 countries and populated places (public domain). To regenerate
`data/` from newer releases:

```
python3 -I scripts/build_data.py \
    ne_50m_admin_0_countries.geojson \
    ne_50m_populated_places_simple.geojson \
    data
```

Natural Earth draws borders as the countries de facto control them. Some
borders are disputed.

## How it works

Each polygon is projected and scanline-filled (even-odd rule, so lakes and
enclaves are holes) onto a grid 3x finer than the character grid. Each
character cell then goes to the country covering most of its samples, if
land covers at least half the cell. Borders and coasts are cells next to a
different country or to water. A selected or highlighted country too small to
win a cell still gets one, so Monaco and Malta don't disappear.

## Limitations

- Views cannot cross the antimeridian (180°), so a Pacific-centred map is
  not possible yet.
- Labels go on the map itself, to the right of the marker or to its left
  when there is no room. A label that would overlap another marker or label
  is dropped.
- Wide characters such as CJK and most emoji take two cells and will break
  the alignment.
