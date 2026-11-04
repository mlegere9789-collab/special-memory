//! Terrain classes and the terrain grid (SPEC-004 section 5).

use crate::{fx_ratio, parse_table, DataError, Provenance, Table};
use core_math::{Fx, Vec2Fx};

const SCHEMA: &str = "terrain/1";

/// Default grid cell size in mm.
pub const DEFAULT_CELL_MM: i64 = 8000;

/// One terrain class row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainClass {
    /// Class id used in grids; unique and ascending in the file.
    pub id: u8,
    /// Lower-case name.
    pub name: String,
    /// Speed multiplier in permille.
    pub speed_pm: u32,
    /// Fatigue effort multiplier in permille.
    pub effort_pm: u32,
    /// Line disorder in permille.
    pub line_disorder_pm: u32,
    /// Cover in permille (consumed by the fire rules).
    pub cover_pm: u32,
    /// True when soldiers cannot enter cells of this class.
    pub blocks_movement: bool,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// The loaded terrain classes, ascending by id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainTable {
    classes: Vec<TerrainClass>,
}

fn int_in(t: &mut Table, key: &str, lo: i64, hi: i64) -> Result<u32, DataError> {
    let v = t.take_int_in(key, lo, hi)?;
    u32::try_from(v).map_err(|_| DataError::Range(format!("{}.{key} = {v}", t.path())))
}

fn load_class(mut t: Table) -> Result<TerrainClass, DataError> {
    let at = t.path().to_owned();
    let id = u8::try_from(t.take_int_in("id", 0, 255)?)
        .map_err(|_| DataError::Range(format!("{at}.id")))?;
    let name = t.take_str("name")?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return Err(DataError::Parse(format!(
            "{at}.name must be lower-case letters and underscores"
        )));
    }
    let speed_pm = int_in(&mut t, "speed_pm", 0, 2000)?;
    let effort_pm = int_in(&mut t, "effort_pm", 0, 10_000)?;
    let line_disorder_pm = int_in(&mut t, "line_disorder_pm", 0, 1000)?;
    let cover_pm = int_in(&mut t, "cover_pm", 0, 1000)?;
    let blocks_movement = t.take_bool("blocks_movement")?;
    let provenance = t.take_provenance()?;
    t.finish()?;
    if !blocks_movement && speed_pm == 0 {
        return Err(DataError::Range(format!(
            "{at}: a passable class needs a positive speed_pm"
        )));
    }
    Ok(TerrainClass {
        id,
        name,
        speed_pm,
        effort_pm,
        line_disorder_pm,
        cover_pm,
        blocks_movement,
        provenance,
    })
}

impl TerrainTable {
    /// Parses and validates `terrain.toml` text.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`] and
    /// [`DataError::Provenance`]; [`DataError::Parse`] for a wrong schema tag, a missing key or
    /// a bad name; [`DataError::Range`] for out-of-range numbers or ids that are not strictly
    /// ascending.
    pub fn from_toml_str(text: &str) -> Result<TerrainTable, DataError> {
        let mut root = parse_table(text)?;
        let schema = root.take_str("schema")?;
        if schema != SCHEMA {
            return Err(DataError::Parse(format!(
                "schema must be {SCHEMA:?}, found {schema:?}"
            )));
        }
        let mut classes: Vec<TerrainClass> = Vec::new();
        for row in root.take_rows("class")? {
            let class = load_class(row)?;
            if classes.last().is_some_and(|p| class.id <= p.id) {
                return Err(DataError::Range(format!(
                    "class id {} is not strictly ascending",
                    class.id
                )));
            }
            if classes.iter().any(|p| p.name == class.name) {
                return Err(DataError::Parse(format!(
                    "duplicate class name {}",
                    class.name
                )));
            }
            classes.push(class);
        }
        root.finish()?;
        if classes.is_empty() {
            return Err(DataError::Parse("class: no rows".to_owned()));
        }
        Ok(TerrainTable { classes })
    }

    /// All classes, ascending by id.
    pub fn classes(&self) -> &[TerrainClass] {
        &self.classes
    }

    /// The class with this id, if the table has one.
    pub fn class(&self, id: u8) -> Option<&TerrainClass> {
        self.classes
            .binary_search_by_key(&id, |c| c.id)
            .ok()
            .map(|i| &self.classes[i])
    }
}

/// A rectangular grid of terrain class ids. Row 0 is the southern row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainGrid {
    origin_x: i64,
    origin_y: i64,
    cell: i64,
    width: usize,
    height: usize,
    cells: Vec<u8>,
    blocked: [bool; 256],
}

impl TerrainGrid {
    /// Builds a grid from text rows, row 0 at the south, one legend character per cell.
    /// `legend` maps a character to a class id of `table`. Cells are `cell_mm` wide and the grid
    /// starts at `(origin_x_mm, origin_y_mm)`.
    ///
    /// Errors: [`DataError::Range`] for a non-positive cell size or a legend class missing from
    /// `table`; [`DataError::Parse`] for no rows, unequal row lengths, a duplicate legend
    /// character or a cell character the legend lacks.
    pub fn from_rows<S: AsRef<str>>(
        cell_mm: i64,
        origin_x_mm: i64,
        origin_y_mm: i64,
        legend: &[(char, u8)],
        rows: &[S],
        table: &TerrainTable,
    ) -> Result<TerrainGrid, DataError> {
        if !(1..=1_000_000).contains(&cell_mm) {
            return Err(DataError::Range(format!(
                "cell_mm = {cell_mm} outside 1..=1000000"
            )));
        }
        for (i, (ch, class)) in legend.iter().enumerate() {
            if legend[..i].iter().any(|(c, _)| c == ch) {
                return Err(DataError::Parse(format!(
                    "duplicate legend character {ch:?}"
                )));
            }
            if table.class(*class).is_none() {
                return Err(DataError::Range(format!(
                    "legend {ch:?} names unknown class {class}"
                )));
            }
        }
        let height = rows.len();
        let width = rows.first().map_or(0, |r| r.as_ref().chars().count());
        if height == 0 || width == 0 {
            return Err(DataError::Parse("terrain grid has no cells".to_owned()));
        }
        let mut cells = Vec::with_capacity(width * height);
        for (y, row) in rows.iter().enumerate() {
            if row.as_ref().chars().count() != width {
                return Err(DataError::Parse(format!(
                    "terrain row {y} has a different length"
                )));
            }
            for ch in row.as_ref().chars() {
                let class = legend
                    .iter()
                    .find(|(c, _)| *c == ch)
                    .map(|(_, class)| *class)
                    .ok_or_else(|| DataError::Parse(format!("terrain row {y}: unmapped {ch:?}")))?;
                cells.push(class);
            }
        }
        let mut blocked = [false; 256];
        for c in table.classes() {
            blocked[usize::from(c.id)] = c.blocks_movement;
        }
        Ok(TerrainGrid {
            origin_x: fx_ratio(origin_x_mm, 1000).raw(),
            origin_y: fx_ratio(origin_y_mm, 1000).raw(),
            cell: fx_ratio(cell_mm, 1000).raw(),
            width,
            height,
            cells,
            blocked,
        })
    }

    /// Number of columns.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Number of rows.
    pub fn height(&self) -> usize {
        self.height
    }

    fn index(&self, v: Fx, origin: i64, n: usize) -> usize {
        let cell = (i128::from(v.raw()) - i128::from(origin)).div_euclid(i128::from(self.cell));
        let last = i128::try_from(n - 1).unwrap_or(0);
        usize::try_from(cell.clamp(0, last)).unwrap_or(0)
    }

    /// The class id at a position in metres: cell index `floor((pos - origin) / cell)` per axis,
    /// clamped to the grid so an outside position takes the nearest edge cell.
    pub fn class_at(&self, pos: Vec2Fx) -> u8 {
        let cx = self.index(pos.x, self.origin_x, self.width);
        let cy = self.index(pos.y, self.origin_y, self.height);
        self.cells[cy * self.width + cx]
    }

    /// True when the cell at `pos` (after the edge clamp) blocks movement.
    pub fn blocks(&self, pos: Vec2Fx) -> bool {
        self.blocked[usize::from(self.class_at(pos))]
    }

    /// Applies a displacement one axis at a time (x, then y). An axis step whose destination lies
    /// in a blocking cell is dropped, so the soldier slides along the other axis when that one is
    /// free. A soldier already inside a blocking cell may step out into a free cell but cannot
    /// move within blocking terrain.
    pub fn apply_move(&self, pos: Vec2Fx, displacement: Vec2Fx) -> Vec2Fx {
        let mut out = pos;
        let try_x = Vec2Fx::new(pos.x + displacement.x, pos.y);
        if !self.blocks(try_x) {
            out = try_x;
        }
        let try_y = Vec2Fx::new(out.x, out.y + displacement.y);
        if !self.blocks(try_y) {
            out = try_y;
        }
        out
    }
}
