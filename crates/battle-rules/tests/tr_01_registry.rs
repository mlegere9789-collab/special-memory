//! TR-01: the constants of `battle_rules::registry` are pairwise disjoint inside each namespace and
//! equal the tables of `docs/specs/REGISTRY.md` (sections 2 to 6), which are parsed here.

use battle_rules::registry::{self as reg, EnumKind, Enumeration, Span};
use battle_rules::{
    soldier_stream, Order, F_DISABLED, F_GUN, F_KNEEL, F_PRONE, F_WOUNDED, STREAM_BATTLE_SETUP,
    STREAM_SOLDIER_BASE,
};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------------------------------
// Markdown table reading, tolerant of spacing, alignment rows and optional outer pipes.
// ---------------------------------------------------------------------------------------------

fn registry_md() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/specs/REGISTRY.md");
    std::fs::read_to_string(path).expect("REGISTRY.md is readable")
}

/// The rows (cells trimmed) of the first table under the heading numbered `num` (for example
/// `"2.1"` or `"5"`), without header and alignment rows. `nth` picks a later table of the section.
fn table(md: &str, num: &str, nth: usize) -> Vec<Vec<String>> {
    let mut in_section = false;
    let mut tables: Vec<Vec<Vec<String>>> = Vec::new();
    let mut current: Option<Vec<Vec<String>>> = None;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            if let Some(done) = current.take() {
                tables.push(done);
            }
            if in_section && tables.len() > nth {
                break;
            }
            let word = t
                .trim_start_matches('#')
                .split_whitespace()
                .next()
                .unwrap_or("");
            in_section = word.trim_end_matches('.') == num;
            continue;
        }
        if !in_section {
            continue;
        }
        if t.starts_with('|') {
            let cells: Vec<String> = t
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect();
            let rows = current.get_or_insert_with(Vec::new);
            rows.push(cells);
        } else if let Some(done) = current.take() {
            tables.push(done);
        }
    }
    if let Some(done) = current.take() {
        tables.push(done);
    }
    let rows = tables
        .into_iter()
        .nth(nth)
        .unwrap_or_else(|| panic!("section {num} has no table {nth}"));
    rows.into_iter()
        .skip(1) // header
        .filter(|r| !r[0].chars().all(|c| matches!(c, '-' | ':' | ' ')))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Shape {
    Single,
    Range(u64),
    Base,
    Open,
}

#[derive(Debug)]
struct Item {
    lo: u64,
    shape: Shape,
}

/// Leading number of `s` (hex with underscores, or decimal) and the remainder.
fn leading_number(s: &str) -> Option<(u64, &str)> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x") {
        let end = h
            .find(|c: char| !(c.is_ascii_hexdigit() || c == '_'))
            .unwrap_or(h.len());
        let v = u64::from_str_radix(&h[..end].replace('_', ""), 16).ok()?;
        Some((v, &h[end..]))
    } else {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        if end == 0 {
            return None;
        }
        Some((s[..end].parse().ok()?, &s[end..]))
    }
}

/// The numeric code spans of a first-column cell, with `a to b` and `a..b` merged into ranges.
fn items(cell: &str) -> Vec<Item> {
    let parts: Vec<&str> = cell.split('`').collect();
    let mut out: Vec<Item> = Vec::new();
    let mut merge_next = false;
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            merge_next = !out.is_empty() && part.trim() == "to";
            continue;
        }
        let Some((lo, rest)) = leading_number(part) else {
            merge_next = false;
            continue;
        };
        let rest = rest.trim();
        if merge_next {
            let prev = out.last_mut().expect("a range start");
            assert_eq!(prev.shape, Shape::Single, "range start in {cell:?}");
            prev.shape = Shape::Range(lo);
            merge_next = false;
            continue;
        }
        let shape = if let Some(after) = rest.strip_prefix("..") {
            match leading_number(after) {
                Some((hi, _)) => Shape::Range(hi),
                None => Shape::Open,
            }
        } else if rest.starts_with('+') {
            Shape::Base
        } else {
            Shape::Single
        };
        out.push(Item { lo, shape });
    }
    out
}

/// Cross-check one namespace table against its registry spans. `max` is the top of the id type.
fn check_table(
    rows: &[Vec<String>],
    owner_col: usize,
    spans: &[Span],
    exact_single: bool,
    max: u32,
) {
    let mut seen = 0;
    for row in rows {
        let owner_cell = row[owner_col].to_lowercase();
        for item in items(&row[0]) {
            seen += 1;
            let lo = u32::try_from(item.lo).expect("id fits u32");
            let span = spans
                .iter()
                .find(|s| s.lo == lo)
                .unwrap_or_else(|| panic!("REGISTRY row {:?} has no constant at {lo:#x}", row[0]));
            match item.shape {
                Shape::Single if exact_single => assert_eq!(span.hi, lo, "{}", span.name),
                Shape::Range(hi) => assert_eq!(u64::from(span.hi), hi, "{}", span.name),
                Shape::Open => assert_eq!(span.hi, max, "{}", span.name),
                _ => assert!(span.hi >= lo, "{}", span.name),
            }
            if owner_cell == "reserved" {
                assert!(span.reserved, "{} must be reserved", span.name);
            } else {
                assert!(!span.reserved, "{} must not be reserved", span.name);
                assert!(
                    owner_cell.contains(&span.owner.to_lowercase()),
                    "{}: owner {:?} not in {:?}",
                    span.name,
                    span.owner,
                    owner_cell
                );
            }
        }
    }
    assert_eq!(
        seen,
        spans.len(),
        "REGISTRY and registry.rs list different numbers of ids"
    );
}

fn assert_disjoint(namespace: &str, spans: &[Span]) {
    let mut sorted: Vec<&Span> = spans.iter().collect();
    sorted.sort_by_key(|s| s.lo);
    for s in &sorted {
        assert!(s.lo <= s.hi, "{namespace}: {} is an empty range", s.name);
    }
    for w in sorted.windows(2) {
        assert!(
            w[0].hi < w[1].lo,
            "{namespace}: {} ({:#x}..={:#x}) overlaps {} ({:#x}..={:#x})",
            w[0].name,
            w[0].lo,
            w[0].hi,
            w[1].name,
            w[1].lo,
            w[1].hi
        );
    }
    let names: BTreeSet<&str> = spans.iter().map(|s| s.name).collect();
    assert_eq!(names.len(), spans.len(), "{namespace}: duplicate names");
}

// ---------------------------------------------------------------------------------------------
// Actors, tags, streams
// ---------------------------------------------------------------------------------------------

#[test]
fn actors_are_disjoint_and_match_section_2_1() {
    assert_disjoint("actor", reg::ACTOR_SPANS);
    let md = registry_md();
    check_table(&table(&md, "2.1", 0), 1, reg::ACTOR_SPANS, true, 0xFF);
    assert_eq!(reg::ACTOR_SEAT_1, 0x01);
    assert_eq!(reg::seat_actor(1), 0x01);
    assert_eq!(reg::advisor_actor(1), 0x61);
    assert_eq!(reg::ai_actor(1), 0x81);
    assert_eq!(
        [
            reg::ACTOR_HISTORY_SCRIPT,
            reg::ACTOR_SCENARIO_SCRIPT,
            reg::ACTOR_SYSTEM
        ],
        [0xFD, 0xFE, 0xFF]
    );
    assert_eq!(
        [
            reg::SEQ_SCENARIO_BIND,
            reg::SEQ_DIFFICULTY,
            reg::SEQ_AI_CONFIG_BASE
        ],
        [0, 1, 2]
    );
}

#[test]
fn tags_are_disjoint_and_match_section_2_2() {
    assert_disjoint("tag", reg::TAG_SPANS);
    let md = registry_md();
    check_table(&table(&md, "2.2", 0), 1, reg::TAG_SPANS, true, 0xFF);
}

#[test]
fn order_wire_tags_keep_their_values() {
    assert_eq!(
        [reg::TAG_HOLD, reg::TAG_MOVE, reg::TAG_FORM, reg::TAG_FACE],
        [1, 2, 3, 4]
    );
    assert_eq!(reg::TAG_SET_FIRE_MODE, 5);
    assert_eq!(Order::Hold.tag(), reg::TAG_HOLD);
    assert_eq!(
        Order::Form(battle_rules::Formation::Line).tag(),
        reg::TAG_FORM
    );
}

#[test]
fn streams_are_disjoint_and_match_section_3() {
    assert_disjoint("stream", reg::STREAM_SPANS);
    let md = registry_md();
    check_table(&table(&md, "3", 0), 1, reg::STREAM_SPANS, false, u32::MAX);
}

#[test]
fn stream_ids_keep_their_values_and_sit_in_their_blocks() {
    assert_eq!(STREAM_BATTLE_SETUP.get(), 0x0B01);
    assert_eq!(reg::STREAM_BATTLE_SETUP, STREAM_BATTLE_SETUP);
    assert_eq!(STREAM_SOLDIER_BASE, 0x4000_0000);
    assert_eq!(reg::STREAM_SOLDIER_BASE, STREAM_SOLDIER_BASE);
    assert_eq!(soldier_stream(7).get(), 0x4000_0007);
    assert_eq!(reg::STREAM_OBSERVE.get(), 0x0C02);
    assert_eq!(
        [
            reg::STREAM_TOY_PLACEMENT.get(),
            reg::STREAM_TOY_WANDER.get(),
            reg::STREAM_TOY_SKIRMISH.get()
        ],
        [1, 2, 3]
    );
    let bases = [
        (reg::STREAM_SOLDIER_BASE, "STREAM_SOLDIER_BASE"),
        (reg::STREAM_MORALE_BASE, "STREAM_MORALE_BASE"),
        (reg::STREAM_MESSAGE_BASE, "STREAM_MESSAGE_BASE"),
        (reg::STREAM_F1_COUNT_BASE, "STREAM_F1_COUNT_BASE"),
        (reg::STREAM_AI_BASE, "STREAM_AI_BASE"),
        (reg::STREAM_F1_VICTIM_BASE, "STREAM_F1_VICTIM_BASE"),
        (reg::STREAM_MATERIALISED_BASE, "STREAM_MATERIALISED_BASE"),
    ];
    for (base, name) in bases {
        let span = reg::STREAM_SPANS
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name} is not in STREAM_SPANS"));
        assert_eq!(span.lo, base, "{name}");
        assert!(!span.reserved);
    }
    assert_eq!(reg::ai_stream(1, 3).get(), 0x5400_0000 + 16 + 3);
    assert_eq!(reg::morale_stream(5).get(), 0x5000_0005);
}

// ---------------------------------------------------------------------------------------------
// Flags and other bit fields
// ---------------------------------------------------------------------------------------------

#[test]
fn flag_bits_are_disjoint_and_match_section_4_1() {
    assert_disjoint("flag", reg::FLAG_SPANS);
    let md = registry_md();
    let rows = table(&md, "4.1", 0);
    assert_eq!(rows.len(), reg::FLAG_SPANS.len());
    for (row, span) in rows.iter().zip(reg::FLAG_SPANS) {
        let bits: Vec<u32> = row[0]
            .split(" to ")
            .map(|p| p.trim().parse().expect("bit number"))
            .collect();
        assert_eq!(bits[0], span.lo, "{}", span.name);
        assert_eq!(*bits.last().unwrap(), span.hi, "{}", span.name);
        if span.reserved {
            assert_eq!(row[2].to_lowercase(), "registry");
        } else {
            assert_eq!(row[1].trim_matches('`'), span.name);
            assert!(row[2].contains(span.owner));
        }
    }
    // Named masks equal their bit positions and the previous values.
    for (mask, name) in [
        (F_KNEEL, "F_KNEEL"),
        (F_PRONE, "F_PRONE"),
        (F_DISABLED, "F_DISABLED"),
        (F_WOUNDED, "F_WOUNDED"),
        (F_GUN, "F_GUN"),
        (reg::F_LOOKBACK, "F_LOOKBACK"),
        (reg::F_SKULK, "F_SKULK"),
    ] {
        let span = reg::FLAG_SPANS.iter().find(|s| s.name == name).unwrap();
        assert_eq!(mask, 1u16 << span.lo, "{name}");
    }
    assert_eq!(
        [F_KNEEL, F_PRONE, F_DISABLED, F_WOUNDED, F_GUN],
        [1, 2, 4, 8, 16]
    );
    assert_eq!((reg::F_LOOKBACK, reg::F_SKULK), (32, 64));
}

#[test]
fn bit_fields_and_enumerations_match_section_4_2() {
    let md = registry_md();
    let rows = table(&md, "4.2", 0);
    for e in reg::ENUMERATIONS {
        check_enumeration(e, &rows);
    }
}

fn check_enumeration(e: &Enumeration, rows: &[Vec<String>]) {
    let row = rows
        .iter()
        .find(|r| r[0].trim_matches('`') == e.doc_name)
        .unwrap_or_else(|| panic!("REGISTRY 4.2 has no row {}", e.doc_name));
    let cell = row[2].to_lowercase();
    // Values must be pairwise distinct, and masks must be single bits.
    let values: BTreeSet<u32> = e.entries.iter().map(|n| n.value).collect();
    assert_eq!(
        values.len(),
        e.entries.len(),
        "{}: duplicate values",
        e.doc_name
    );
    if e.kind == EnumKind::Mask {
        assert!(values.iter().all(|v| v.is_power_of_two()), "{}", e.doc_name);
    }
    // Every named entry appears in the cell, and the numbers in the cell are exactly the values.
    for n in e.entries {
        assert!(
            cell.contains(&n.name.to_lowercase()),
            "{}: {} missing in {cell:?}",
            e.doc_name,
            n.name
        );
    }
    let mut doc_numbers = BTreeSet::new();
    let mut run = String::new();
    let mut prev = ' ';
    for c in cell.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit()
            && (run.is_empty() && !(prev.is_alphanumeric() || prev == '_') || !run.is_empty())
        {
            run.push(c);
        } else {
            if !run.is_empty() {
                doc_numbers.insert(run.parse::<u32>().unwrap());
                run.clear();
            }
        }
        prev = c;
    }
    // Entries whose number is zero ("0 in a dispatch means automatic") are optional prose.
    let expected: BTreeSet<u32> = e.entries.iter().map(|n| n.value).collect();
    let missing: Vec<_> = expected.difference(&doc_numbers).collect();
    assert!(
        missing.is_empty(),
        "{}: values {missing:?} not in {cell:?}",
        e.doc_name
    );
    let extra: Vec<_> = doc_numbers
        .difference(&expected)
        .filter(|v| **v != 0)
        .collect();
    assert!(
        extra.is_empty(),
        "{}: unexpected numbers {extra:?} in {cell:?}",
        e.doc_name
    );
}

#[test]
fn enumeration_constants_keep_their_documented_values() {
    assert_eq!(
        [
            reg::T_AGGRESSIVE,
            reg::T_CAUTIOUS,
            reg::T_HIGH_DISC,
            reg::T_LOW_DISC
        ],
        [1, 2, 4, 8]
    );
    assert_eq!(
        [
            reg::MEDIUM_COURIER,
            reg::MEDIUM_FLAGS,
            reg::MEDIUM_TELEGRAPH
        ],
        [1, 2, 3]
    );
    assert_eq!([reg::LATITUDE_STRICT, reg::LATITUDE_DISCRETIONARY], [0, 1]);
    assert_eq!(reg::LEVEL_REGIMENT, 4);
    assert_eq!(reg::OPT_INSTANT_ORDERS, 2);
    assert_eq!(reg::ASSIST_HISTORICAL_RESTRICTIONS, 16);
    assert_eq!(reg::TILE_STEEP, 16);
}

// ---------------------------------------------------------------------------------------------
// Hash blocks, magics, tick steps
// ---------------------------------------------------------------------------------------------

#[test]
fn hash_blocks_keep_order_and_match_section_5() {
    let md = registry_md();
    let rows = table(&md, "5", 0);
    assert_eq!(rows.len(), reg::HASH_BLOCKS.len());
    let mut magics = BTreeSet::new();
    for (i, (row, b)) in rows.iter().zip(reg::HASH_BLOCKS).enumerate() {
        assert_eq!(row[0].parse::<usize>().unwrap(), i + 1);
        assert_eq!(usize::from(b.order), i + 1, "{}", b.name);
        assert!(row[2].contains(b.owner), "{}: owner", b.name);
        match (b.magic, items(&row[4]).first()) {
            (None, None) => {}
            (Some(m), Some(it)) => {
                assert_eq!(u64::from(m), it.lo, "{}", b.name);
                assert!(magics.insert(m), "{}: magic repeated", b.name);
            }
            (a, b2) => panic!("{}: magic {a:?} versus REGISTRY {b2:?}", b.name),
        }
        // Optional blocks (written only when an option is Some) carry a magic; fixed ones do not.
        assert_eq!(
            b.magic.is_some(),
            row[3].contains("is `Some`"),
            "{}",
            b.name
        );
    }
    assert_eq!(reg::HASH_BLOCKS[0].name, "world");
    assert_eq!(reg::HASH_BLOCKS[8].name, "environment");
}

#[test]
fn magics_are_disjoint_ascii_labels_and_match_section_5() {
    let set: BTreeSet<u32> = reg::MAGICS.iter().map(|m| m.value).collect();
    assert_eq!(set.len(), reg::MAGICS.len(), "magics repeat");
    for m in reg::MAGICS {
        assert_eq!(&m.value.to_be_bytes()[..], m.label.as_bytes(), "{}", m.name);
    }
    for b in reg::HASH_BLOCKS.iter().filter_map(|b| b.magic) {
        assert!(set.contains(&b));
    }
    // Every eight-digit hex code span of section 5 is a registered magic, and vice versa.
    let md = registry_md();
    let mut in5 = false;
    let mut found = BTreeSet::new();
    for line in md.lines() {
        if line.starts_with('#') {
            in5 = line.trim_start_matches('#').trim().starts_with("5.");
        }
        if in5 {
            for (i, part) in line.split('`').enumerate() {
                if i % 2 == 1 {
                    if let Some((v, "")) = leading_number(part) {
                        if part.trim().starts_with("0x") && part.trim().len() == 11 {
                            found.insert(u32::try_from(v).unwrap());
                        }
                    }
                }
            }
        }
    }
    assert_eq!(found, set);
}

#[test]
fn tick_steps_keep_their_order_and_match_section_6() {
    let md = registry_md();
    let mut doc_ids: Vec<String> = Vec::new();
    for sec in ["6.1", "6.2", "6.3"] {
        for row in table(&md, sec, 0) {
            doc_ids.push(row[0].trim_matches('`').to_string());
        }
    }
    // The shell steps before the step come first, the step proper in the middle, then after.
    let expect: Vec<&str> = reg::TICK_STEPS.iter().map(|s| s.id).collect();
    let doc_refs: Vec<&str> = doc_ids.iter().map(String::as_str).collect();
    // Section 6.1 and 6.3 are listed around 6.2 in the file; the registry orders PR, TK, PS.
    assert_eq!(doc_refs, expect);
    let unique: BTreeSet<&str> = expect.iter().copied().collect();
    assert_eq!(unique.len(), expect.len());
    assert_eq!(reg::TICK_STEPS.len(), 2 + 15 + 3);
    assert_eq!(reg::TICK_STEPS[2].id, "TK-01");
    assert_eq!(reg::TICK_STEPS[16].id, "TK-15");
}
