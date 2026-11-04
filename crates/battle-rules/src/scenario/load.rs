//! The scenario loader: reads the TOML, applies rules V1 to V12 and sorts every row.

use super::{
    Arm, Arrival, Departure, MapSpec, Objective, Scenario, ScenarioError, Side, StrengthCheck,
    TerrainSpec, Trigger, TriggerKind, TriggerOrder, Unit, WeatherRow,
};
use crate::{
    fx_ratio, parse_table, DataError, Formation, Gait, RegimentId, Table, Tables, TerrainGrid,
    Training, TICK_HZ,
};
use core_math::Vec2Fx;
use sim_state::StateHasher;

type R<T> = Result<T, ScenarioError>;

/// Most units a scenario may hold (V11).
const MAX_UNITS: usize = 4096;
/// Most rows of any other kind (V11): the count is checked before a row is read.
const MAX_ROWS: usize = 65_536;
/// Most men in one unit (V11).
const MAX_MEN: i64 = 1_000_000;
/// Largest agent cap (V8).
const MAX_AGENT_CAP: i64 = 200_000;
/// Longest time in a row, seconds (one year).
const MAX_S: i64 = 31_536_000;
/// Most terrain cells (V10); 1250 x 1250 is 1.56 million.
const MAX_CELLS: usize = 16_777_216;

fn rng<T>(r: Result<T, DataError>) -> R<T> {
    r.map_err(|e| match e {
        DataError::Range(m) => ScenarioError::Range(m),
        e => ScenarioError::Data(e),
    })
}

fn uint(t: &mut Table, key: &str, hi: i64) -> R<u32> {
    let v = rng(t.take_int_in(key, 0, hi))?;
    Ok(u32::try_from(v).unwrap_or(u32::MAX))
}

fn range_of(v: i64, lo: i64, hi: i64, what: &str) -> R<i64> {
    if v < lo || v > hi {
        return Err(ScenarioError::Range(format!(
            "{what} = {v} outside {lo}..={hi}"
        )));
    }
    Ok(v)
}

fn id_ok(s: &str, extra_dash: bool) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || (extra_dash && b == b'-')
        })
}

fn take_id(t: &mut Table, key: &str) -> R<String> {
    let s = t.take_str(key)?;
    if !id_ok(&s, true) {
        return Err(ScenarioError::Data(DataError::Parse(format!(
            "{}.{key} = {s:?} must match [a-z0-9_-]{{1,32}}",
            t.path()
        ))));
    }
    Ok(s)
}

fn take_side(t: &mut Table, key: &str) -> R<Side> {
    match t.take_str(key)?.as_str() {
        "union" => Ok(Side::Union),
        "confederate" => Ok(Side::Confederate),
        other => Err(ScenarioError::Data(DataError::Parse(format!(
            "{}.{key} = {other:?} must be union or confederate",
            t.path()
        )))),
    }
}

fn take_formation(t: &mut Table, key: &str) -> R<Formation> {
    match t.take_str(key)?.as_str() {
        "column" => Ok(Formation::Column),
        "line" => Ok(Formation::Line),
        "skirmish" => Ok(Formation::Skirmish),
        other => Err(ScenarioError::Data(DataError::Parse(format!(
            "{}.{key} = {other:?} is not a formation",
            t.path()
        )))),
    }
}

fn take_heading(t: &mut Table, key: &str) -> R<u16> {
    let v = rng(t.take_int_in(key, 0, 359))?;
    Ok(u16::try_from(v).unwrap_or(0))
}

/// A time in `0..=hi` seconds; anything else is a [`ScenarioError::Time`].
fn take_time(t: &mut Table, key: &str, hi: i64) -> R<u32> {
    let v = t.take_int(key)?;
    if v < 0 || v > hi {
        return Err(ScenarioError::Time(format!(
            "{}.{key} = {v} outside 0..={hi}",
            t.path()
        )));
    }
    Ok(u32::try_from(v).unwrap_or(u32::MAX))
}

fn rows_or_empty(root: &mut Table, key: &str, max: usize) -> R<Vec<Table>> {
    let rows = root.take_rows_opt(key)?.unwrap_or_default();
    if rows.len() > max {
        return Err(ScenarioError::Range(format!(
            "{key}: {} rows, at most {max}",
            rows.len()
        )));
    }
    Ok(rows)
}

fn first_dup<'a>(sorted: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut prev: Option<&str> = None;
    for s in sorted {
        if prev == Some(s) {
            return Some(s);
        }
        prev = Some(s);
    }
    None
}

/// FNV-1a 64 of canonical terrain rows: each row followed by a newline.
fn rows_of(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    if v.last() == Some(&"") {
        v.pop();
    }
    v
}

fn rows_hash(rows: &[&str]) -> u64 {
    let mut h = StateHasher::new();
    for r in rows {
        h.write_bytes(r.as_bytes());
        h.write_u8(b'\n');
    }
    h.finish()
}

struct Space {
    map: MapSpec,
}

impl Space {
    fn inside(&self, x: i64, y: i64) -> bool {
        let m = &self.map;
        x >= m.origin_x_mm
            && y >= m.origin_y_mm
            && x - m.origin_x_mm < m.width_mm
            && y - m.origin_y_mm < m.height_mm
    }

    fn check(&self, grid: Option<&TerrainGrid>, x: i64, y: i64, what: &str) -> R<()> {
        if !self.inside(x, y) {
            return Err(ScenarioError::Range(format!(
                "{what}: ({x}, {y}) mm is outside the map"
            )));
        }
        if let Some(g) = grid {
            if g.blocks(Vec2Fx::new(fx_ratio(x, 1000), fx_ratio(y, 1000))) {
                return Err(ScenarioError::Range(format!(
                    "{what}: ({x}, {y}) mm is on a blocking cell"
                )));
            }
        }
        Ok(())
    }
}

fn load_terrain(
    root: &mut Table,
    tables: &Tables,
    map: &MapSpec,
    source_rows: Option<&str>,
) -> R<TerrainSpec> {
    let mut t = root.take_table("terrain")?;
    let bad = |m: String| ScenarioError::Terrain(m);
    let cell_mm = match t.take_int_opt("cell_mm")? {
        Some(v) => range_of(v, 1, 1_000_000, "terrain.cell_mm").map_err(|e| match e {
            ScenarioError::Range(m) => bad(m),
            e => e,
        })?,
        None => crate::DEFAULT_CELL_MM,
    };
    let ox = t.take_int_opt("origin_x_mm")?.unwrap_or(map.origin_x_mm);
    let oy = t.take_int_opt("origin_y_mm")?.unwrap_or(map.origin_y_mm);
    let mut legend: Vec<(char, u8)> = Vec::new();
    for mut row in t.take_rows("legend")? {
        let ch = row.take_str("ch")?;
        let mut it = ch.chars();
        let (Some(c), None) = (it.next(), it.next()) else {
            return Err(bad(format!("legend ch {ch:?} must be one character")));
        };
        let class = u8::try_from(rng(row.take_int_in("class", 0, 255))?).unwrap_or(0);
        row.finish()?;
        legend.push((c, class));
    }
    legend.sort_unstable();
    let inline = t.take_str_opt("rows")?;
    let source = t.take_str_opt("source")?;
    let source_hash = t.take_str_opt("source_hash")?;
    t.finish()?;
    let text: &str = match (&inline, &source) {
        (Some(r), None) if source_hash.is_none() => r,
        (None, Some(s)) => {
            if !source_path_ok(s) {
                return Err(bad(format!("source {s:?} must be terrain/<name>.rows")));
            }
            let rows = source_rows.ok_or_else(|| bad(format!("rows file {s} not supplied")))?;
            let want = source_hash
                .as_deref()
                .and_then(|h| (h.len() == 16).then(|| u64::from_str_radix(h, 16).ok())?)
                .ok_or_else(|| bad("source_hash must be 16 hex digits".to_owned()))?;
            if rows_hash(&rows_of(rows)) != want {
                return Err(bad(format!("rows file {s} does not match source_hash")));
            }
            rows
        }
        _ => {
            return Err(bad(
                "give either rows, or source with source_hash".to_owned()
            ))
        }
    };
    let rows = rows_of(text);
    let cells = rows
        .len()
        .saturating_mul(rows.first().map_or(0, |r| r.chars().count()));
    if cells > MAX_CELLS {
        return Err(bad(format!("{cells} cells, at most {MAX_CELLS}")));
    }
    let grid = TerrainGrid::from_rows(cell_mm, ox, oy, &legend, &rows, &tables.terrain)
        .map_err(|e| bad(e.to_string()))?;
    Ok(TerrainSpec {
        cell_mm,
        legend,
        rows_hash: rows_hash(&rows),
        grid,
    })
}

fn source_path_ok(s: &str) -> bool {
    s.strip_prefix("terrain/")
        .and_then(|n| n.strip_suffix(".rows"))
        .is_some_and(|n| id_ok(n, true))
}

/// The `[terrain] source` path a scenario text names, if any, so a caller can read the rows file
/// before calling [`Scenario::from_toml_str`].
pub(super) fn terrain_source(text: &str) -> R<Option<String>> {
    let mut root = parse_table(text)?;
    let mut t = root.take_table("terrain")?;
    Ok(t.take_str_opt("source")?)
}

fn load_unit(mut t: Table) -> R<Unit> {
    let id = take_id(&mut t, "id")?;
    let side = take_side(&mut t, "side")?;
    let parent = t.take_str("parent")?;
    let arm = match t.take_str("arm")?.as_str() {
        "infantry" => Arm::Infantry,
        "cavalry" => Arm::Cavalry,
        "battery" => Arm::Battery,
        "hq" => Arm::Hq,
        other => {
            return Err(ScenarioError::Data(DataError::Parse(format!(
                "unit {id}: arm {other:?} is not infantry, cavalry, battery or hq"
            ))))
        }
    };
    let men = uint(&mut t, "men", MAX_MEN)?;
    let guns = uint(&mut t, "guns", 1000)?;
    let training = match t.take_str("training")?.as_str() {
        "raw" => Training::Raw,
        "trained" => Training::Trained,
        "veteran" => Training::Veteran,
        other => {
            return Err(ScenarioError::Data(DataError::Parse(format!(
                "unit {id}: training {other:?} is not raw, trained or veteran"
            ))))
        }
    };
    let weapon = t.take_str("weapon")?;
    let rounds_per_man = uint(&mut t, "rounds_per_man", 1000)?;
    let commander = t.take_str("commander")?;
    if !commander.is_empty() && !id_ok(&commander, false) {
        return Err(ScenarioError::Data(DataError::Parse(format!(
            "unit {id}: commander {commander:?} is not a record id"
        ))));
    }
    let provenance = t.take_provenance()?;
    t.finish()?;
    let shape = match arm {
        Arm::Hq => men == 0 && guns == 0 && weapon.is_empty() && rounds_per_man == 0,
        Arm::Battery => men >= 1 && guns >= 1 && !weapon.is_empty(),
        Arm::Infantry | Arm::Cavalry => men >= 1 && guns == 0 && !weapon.is_empty(),
    };
    if !shape {
        return Err(ScenarioError::Range(format!(
            "unit {id}: men, guns, weapon and rounds do not fit arm {arm:?}"
        )));
    }
    Ok(Unit {
        id,
        side,
        parent,
        arm,
        men,
        guns,
        training,
        weapon,
        rounds_per_man,
        commander,
        regiment: None,
        provenance,
    })
}

/// V2, V3, V4: unique ids, resolving parents, acyclic same-side chains; then regiment ids.
fn check_hierarchy(units: &mut [Unit]) -> R<()> {
    if let Some(d) = first_dup(units.iter().map(|u| u.id.as_str())) {
        return Err(ScenarioError::Duplicate(format!("unit id {d}")));
    }
    for u in units.iter() {
        if u.parent.is_empty() {
            continue;
        }
        let Some(p) = find(units, &u.parent) else {
            return Err(ScenarioError::Reference(format!(
                "unit {} has unknown parent {}",
                u.id, u.parent
            )));
        };
        if p.side != u.side {
            return Err(ScenarioError::Cycle(format!(
                "unit {} and its parent {} are on different sides",
                u.id, u.parent
            )));
        }
    }
    for u in units.iter() {
        let mut cur = u;
        let mut steps = 0usize;
        while let Some(p) = find(units, &cur.parent) {
            cur = p;
            steps += 1;
            if steps > units.len() {
                return Err(ScenarioError::Cycle(format!(
                    "parent chain of unit {} loops",
                    u.id
                )));
            }
        }
    }
    for (n, u) in units.iter_mut().filter(|u| u.arm != Arm::Hq).enumerate() {
        u.regiment = Some(RegimentId(u16::try_from(n).unwrap_or(u16::MAX)));
    }
    Ok(())
}

fn find<'a>(units: &'a [Unit], id: &str) -> Option<&'a Unit> {
    units
        .binary_search_by(|u| u.id.as_str().cmp(id))
        .ok()
        .map(|i| &units[i])
}

fn real_unit<'a>(units: &'a [Unit], id: &str, what: &str) -> R<&'a Unit> {
    match find(units, id) {
        Some(u) if u.arm != Arm::Hq => Ok(u),
        Some(_) => Err(ScenarioError::Reference(format!(
            "{what}: {id} is a headquarters and becomes no entity"
        ))),
        None => Err(ScenarioError::Reference(format!(
            "{what}: unknown unit {id}"
        ))),
    }
}

fn load_arrival(
    mut t: Table,
    sp: &Space,
    grid: &TerrainGrid,
    dur: i64,
    units: &[Unit],
) -> R<Arrival> {
    let unit = take_id(&mut t, "unit")?;
    real_unit(units, &unit, "arrival")?;
    let at_s = take_time(&mut t, "at_s", dur)?;
    let x_mm = t.take_int("x_mm")?;
    let y_mm = t.take_int("y_mm")?;
    let heading_deg = take_heading(&mut t, "heading_deg")?;
    let formation = take_formation(&mut t, "formation")?;
    let low_s = take_time(&mut t, "low_s", MAX_S)?;
    let high_s = take_time(&mut t, "high_s", MAX_S)?;
    let provenance = t.take_provenance()?;
    t.finish()?;
    if !(low_s <= at_s && at_s <= high_s) {
        return Err(ScenarioError::Time(format!(
            "arrival of {unit}: at_s {at_s} outside the window {low_s}..={high_s}"
        )));
    }
    sp.check(Some(grid), x_mm, y_mm, &format!("arrival of {unit}"))?;
    Ok(Arrival {
        unit,
        at_s,
        x_mm,
        y_mm,
        heading_deg,
        formation,
        low_s,
        high_s,
        provenance,
    })
}

fn load_trigger(
    mut t: Table,
    sp: &Space,
    grid: &TerrainGrid,
    dur: i64,
    units: &[Unit],
) -> R<Trigger> {
    let id = take_id(&mut t, "id")?;
    let at_s = take_time(&mut t, "at_s", dur)?;
    let kind_name = t.take_str("kind")?;
    let unit = take_id(&mut t, "unit")?;
    real_unit(units, &unit, &format!("trigger {id}"))?;
    let kind = match kind_name.as_str() {
        "order" => TriggerKind::Order(match t.take_str("order")?.as_str() {
            "hold" => TriggerOrder::Hold,
            "move" => {
                let x_mm = t.take_int("x_mm")?;
                let y_mm = t.take_int("y_mm")?;
                sp.check(Some(grid), x_mm, y_mm, &format!("trigger {id}"))?;
                let name = t.take_str("gait")?;
                let gait = Gait::ALL
                    .into_iter()
                    .find(|g| g.id() == name)
                    .ok_or_else(|| {
                        ScenarioError::Data(DataError::Parse(format!(
                            "trigger {id}: unknown gait {name:?}"
                        )))
                    })?;
                TriggerOrder::Move { x_mm, y_mm, gait }
            }
            "form" => TriggerOrder::Form(take_formation(&mut t, "formation")?),
            "face" => TriggerOrder::Face(take_heading(&mut t, "heading_deg")?),
            other => {
                return Err(ScenarioError::Data(DataError::Parse(format!(
                    "trigger {id}: order {other:?} is not hold, move, form or face"
                ))))
            }
        }),
        "fire_mode" => {
            let target = take_id(&mut t, "target")?;
            real_unit(units, &target, &format!("trigger {id} target"))?;
            TriggerKind::FireMode { target }
        }
        "resupply" => TriggerKind::Resupply {
            rounds: uint(&mut t, "rounds", 1000)?,
        },
        "marker" => TriggerKind::Marker,
        other => {
            return Err(ScenarioError::Data(DataError::Parse(format!(
                "trigger {id}: kind {other:?} is not order, fire_mode, resupply or marker"
            ))))
        }
    };
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok(Trigger {
        id,
        at_s,
        unit,
        kind,
        provenance,
    })
}

fn load_weather(mut t: Table, dur: i64) -> R<WeatherRow> {
    let from_s = take_time(&mut t, "from_s", dur)?;
    let wind_x_mm_s = rng(t.take_int_in("wind_x_mm_s", -100_000, 100_000))?;
    let wind_y_mm_s = rng(t.take_int_in("wind_y_mm_s", -100_000, 100_000))?;
    let wet_pm = uint(&mut t, "wet_pm", 1000)?;
    let ground_pm = uint(&mut t, "ground_pm", 1000)?;
    let visibility_m = uint(&mut t, "visibility_m", 100_000)?;
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok(WeatherRow {
        from_s,
        wind_x_mm_s,
        wind_y_mm_s,
        wet_pm,
        ground_pm,
        visibility_m,
        provenance,
    })
}

/// V5: one arrival per unit, every real unit arrives, departures after arrivals.
fn check_times(units: &[Unit], arrivals: &[Arrival], departures: &[Departure]) -> R<()> {
    let mut by_unit: Vec<&str> = arrivals.iter().map(|a| a.unit.as_str()).collect();
    by_unit.sort_unstable();
    if let Some(d) = first_dup(by_unit.iter().copied()) {
        return Err(ScenarioError::Time(format!("two arrivals for unit {d}")));
    }
    for u in units.iter().filter(|u| u.arm != Arm::Hq) {
        if by_unit.binary_search(&u.id.as_str()).is_err() {
            return Err(ScenarioError::Time(format!("unit {} never arrives", u.id)));
        }
    }
    let mut gone: Vec<&str> = departures.iter().map(|d| d.unit.as_str()).collect();
    gone.sort_unstable();
    if let Some(d) = first_dup(gone.iter().copied()) {
        return Err(ScenarioError::Time(format!("two departures for unit {d}")));
    }
    for d in departures {
        let a = arrivals.iter().find(|a| a.unit == d.unit);
        if a.is_none_or(|a| d.at_s <= a.at_s) {
            return Err(ScenarioError::Time(format!(
                "unit {} departs at {} s, not after its arrival",
                d.unit, d.at_s
            )));
        }
    }
    Ok(())
}

/// V7: the men of the units present at `at_s` on a side are within tolerance.
fn check_strength(
    checks: &[StrengthCheck],
    units: &[Unit],
    arrivals: &[Arrival],
    departures: &[Departure],
) -> R<()> {
    for c in checks {
        let mut sum = 0u64;
        for u in units.iter().filter(|u| u.side == c.side) {
            let arrived = arrivals.iter().any(|a| a.unit == u.id && a.at_s <= c.at_s);
            let left = departures
                .iter()
                .any(|d| d.unit == u.id && d.at_s <= c.at_s);
            if arrived && !left {
                sum += u64::from(u.men);
            }
        }
        if sum.abs_diff(u64::from(c.men)) > u64::from(c.tol_men) {
            return Err(ScenarioError::Strength(format!(
                "at {} s the {:?} units hold {sum} men, expected {} +/- {}",
                c.at_s, c.side, c.men, c.tol_men
            )));
        }
    }
    Ok(())
}

pub(super) fn load(text: &str, tables: &Tables, terrain_rows: Option<&str>) -> R<Scenario> {
    let mut root = parse_table(text)?;
    let schema = root.take_str("schema")?;
    if schema != "scenario/1" {
        return Err(ScenarioError::Data(DataError::Parse(format!(
            "schema must be \"scenario/1\", found {schema:?}"
        ))));
    }
    let id = take_id(&mut root, "id")?;
    let title = root.take_str("title")?;
    let date = root.take_str("date")?;
    let clock_start_s = uint(&mut root, "clock_start_s", MAX_S)?;
    let duration_s = rng(root.take_int_in("duration_s", 1, MAX_S))?;
    if root.take_int("tick_hz")? != i64::from(TICK_HZ) {
        return Err(ScenarioError::Range(format!("tick_hz must be {TICK_HZ}")));
    }
    let family = match root.take_str_opt("family")? {
        Some(f) if id_ok(&f, true) => f,
        Some(f) => {
            return Err(ScenarioError::Data(DataError::Parse(format!(
                "family {f:?} must match [a-z0-9_-]{{1,32}}"
            ))))
        }
        None => id.clone(),
    };
    let class = root.take_int("fidelity_class")?;
    if !(0..=2).contains(&class) {
        return Err(ScenarioError::Class(format!(
            "fidelity_class = {class} must be 0, 1 or 2"
        )));
    }
    let f0_cap = uint(&mut root, "f0_cap", i64::from(u32::MAX))?;
    let agent_cap = uint(&mut root, "agent_cap", i64::from(u32::MAX))?;
    if f0_cap > agent_cap || i64::from(agent_cap) > MAX_AGENT_CAP {
        return Err(ScenarioError::Cap(format!(
            "need f0_cap {f0_cap} <= agent_cap {agent_cap} <= {MAX_AGENT_CAP}"
        )));
    }
    let mut m = root.take_table("map")?;
    let map = MapSpec {
        width_mm: rng(m.take_int_in("width_mm", 1, 12_000_000))?,
        height_mm: rng(m.take_int_in("height_mm", 1, 12_000_000))?,
        origin_x_mm: m.take_int("origin_x_mm")?,
        origin_y_mm: m.take_int("origin_y_mm")?,
    };
    m.finish()?;
    let terrain = load_terrain(&mut root, tables, &map, terrain_rows)?;
    let sp = Space { map };
    let dur = duration_s;

    let unit_rows = rows_or_empty(&mut root, "unit", MAX_UNITS)?;
    let mut units = unit_rows
        .into_iter()
        .map(load_unit)
        .collect::<R<Vec<_>>>()?;
    units.sort_by(|a, b| a.id.cmp(&b.id));
    check_hierarchy(&mut units)?;

    let mut arrivals = rows_or_empty(&mut root, "arrival", MAX_UNITS)?
        .into_iter()
        .map(|t| load_arrival(t, &sp, &terrain.grid, dur, &units))
        .collect::<R<Vec<_>>>()?;
    arrivals.sort_by(|a, b| (a.at_s, &a.unit).cmp(&(b.at_s, &b.unit)));
    let mut departures = Vec::new();
    for mut t in rows_or_empty(&mut root, "departure", MAX_UNITS)? {
        let unit = take_id(&mut t, "unit")?;
        real_unit(&units, &unit, "departure")?;
        let at_s = take_time(&mut t, "at_s", dur)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        departures.push(Departure {
            unit,
            at_s,
            provenance,
        });
    }
    departures.sort_by(|a, b| (a.at_s, &a.unit).cmp(&(b.at_s, &b.unit)));
    check_times(&units, &arrivals, &departures)?;

    let mut checks = Vec::new();
    for mut t in rows_or_empty(&mut root, "strength_check", MAX_ROWS)? {
        let at_s = take_time(&mut t, "at_s", dur)?;
        let side = take_side(&mut t, "side")?;
        let men = uint(&mut t, "men", MAX_MEN * 10)?;
        let tol_men = uint(&mut t, "tol_men", MAX_MEN * 10)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        checks.push(StrengthCheck {
            at_s,
            side,
            men,
            tol_men,
            provenance,
        });
    }
    checks.sort_by_key(|c| (c.at_s, c.side));
    if checks
        .windows(2)
        .any(|w| (w[0].at_s, w[0].side) == (w[1].at_s, w[1].side))
    {
        return Err(ScenarioError::Duplicate(
            "two strength checks at one time and side".to_owned(),
        ));
    }
    check_strength(&checks, &units, &arrivals, &departures)?;

    let mut objectives = Vec::new();
    for mut t in rows_or_empty(&mut root, "objective", MAX_ROWS)? {
        let id = take_id(&mut t, "id")?;
        let side = take_side(&mut t, "side")?;
        let x_mm = t.take_int("x_mm")?;
        let y_mm = t.take_int("y_mm")?;
        sp.check(None, x_mm, y_mm, &format!("objective {id}"))?;
        let radius_mm = rng(t.take_int_in("radius_mm", 1, 12_000_000))?;
        let points = uint(&mut t, "points", 1_000_000)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        objectives.push(Objective {
            id,
            side,
            x_mm,
            y_mm,
            radius_mm: u32::try_from(radius_mm).unwrap_or(u32::MAX),
            points,
            provenance,
        });
    }
    objectives.sort_by(|a, b| a.id.cmp(&b.id));
    if let Some(d) = first_dup(objectives.iter().map(|o| o.id.as_str())) {
        return Err(ScenarioError::Duplicate(format!("objective id {d}")));
    }

    let mut triggers = rows_or_empty(&mut root, "trigger", MAX_ROWS)?
        .into_iter()
        .map(|t| load_trigger(t, &sp, &terrain.grid, dur, &units))
        .collect::<R<Vec<_>>>()?;
    let mut tids: Vec<&str> = triggers.iter().map(|t| t.id.as_str()).collect();
    tids.sort_unstable();
    if let Some(d) = first_dup(tids.iter().copied()) {
        return Err(ScenarioError::Duplicate(format!("trigger id {d}")));
    }
    triggers.sort_by(|a, b| (a.at_s, &a.id).cmp(&(b.at_s, &b.id)));

    let mut weather = rows_or_empty(&mut root, "weather", MAX_ROWS)?
        .into_iter()
        .map(|t| load_weather(t, dur))
        .collect::<R<Vec<_>>>()?;
    weather.sort_by_key(|w| w.from_s);
    if weather.windows(2).any(|w| w[0].from_s == w[1].from_s) {
        return Err(ScenarioError::Duplicate(
            "two weather rows at one time".to_owned(),
        ));
    }
    if weather.first().is_some_and(|w| w.from_s != 0) {
        return Err(ScenarioError::Time(
            "the first weather row must have from_s = 0".to_owned(),
        ));
    }
    root.finish()?;

    Ok(Scenario {
        id,
        title,
        date,
        clock_start_s,
        duration_s: u32::try_from(duration_s).unwrap_or(u32::MAX),
        tick_hz: TICK_HZ,
        family,
        fidelity_class: u8::try_from(class).unwrap_or(2),
        f0_cap,
        agent_cap,
        map: sp.map,
        terrain,
        units,
        arrivals,
        departures,
        strength_checks: checks,
        objectives,
        triggers,
        weather,
    })
}
