//! Reader for a survey directory: `survey.toml`, the four feature files and the two ingest rasters
//! (`dem_cm.i32`, `coverage.u8`; SPEC-014 section 3). Strict: unknown keys, floats, dates and
//! missing provenance are refused with a message naming the file and row.

use crate::geom::Pt;
use std::path::Path;
use toml::{Table, Value};

/// Class names by id (SPEC-004 classes 0 to 7 and amendment A3).
pub const CLASS_NAMES: [&str; 12] = [
    "open",
    "road",
    "orchard",
    "woods",
    "thicket",
    "marsh",
    "ford",
    "deep_water",
    "crop",
    "rocky",
    "yard",
    "building",
];
/// Raster priority, lowest first, as class ids (SPEC-014 section 4.3).
pub const PRIORITY: [u8; 12] = [0, 8, 2, 3, 4, 9, 5, 10, 7, 6, 1, 11];
/// Default cover height above ground per class, decimetres (CT-014-2).
pub const DEFAULT_OCC_DM: [u8; 12] = [0, 0, 45, 180, 25, 10, 0, 0, 10, 12, 20, 70];
/// Classes a land-cover polygon may take.
const POLYGON_CLASSES: [&str; 9] = [
    "crop",
    "orchard",
    "woods",
    "thicket",
    "rocky",
    "marsh",
    "yard",
    "deep_water",
    "ford",
];
const LINE_KINDS: [&str; 7] = [
    "road",
    "creek",
    "stone_wall",
    "rail_fence",
    "board_fence",
    "hedge",
    "earthwork",
];

/// A land-cover polygon: outer ring and holes, vertices without the closing repeat.
pub struct Polygon {
    pub id: u32,
    pub class: u8,
    pub outer: Vec<Pt>,
    pub holes: Vec<Vec<Pt>>,
    pub occ_dm: Option<u8>,
}

/// A polyline feature (`kind` is one of the linear kinds of section 3).
pub struct Line {
    pub id: u32,
    pub kind: &'static str,
    pub pts: Vec<Pt>,
    pub half_width_mm: i64,
    pub dead_end: bool,
}

/// A building footprint with its wall height.
pub struct Building {
    pub id: u32,
    pub height_mm: u16,
    pub outer: Vec<Pt>,
}

/// Everything the check and the compile read.
pub struct Survey {
    pub origin_easting_mm: i64,
    pub origin_northing_mm: i64,
    pub crs: String,
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub width_posts: usize,
    pub height_posts: usize,
    pub slabs: Vec<terrain::Slab>,
    /// Centimetres, row 0 south, `i32::MIN` void.
    pub dem: Vec<i32>,
    /// Source id per post, 0 none.
    pub coverage: Vec<u8>,
    pub polygons: Vec<Polygon>,
    pub lines: Vec<Line>,
    pub buildings: Vec<Building>,
    pub name_ids: Vec<u32>,
}

type Res<T> = Result<T, String>;

fn no_floats(v: &Value, ctx: &str) -> Res<()> {
    match v {
        Value::Float(_) => Err(format!("{ctx}: a float (survey data are integers only)")),
        Value::Datetime(_) => Err(format!("{ctx}: a date value is not allowed")),
        Value::Array(a) => a.iter().try_for_each(|x| no_floats(x, ctx)),
        Value::Table(t) => t.values().try_for_each(|x| no_floats(x, ctx)),
        _ => Ok(()),
    }
}

fn read_doc(dir: &Path, file: &str, schema: &str) -> Res<Table> {
    let text = std::fs::read_to_string(dir.join(file)).map_err(|e| format!("{file}: {e}"))?;
    let mut doc: Table = text.parse().map_err(|e| format!("{file}: {e}"))?;
    no_floats(&Value::Table(doc.clone()), file)?;
    match doc.remove("schema") {
        Some(Value::String(s)) if s == schema => Ok(doc),
        _ => Err(format!("{file}: schema must be \"{schema}\"")),
    }
}

fn int(t: &mut Table, k: &str, ctx: &str) -> Res<i64> {
    match t.remove(k) {
        Some(Value::Integer(v)) => Ok(v),
        _ => Err(format!("{ctx}: `{k}` must be an integer")),
    }
}

fn ranged(t: &mut Table, k: &str, lo: i64, hi: i64, ctx: &str) -> Res<i64> {
    let v = int(t, k, ctx)?;
    if (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{ctx}: `{k}` must be {lo} to {hi}"))
    }
}

fn text(t: &mut Table, k: &str, ctx: &str) -> Res<String> {
    match t.remove(k) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s),
        _ => Err(format!("{ctx}: `{k}` must be a non-empty string")),
    }
}

fn pt(v: &Value, ctx: &str) -> Res<Pt> {
    match v {
        Value::Array(a) => match a.as_slice() {
            [Value::Integer(x), Value::Integer(y)] => Ok([*x, *y]),
            _ => Err(format!("{ctx}: a point is [x, y] in integer millimetres")),
        },
        _ => Err(format!("{ctx}: a point is [x, y] in integer millimetres")),
    }
}

fn points(v: Option<Value>, k: &str, ctx: &str) -> Res<Vec<Pt>> {
    match v {
        Some(Value::Array(a)) => a.iter().map(|p| pt(p, ctx)).collect(),
        _ => Err(format!("{ctx}: `{k}` must be an array of points")),
    }
}

fn finish(t: &Table, ctx: &str) -> Res<()> {
    match t.keys().next() {
        Some(k) => Err(format!("{ctx}: unknown key `{k}`")),
        None => Ok(()),
    }
}

/// Rows of `[[name]]`; a file with no such rows is valid.
fn rows(doc: &mut Table, name: &str, file: &str) -> Res<Vec<Table>> {
    let list = match doc.remove(name) {
        None => return finish(doc, file).map(|()| Vec::new()),
        Some(Value::Array(a)) => a,
        Some(_) => return Err(format!("{file}: `{name}` must be an array of tables")),
    };
    finish(doc, file)?;
    list.into_iter()
        .map(|v| match v {
            Value::Table(t) => Ok(t),
            _ => Err(format!("{file}: `{name}` rows are tables")),
        })
        .collect()
}

/// Provenance of a survey row: `confidence`, `sources` (source id plus page or sheet), and a
/// `second_source` for `verified`.
fn provenance(t: &mut Table, ctx: &str) -> Res<()> {
    match t.remove("confidence") {
        Some(Value::String(c)) if ["verified", "partial", "unverified"].contains(&c.as_str()) => {
            if c == "verified" && text(t, "second_source", ctx).is_err() {
                return Err(format!("{ctx}: `verified` needs a `second_source`"));
            }
        }
        _ => {
            return Err(format!(
                "{ctx}: `confidence` must be verified, partial or unverified"
            ))
        }
    }
    for k in ["second_source", "note"] {
        t.remove(k);
    }
    match t.remove("sources") {
        Some(Value::Array(a)) if !a.is_empty() => {
            for s in a {
                let Value::Table(mut s) = s else {
                    return Err(format!("{ctx}: `sources` entries are tables"));
                };
                ranged(&mut s, "source", 1, 65535, ctx)?;
                let (page, sheet) = (s.remove("page").is_some(), s.remove("sheet").is_some());
                if page == sheet {
                    return Err(format!(
                        "{ctx}: a source needs exactly one of `page`, `sheet`"
                    ));
                }
                finish(&s, ctx)?;
            }
            Ok(())
        }
        _ => Err(format!("{ctx}: `sources` must list at least one source")),
    }
}

fn row_id(t: &mut Table, ctx: &str) -> Res<u32> {
    Ok(ranged(t, "id", 1, i64::from(u32::MAX), ctx)? as u32)
}

fn polygons(dir: &Path) -> Res<Vec<Polygon>> {
    let file = "landcover.toml";
    let mut out = Vec::new();
    for (n, mut t) in rows(
        &mut read_doc(dir, file, "survey-landcover/1")?,
        "polygon",
        file,
    )?
    .into_iter()
    .enumerate()
    {
        let ctx = format!("{file} row {}", n + 1);
        let id = row_id(&mut t, &ctx)?;
        let name = text(&mut t, "class", &ctx)?;
        let class = POLYGON_CLASSES
            .contains(&name.as_str())
            .then(|| CLASS_NAMES.iter().position(|c| *c == name))
            .flatten()
            .ok_or_else(|| format!("{ctx}: `class` `{name}` is not a polygon class"))?;
        let occ_dm = match t.contains_key("occ_dm") {
            true => Some(ranged(&mut t, "occ_dm", 0, 255, &ctx)? as u8),
            false => None,
        };
        let outer = points(t.remove("outer"), "outer", &ctx)?;
        let holes = match t.remove("holes") {
            None => Vec::new(),
            Some(Value::Array(h)) => h
                .into_iter()
                .map(|r| points(Some(r), "holes", &ctx))
                .collect::<Res<_>>()?,
            Some(_) => return Err(format!("{ctx}: `holes` must be an array of rings")),
        };
        provenance(&mut t, &ctx)?;
        finish(&t, &ctx)?;
        out.push(Polygon {
            id,
            class: class as u8,
            outer,
            holes,
            occ_dm,
        });
    }
    Ok(out)
}

fn lines(dir: &Path) -> Res<Vec<Line>> {
    let file = "linear.toml";
    let mut out = Vec::new();
    for (n, mut t) in rows(&mut read_doc(dir, file, "survey-linear/1")?, "line", file)?
        .into_iter()
        .enumerate()
    {
        let ctx = format!("{file} row {}", n + 1);
        let id = row_id(&mut t, &ctx)?;
        let name = text(&mut t, "kind", &ctx)?;
        let kind = *LINE_KINDS
            .iter()
            .find(|k| **k == name)
            .ok_or_else(|| format!("{ctx}: unknown kind `{name}`"))?;
        let wide = kind == "road" || kind == "creek";
        let half_width_mm = if wide {
            ranged(&mut t, "half_width_mm", 1, 100_000, &ctx)?
        } else {
            0
        };
        let dead_end = match (kind == "road", t.remove("dead_end")) {
            (true, Some(Value::Boolean(b))) => b,
            (true, None) => false,
            (_, None) => false,
            _ => return Err(format!("{ctx}: `dead_end` is a boolean on roads only")),
        };
        let pts = points(t.remove("points"), "points", &ctx)?;
        provenance(&mut t, &ctx)?;
        finish(&t, &ctx)?;
        out.push(Line {
            id,
            kind,
            pts,
            half_width_mm,
            dead_end,
        });
    }
    Ok(out)
}

fn buildings(dir: &Path) -> Res<Vec<Building>> {
    let file = "buildings.toml";
    let mut out = Vec::new();
    for (n, mut t) in rows(
        &mut read_doc(dir, file, "survey-buildings/1")?,
        "building",
        file,
    )?
    .into_iter()
    .enumerate()
    {
        let ctx = format!("{file} row {}", n + 1);
        let id = row_id(&mut t, &ctx)?;
        let height_mm = ranged(&mut t, "height_mm", 1, 65535, &ctx)? as u16;
        let outer = points(t.remove("outer"), "outer", &ctx)?;
        provenance(&mut t, &ctx)?;
        finish(&t, &ctx)?;
        out.push(Building {
            id,
            height_mm,
            outer,
        });
    }
    Ok(out)
}

fn names(dir: &Path) -> Res<Vec<u32>> {
    let file = "names.toml";
    let mut out = Vec::new();
    for (n, mut t) in rows(&mut read_doc(dir, file, "survey-names/1")?, "name", file)?
        .into_iter()
        .enumerate()
    {
        let ctx = format!("{file} row {}", n + 1);
        out.push(row_id(&mut t, &ctx)?);
        text(&mut t, "name", &ctx)?;
        text(&mut t, "kind", &ctx)?;
        pt(&t.remove("anchor").unwrap_or(Value::Boolean(false)), &ctx)?;
        if t.contains_key("polygon") {
            points(t.remove("polygon"), "polygon", &ctx)?;
        }
        provenance(&mut t, &ctx)?;
        finish(&t, &ctx)?;
    }
    Ok(out)
}

fn rasters(dir: &Path, w: usize, h: usize) -> Res<(Vec<i32>, Vec<u8>)> {
    let read = |f: &str, n: usize| {
        let b = std::fs::read(dir.join(f)).map_err(|e| format!("{f}: {e}"))?;
        if b.len() == n {
            Ok(b)
        } else {
            Err(format!("{f}: {} bytes, expected {n}", b.len()))
        }
    };
    let dem = read("dem_cm.i32", w * h * 4)?;
    let dem = dem
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Ok((dem, read("coverage.u8", w * h)?))
}

/// Loads and validates the survey in `dir`.
pub fn load(dir: &Path) -> Res<Survey> {
    let mut doc = read_doc(dir, "survey.toml", "survey/1")?;
    let ctx = "survey.toml";
    let origin_easting_mm = int(&mut doc, "origin_easting_mm", ctx)?;
    let origin_northing_mm = int(&mut doc, "origin_northing_mm", ctx)?;
    let crs = text(&mut doc, "crs", ctx)?;
    let tiles_x = ranged(&mut doc, "tiles_x", 1, 1024, ctx)? as u32;
    let tiles_y = ranged(&mut doc, "tiles_y", 1, 1024, ctx)? as u32;
    let width_posts = ranged(&mut doc, "width_posts", 1, 1 << 20, ctx)? as usize;
    let height_posts = ranged(&mut doc, "height_posts", 1, 1 << 20, ctx)? as usize;
    let mut slabs = Vec::new();
    for mut t in rows(&mut doc, "slab", ctx)? {
        let id = text(&mut t, "id", ctx)?;
        if !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(format!(
                "{ctx}: slab id `{id}` must be lower-case letters, digits, underscore"
            ));
        }
        let mut c = |k: &str, hi: u32| ranged(&mut t, k, 0, i64::from(hi) - 1, ctx);
        let (x0, y0, x1, y1) = (
            c("x0", tiles_x)?,
            c("y0", tiles_y)?,
            c("x1", tiles_x)?,
            c("y1", tiles_y)?,
        );
        finish(&t, ctx)?;
        if x1 < x0 || y1 < y0 {
            return Err(format!("{ctx}: slab `{id}` is empty"));
        }
        slabs.push(terrain::Slab { id, x0, y0, x1, y1 });
    }
    let (dem, coverage) = rasters(dir, width_posts, height_posts)?;
    Ok(Survey {
        origin_easting_mm,
        origin_northing_mm,
        crs,
        tiles_x,
        tiles_y,
        width_posts,
        height_posts,
        slabs,
        dem,
        coverage,
        polygons: polygons(dir)?,
        lines: lines(dir)?,
        buildings: buildings(dir)?,
        name_ids: names(dir)?,
    })
}
