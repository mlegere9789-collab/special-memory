//! Shared identifiers of `docs/specs/REGISTRY.md` sections 2 to 6 (ticket T-026).
//!
//! This module is the single place where replay input actors and payload tags, random stream ids,
//! unit flag bits, hash block ids and magics, and the ordered tick-step list are written as
//! constants. Every constant names its owning spec in its documentation. A ticket that adds a tag,
//! actor, stream, flag or magic adds its row to REGISTRY first and its constant here in the same
//! change. Test TR-01 (`tests/tr_01_registry.rs`) proves each namespace is pairwise disjoint and
//! equal to the REGISTRY tables.
//!
//! Ranges (`Span`) are inclusive. A block such as `0x5000_0000 + regiment` is listed with the
//! whole 24-bit (or 28-bit) block it owns so that blocks cannot grow into each other.

use core_math::StreamId;

/// An inclusive range of ids inside one namespace, as listed in a REGISTRY table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Constant name, or a descriptive name for a reserved range.
    pub name: &'static str,
    /// First id of the range.
    pub lo: u32,
    /// Last id of the range (inclusive).
    pub hi: u32,
    /// Owning spec (`SPEC-nnn`), `system` or `registry`; `reserved` for a free range.
    pub owner: &'static str,
    /// `true` for a range REGISTRY marks reserved and not yet used.
    pub reserved: bool,
}

impl Span {
    /// An allocated range.
    pub const fn owned(name: &'static str, owner: &'static str, lo: u32, hi: u32) -> Span {
        Span {
            name,
            lo,
            hi,
            owner,
            reserved: false,
        }
    }

    /// A reserved range.
    pub const fn reserved(name: &'static str, lo: u32, hi: u32) -> Span {
        Span {
            name,
            lo,
            hi,
            owner: "reserved",
            reserved: true,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Section 2.1: replay input actors (`u8`)
// ---------------------------------------------------------------------------------------------

/// Human seat of side 0 (`actor = side`). Owner: SPEC-007 (REGISTRY 2.1).
pub const ACTOR_SEAT_0: u8 = 0x00;
/// Human seat of side 1. Owner: SPEC-007 (REGISTRY 2.1).
pub const ACTOR_SEAT_1: u8 = 0x01;
/// Advisor reply of side `s` is `ACTOR_ADVISOR_BASE + s`. Owner: SPEC-010 (`ADVISOR_ACTOR_BASE`).
pub const ACTOR_ADVISOR_BASE: u8 = 0x60;
/// AI seat of side `s` is `ACTOR_AI_BASE + s`. Owner: SPEC-007; SPEC-010 `AI_ACTOR_BASE`.
pub const ACTOR_AI_BASE: u8 = 0x80;
/// Historical script (compiled arrivals, orders, triggers). Owner: SPEC-011.
pub const ACTOR_HISTORY_SCRIPT: u8 = 0xFD;
/// Scenario script events (payload tag `0x13`). Owner: SPEC-007.
pub const ACTOR_SCENARIO_SCRIPT: u8 = 0xFE;
/// System actor. Seq 0 scenario binding (SPEC-008), 1 difficulty (SPEC-015), 2 and 3 AI
/// configuration (SPEC-010). Owner: system (REGISTRY 2.1).
pub const ACTOR_SYSTEM: u8 = 0xFF;

/// Sequence number of the scenario binding at actor `0xFF`. Owner: SPEC-008.
pub const SEQ_SCENARIO_BIND: u16 = 0;
/// Sequence number of the difficulty record at actor `0xFF`. Owner: SPEC-015.
pub const SEQ_DIFFICULTY: u16 = 1;
/// Sequence number of the AI configuration of side 0 at actor `0xFF` (side 1 is `+ 1`).
/// Owner: SPEC-010.
pub const SEQ_AI_CONFIG_BASE: u16 = 2;

/// The human seat actor of `side` (0 or 1). Owner: SPEC-007.
pub const fn seat_actor(side: u8) -> u8 {
    ACTOR_SEAT_0.wrapping_add(side)
}

/// The advisor-reply actor of `side`. Owner: SPEC-010.
pub const fn advisor_actor(side: u8) -> u8 {
    ACTOR_ADVISOR_BASE.wrapping_add(side)
}

/// The AI seat actor of `side`. Owner: SPEC-007, SPEC-010.
pub const fn ai_actor(side: u8) -> u8 {
    ACTOR_AI_BASE.wrapping_add(side)
}

/// Every actor id and range of REGISTRY 2.1, ascending.
pub const ACTOR_SPANS: &[Span] = &[
    Span::owned(
        "ACTOR_SEAT_0",
        "SPEC-007",
        ACTOR_SEAT_0 as u32,
        ACTOR_SEAT_0 as u32,
    ),
    Span::owned(
        "ACTOR_SEAT_1",
        "SPEC-007",
        ACTOR_SEAT_1 as u32,
        ACTOR_SEAT_1 as u32,
    ),
    Span::reserved("ACTOR_RESERVED_SEATS_LOW", 0x02, 0x3F),
    Span::reserved("ACTOR_RESERVED_SEATS_HIGH", 0x40, 0x5F),
    Span::owned(
        "ACTOR_ADVISOR_0",
        "SPEC-010",
        ACTOR_ADVISOR_BASE as u32,
        ACTOR_ADVISOR_BASE as u32,
    ),
    Span::owned(
        "ACTOR_ADVISOR_1",
        "SPEC-010",
        ACTOR_ADVISOR_BASE as u32 + 1,
        ACTOR_ADVISOR_BASE as u32 + 1,
    ),
    Span::owned(
        "ACTOR_AI_0",
        "SPEC-007",
        ACTOR_AI_BASE as u32,
        ACTOR_AI_BASE as u32,
    ),
    Span::owned(
        "ACTOR_AI_1",
        "SPEC-007",
        ACTOR_AI_BASE as u32 + 1,
        ACTOR_AI_BASE as u32 + 1,
    ),
    Span::owned(
        "ACTOR_HISTORY_SCRIPT",
        "SPEC-011",
        ACTOR_HISTORY_SCRIPT as u32,
        ACTOR_HISTORY_SCRIPT as u32,
    ),
    Span::owned(
        "ACTOR_SCENARIO_SCRIPT",
        "SPEC-007",
        ACTOR_SCENARIO_SCRIPT as u32,
        ACTOR_SCENARIO_SCRIPT as u32,
    ),
    Span::owned(
        "ACTOR_SYSTEM",
        "system",
        ACTOR_SYSTEM as u32,
        ACTOR_SYSTEM as u32,
    ),
];

// ---------------------------------------------------------------------------------------------
// Section 2.2: replay payload tags (`u8`, first payload byte)
// ---------------------------------------------------------------------------------------------

/// Regiment order Hold, `u8 tag, u16 regiment`. Owner: SPEC-004.
pub const TAG_HOLD: u8 = 0x01;
/// Regiment order Move, `u8 tag, u16 regiment, i64 x, i64 y, u8 gait`. Owner: SPEC-004.
pub const TAG_MOVE: u8 = 0x02;
/// Regiment order Form, `u8 tag, u16 regiment, u8 formation`. Owner: SPEC-004.
pub const TAG_FORM: u8 = 0x03;
/// Regiment order Face, `u8 tag, u16 regiment, u32 angle`. Owner: SPEC-004.
pub const TAG_FACE: u8 = 0x04;
/// `set_fire_mode` payload. Owner: SPEC-005.
pub const TAG_SET_FIRE_MODE: u8 = 0x05;
/// Dispatch (`u8 0x10, u8 medium, u8 latitude, u16 from`, then `encode_order` bytes). Owner:
/// SPEC-007.
pub const TAG_DISPATCH: u8 = 0x10;
/// Reserved by SPEC-007 (no payload defined). Owner: SPEC-007.
pub const TAG_COMMAND_RESERVED: u8 = 0x11;
/// Options (`omniscient_view`, `instant_orders`). Owner: SPEC-007.
pub const TAG_OPTIONS: u8 = 0x12;
/// Script (officer down or up, link cut or restored). Owner: SPEC-007.
pub const TAG_SCRIPT: u8 = 0x13;
/// ScenarioBind, `u8 0x20, u64 scenario_hash`. Owner: SPEC-008.
pub const TAG_SCENARIO_BIND: u8 = 0x20;
/// Arrive, `unit_index u16, x i64, y i64, heading u32, formation u8, count u16`. Owner: SPEC-011.
pub const TAG_ARRIVE: u8 = 0x30;
/// Trigger, `index u16`. Owner: SPEC-011.
pub const TAG_TRIGGER: u8 = 0x31;
/// Difficulty record (`encode_difficulty`). Owner: SPEC-015.
pub const TAG_DIFFICULTY: u8 = 0x40;
/// Advisor reply, `u8 0xA0, u32 request_id, u8 n, ...`. Owner: SPEC-010.
pub const TAG_ADVISOR_REPLY: u8 = 0xA0;
/// AiConfig, `u8 0xA1, u8 side, u8 level, u8 advisor`, actor `0xFF`, seq `2 + side`. Owner:
/// SPEC-010.
pub const TAG_AI_CONFIG: u8 = 0xA1;

/// Every payload tag and range of REGISTRY 2.2, ascending.
pub const TAG_SPANS: &[Span] = &[
    Span::owned("TAG_HOLD", "SPEC-004", TAG_HOLD as u32, TAG_HOLD as u32),
    Span::owned("TAG_MOVE", "SPEC-004", TAG_MOVE as u32, TAG_MOVE as u32),
    Span::owned("TAG_FORM", "SPEC-004", TAG_FORM as u32, TAG_FORM as u32),
    Span::owned("TAG_FACE", "SPEC-004", TAG_FACE as u32, TAG_FACE as u32),
    Span::owned(
        "TAG_SET_FIRE_MODE",
        "SPEC-005",
        TAG_SET_FIRE_MODE as u32,
        TAG_SET_FIRE_MODE as u32,
    ),
    Span::reserved("TAG_RESERVED_REGIMENT", 0x06, 0x0F),
    Span::owned(
        "TAG_DISPATCH",
        "SPEC-007",
        TAG_DISPATCH as u32,
        TAG_DISPATCH as u32,
    ),
    Span::owned(
        "TAG_COMMAND_RESERVED",
        "SPEC-007",
        TAG_COMMAND_RESERVED as u32,
        TAG_COMMAND_RESERVED as u32,
    ),
    Span::owned(
        "TAG_OPTIONS",
        "SPEC-007",
        TAG_OPTIONS as u32,
        TAG_OPTIONS as u32,
    ),
    Span::owned(
        "TAG_SCRIPT",
        "SPEC-007",
        TAG_SCRIPT as u32,
        TAG_SCRIPT as u32,
    ),
    Span::reserved("TAG_RESERVED_SPEC007", 0x14, 0x1F),
    Span::owned(
        "TAG_SCENARIO_BIND",
        "SPEC-008",
        TAG_SCENARIO_BIND as u32,
        TAG_SCENARIO_BIND as u32,
    ),
    Span::reserved("TAG_RESERVED_SPEC008", 0x21, 0x2F),
    Span::owned(
        "TAG_ARRIVE",
        "SPEC-011",
        TAG_ARRIVE as u32,
        TAG_ARRIVE as u32,
    ),
    Span::owned(
        "TAG_TRIGGER",
        "SPEC-011",
        TAG_TRIGGER as u32,
        TAG_TRIGGER as u32,
    ),
    Span::reserved("TAG_RESERVED_SPEC011", 0x32, 0x3F),
    Span::owned(
        "TAG_DIFFICULTY",
        "SPEC-015",
        TAG_DIFFICULTY as u32,
        TAG_DIFFICULTY as u32,
    ),
    Span::reserved("TAG_RESERVED_SPEC015", 0x41, 0x4F),
    Span::reserved("TAG_RESERVED_SPEC006", 0x50, 0x5F),
    Span::reserved("TAG_RESERVED_MID", 0x60, 0x9F),
    Span::owned(
        "TAG_ADVISOR_REPLY",
        "SPEC-010",
        TAG_ADVISOR_REPLY as u32,
        TAG_ADVISOR_REPLY as u32,
    ),
    Span::owned(
        "TAG_AI_CONFIG",
        "SPEC-010",
        TAG_AI_CONFIG as u32,
        TAG_AI_CONFIG as u32,
    ),
    Span::reserved("TAG_RESERVED_A2", 0xA2, 0xAF),
    Span::reserved("TAG_RESERVED_HIGH", 0xB0, 0xFF),
];

// ---------------------------------------------------------------------------------------------
// Section 3: random streams (`StreamId(u32)`)
// ---------------------------------------------------------------------------------------------

/// Toy scenario `Placement` stream. Owner: SPEC-003.
pub const STREAM_TOY_PLACEMENT: StreamId = StreamId(1);
/// Toy scenario `Wander` stream. Owner: SPEC-003.
pub const STREAM_TOY_WANDER: StreamId = StreamId(2);
/// Toy scenario `Skirmish` stream. Owner: SPEC-003.
pub const STREAM_TOY_SKIRMISH: StreamId = StreamId(3);
/// Scenario jitter only; block `0x0B00..0x0BFF` is SPEC-004's. Owner: SPEC-004.
pub const STREAM_BATTLE_SETUP: StreamId = StreamId(0x0B01);
/// Observation noise; block `0x0C00..0x0CFF` is SPEC-007's. Owner: SPEC-007.
pub const STREAM_OBSERVE: StreamId = StreamId(0x0C02);
/// First per-soldier stream: soldier `slot_index` uses `STREAM_SOLDIER_BASE + slot_index`, block
/// `0x4000_0000..=0x4FFF_FFFF`. Owner: SPEC-005.
pub const STREAM_SOLDIER_BASE: u32 = 0x4000_0000;
/// Morale stream of regiment `r` is `STREAM_MORALE_BASE + r`. Owner: SPEC-006.
pub const STREAM_MORALE_BASE: u32 = 0x5000_0000;
/// One stream per command message, `STREAM_MESSAGE_BASE + id` (id below 2^24). Owner: SPEC-007.
pub const STREAM_MESSAGE_BASE: u32 = 0x5100_0000;
/// F1 count stream of regiment `r` is `STREAM_F1_COUNT_BASE + r`. Owner: SPEC-008.
pub const STREAM_F1_COUNT_BASE: u32 = 0x5200_0000;
/// AI streams `STREAM_AI_BASE + side * 16 + layer` (layer 0 OL, 1 TL, 2 estimate noise, 3 belief
/// and forecast seeds, 4 spare). Owner: SPEC-010.
pub const STREAM_AI_BASE: u32 = 0x5400_0000;
/// F1 victim stream of regiment `r` is `STREAM_F1_VICTIM_BASE + r`. Owner: SPEC-008.
pub const STREAM_F1_VICTIM_BASE: u32 = 0x5800_0000;
/// Materialised soldiers `STREAM_MATERIALISED_BASE + serial` (SPEC-005 amendment A7), block
/// `0x8000_0000..=0x8FFF_FFFF`. Owner: SPEC-008.
pub const STREAM_MATERIALISED_BASE: u32 = 0x8000_0000;

/// The morale stream of `regiment`. Owner: SPEC-006.
pub const fn morale_stream(regiment: u16) -> StreamId {
    StreamId(STREAM_MORALE_BASE + regiment as u32)
}

/// The AI stream of `side` and `layer`. Owner: SPEC-010.
pub const fn ai_stream(side: u8, layer: u8) -> StreamId {
    StreamId(STREAM_AI_BASE + side as u32 * 16 + layer as u32)
}

/// Every stream id and block of REGISTRY section 3, ascending. Bases own the whole block up to the
/// next listed range.
pub const STREAM_SPANS: &[Span] = &[
    Span::owned(
        "STREAM_TOY_PLACEMENT",
        "SPEC-003",
        STREAM_TOY_PLACEMENT.0,
        STREAM_TOY_PLACEMENT.0,
    ),
    Span::owned(
        "STREAM_TOY_WANDER",
        "SPEC-003",
        STREAM_TOY_WANDER.0,
        STREAM_TOY_WANDER.0,
    ),
    Span::owned(
        "STREAM_TOY_SKIRMISH",
        "SPEC-003",
        STREAM_TOY_SKIRMISH.0,
        STREAM_TOY_SKIRMISH.0,
    ),
    Span::owned(
        "STREAM_BATTLE_SETUP",
        "SPEC-004",
        STREAM_BATTLE_SETUP.0,
        STREAM_BATTLE_SETUP.0,
    ),
    Span::owned(
        "STREAM_OBSERVE",
        "SPEC-007",
        STREAM_OBSERVE.0,
        STREAM_OBSERVE.0,
    ),
    Span::reserved("STREAM_RESERVED_0D", 0x0D00, 0x0DFF),
    Span::reserved("STREAM_RESERVED_0E", 0x0E00, 0x0EFF),
    Span::owned(
        "STREAM_SOLDIER_BASE",
        "SPEC-005",
        STREAM_SOLDIER_BASE,
        0x4FFF_FFFF,
    ),
    Span::owned(
        "STREAM_MORALE_BASE",
        "SPEC-006",
        STREAM_MORALE_BASE,
        0x50FF_FFFF,
    ),
    Span::owned(
        "STREAM_MESSAGE_BASE",
        "SPEC-007",
        STREAM_MESSAGE_BASE,
        0x51FF_FFFF,
    ),
    Span::owned(
        "STREAM_F1_COUNT_BASE",
        "SPEC-008",
        STREAM_F1_COUNT_BASE,
        0x52FF_FFFF,
    ),
    Span::reserved("STREAM_RESERVED_53", 0x5300_0000, 0x53FF_FFFF),
    Span::owned("STREAM_AI_BASE", "SPEC-010", STREAM_AI_BASE, 0x54FF_FFFF),
    Span::reserved("STREAM_RESERVED_55_57", 0x5500_0000, 0x57FF_FFFF),
    Span::owned(
        "STREAM_F1_VICTIM_BASE",
        "SPEC-008",
        STREAM_F1_VICTIM_BASE,
        0x58FF_FFFF,
    ),
    Span::reserved("STREAM_RESERVED_59_7F", 0x5900_0000, 0x7FFF_FFFF),
    Span::owned(
        "STREAM_MATERIALISED_BASE",
        "SPEC-008",
        STREAM_MATERIALISED_BASE,
        0x8FFF_FFFF,
    ),
    Span::reserved("STREAM_RESERVED_HIGH", 0x9000_0000, 0xFFFF_FFFF),
];

// ---------------------------------------------------------------------------------------------
// Section 4.1: `AgentStore.flags` (u16)
// ---------------------------------------------------------------------------------------------

/// `AgentStore.flags` bit 0: kneeling. Owner: SPEC-004.
pub const F_KNEEL: u16 = 1 << 0;
/// `AgentStore.flags` bit 1: prone. Owner: SPEC-004.
pub const F_PRONE: u16 = 1 << 1;
/// `AgentStore.flags` bit 2: down for a number of ticks. Owner: SPEC-004.
pub const F_DISABLED: u16 = 1 << 2;
/// `AgentStore.flags` bit 3: out for the battle. Owner: SPEC-004.
pub const F_WOUNDED: u16 = 1 << 3;
/// `AgentStore.flags` bit 4: a gun crew member. Owner: SPEC-004.
pub const F_GUN: u16 = 1 << 4;
/// `AgentStore.flags` bit 5: looking back (symptom). Owner: SPEC-006.
pub const F_LOOKBACK: u16 = 1 << 5;
/// `AgentStore.flags` bit 6: left the ranks, alive, outside the roster. Owner: SPEC-006.
pub const F_SKULK: u16 = 1 << 6;

/// Flag bits of REGISTRY 4.1 (`lo` and `hi` are bit numbers), ascending.
pub const FLAG_SPANS: &[Span] = &[
    Span::owned("F_KNEEL", "SPEC-004", 0, 0),
    Span::owned("F_PRONE", "SPEC-004", 1, 1),
    Span::owned("F_DISABLED", "SPEC-004", 2, 2),
    Span::owned("F_WOUNDED", "SPEC-004", 3, 3),
    Span::owned("F_GUN", "SPEC-004", 4, 4),
    Span::owned("F_LOOKBACK", "SPEC-006", 5, 5),
    Span::owned("F_SKULK", "SPEC-006", 6, 6),
    Span {
        name: "F_RESERVED_RULES",
        lo: 7,
        hi: 11,
        owner: "registry",
        reserved: true,
    },
    Span {
        name: "F_RESERVED_HOST",
        lo: 12,
        hi: 15,
        owner: "registry",
        reserved: true,
    },
];

// ---------------------------------------------------------------------------------------------
// Section 4.2: other bit fields and enumerations
// ---------------------------------------------------------------------------------------------

/// `TraitMask` bit: aggressive. Owner: SPEC-007.
pub const T_AGGRESSIVE: u8 = 1;
/// `TraitMask` bit: cautious. Owner: SPEC-007.
pub const T_CAUTIOUS: u8 = 2;
/// `TraitMask` bit: high discipline. Owner: SPEC-007.
pub const T_HIGH_DISC: u8 = 4;
/// `TraitMask` bit: low discipline. Owner: SPEC-007.
pub const T_LOW_DISC: u8 = 8;

/// Options payload flag bit 0: `omniscient_view`. Owner: SPEC-007.
pub const OPT_OMNISCIENT_VIEW: u8 = 1 << 0;
/// Options payload flag bit 1: `instant_orders`. Owner: SPEC-007.
pub const OPT_INSTANT_ORDERS: u8 = 1 << 1;

/// Dispatch medium: courier (0 in a dispatch means automatic). Owner: SPEC-007.
pub const MEDIUM_COURIER: u8 = 1;
/// Dispatch medium: signal flags. Owner: SPEC-007.
pub const MEDIUM_FLAGS: u8 = 2;
/// Dispatch medium: telegraph. Owner: SPEC-007.
pub const MEDIUM_TELEGRAPH: u8 = 3;

/// Dispatch latitude: strict. Owner: SPEC-007.
pub const LATITUDE_STRICT: u8 = 0;
/// Dispatch latitude: discretionary. Owner: SPEC-007.
pub const LATITUDE_DISCRETIONARY: u8 = 1;

/// Command level: army. Owner: SPEC-007.
pub const LEVEL_ARMY: u8 = 0;
/// Command level: corps. Owner: SPEC-007.
pub const LEVEL_CORPS: u8 = 1;
/// Command level: division. Owner: SPEC-007.
pub const LEVEL_DIVISION: u8 = 2;
/// Command level: brigade. Owner: SPEC-007.
pub const LEVEL_BRIGADE: u8 = 3;
/// Command level: regiment. Owner: SPEC-007.
pub const LEVEL_REGIMENT: u8 = 4;

/// `Difficulty.assists` bit 0: ETA window. Owner: SPEC-015.
pub const ASSIST_ETA_WINDOW: u8 = 1 << 0;
/// `Difficulty.assists` bit 1: auto-pause. Owner: SPEC-015.
pub const ASSIST_AUTO_PAUSE: u8 = 1 << 1;
/// `Difficulty.assists` bit 2: order preview. Owner: SPEC-015.
pub const ASSIST_ORDER_PREVIEW: u8 = 1 << 2;
/// `Difficulty.assists` bit 3: full view. Owner: SPEC-015.
pub const ASSIST_FULL_VIEW: u8 = 1 << 3;
/// `Difficulty.assists` bit 4: historical restrictions. Owner: SPEC-015.
pub const ASSIST_HISTORICAL_RESTRICTIONS: u8 = 1 << 4;

/// Tile flag bit 0: `NO_SURVEY`. Owner: SPEC-014.
pub const TILE_NO_SURVEY: u8 = 1 << 0;
/// Tile flag bit 1: `ADJ_RESTORE`. Owner: SPEC-014.
pub const TILE_ADJ_RESTORE: u8 = 1 << 1;
/// Tile flag bit 2: `ADJ_PLAY`. Owner: SPEC-014.
pub const TILE_ADJ_PLAY: u8 = 1 << 2;
/// Tile flag bit 3: `POLISH`. Owner: SPEC-014.
pub const TILE_POLISH: u8 = 1 << 3;
/// Tile flag bit 4: `STEEP`. Owner: SPEC-014.
pub const TILE_STEEP: u8 = 1 << 4;

/// How the numbers written in a REGISTRY 4.2 row relate to the constants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnumKind {
    /// The row lists single-bit masks (`1`, `2`, `4`, `8`).
    Mask,
    /// The row lists bit numbers (`bit 0`, `bit 1`).
    Bit,
    /// The row lists enumeration values.
    Value,
}

/// One named value of an [`Enumeration`], as the REGISTRY row writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Named {
    /// Name as written in the REGISTRY row.
    pub name: &'static str,
    /// The number written next to the name (a mask, a bit number or a value).
    pub value: u32,
}

impl Named {
    /// An entry the row writes as a value or a mask.
    pub const fn value(name: &'static str, value: u8) -> Named {
        Named {
            name,
            value: value as u32,
        }
    }

    /// An entry the row writes as a bit number; `mask` is the constant with that single bit set.
    pub const fn bit(name: &'static str, mask: u8) -> Named {
        Named {
            name,
            value: mask.trailing_zeros(),
        }
    }
}

/// A REGISTRY 4.2 row: a bit field or enumeration with its named values.
#[derive(Clone, Copy, Debug)]
pub struct Enumeration {
    /// First cell of the REGISTRY row.
    pub doc_name: &'static str,
    /// How the row's numbers read.
    pub kind: EnumKind,
    /// The named values.
    pub entries: &'static [Named],
}

/// Every enumeration and bit field of REGISTRY 4.2 that carries numbers.
pub const ENUMERATIONS: &[Enumeration] = &[
    Enumeration {
        doc_name: "TraitMask",
        kind: EnumKind::Mask,
        entries: &[
            Named::value("T_AGGRESSIVE", T_AGGRESSIVE),
            Named::value("T_CAUTIOUS", T_CAUTIOUS),
            Named::value("T_HIGH_DISC", T_HIGH_DISC),
            Named::value("T_LOW_DISC", T_LOW_DISC),
        ],
    },
    Enumeration {
        doc_name: "Options payload flags (`0x12`)",
        kind: EnumKind::Bit,
        entries: &[
            Named::bit("omniscient_view", OPT_OMNISCIENT_VIEW),
            Named::bit("instant_orders", OPT_INSTANT_ORDERS),
        ],
    },
    Enumeration {
        doc_name: "Medium",
        kind: EnumKind::Value,
        entries: &[
            Named::value("Courier", MEDIUM_COURIER),
            Named::value("Flags", MEDIUM_FLAGS),
            Named::value("Telegraph", MEDIUM_TELEGRAPH),
        ],
    },
    Enumeration {
        doc_name: "Latitude",
        kind: EnumKind::Value,
        entries: &[
            Named::value("Strict", LATITUDE_STRICT),
            Named::value("Discretionary", LATITUDE_DISCRETIONARY),
        ],
    },
    Enumeration {
        doc_name: "Level",
        kind: EnumKind::Value,
        entries: &[
            Named::value("army", LEVEL_ARMY),
            Named::value("corps", LEVEL_CORPS),
            Named::value("division", LEVEL_DIVISION),
            Named::value("brigade", LEVEL_BRIGADE),
            Named::value("regiment", LEVEL_REGIMENT),
        ],
    },
    Enumeration {
        doc_name: "Difficulty.assists",
        kind: EnumKind::Bit,
        entries: &[
            Named::bit("ETA window", ASSIST_ETA_WINDOW),
            Named::bit("auto-pause", ASSIST_AUTO_PAUSE),
            Named::bit("order preview", ASSIST_ORDER_PREVIEW),
            Named::bit("full view", ASSIST_FULL_VIEW),
            Named::bit("historical restrictions", ASSIST_HISTORICAL_RESTRICTIONS),
        ],
    },
    Enumeration {
        doc_name: "Tile flags",
        kind: EnumKind::Bit,
        entries: &[
            Named::bit("NO_SURVEY", TILE_NO_SURVEY),
            Named::bit("ADJ_RESTORE", TILE_ADJ_RESTORE),
            Named::bit("ADJ_PLAY", TILE_ADJ_PLAY),
            Named::bit("POLISH", TILE_POLISH),
            Named::bit("STEEP", TILE_STEEP),
        ],
    },
];

// ---------------------------------------------------------------------------------------------
// Section 5: hash blocks, their order, and the block magics
// ---------------------------------------------------------------------------------------------

/// Magic of the morale block, "MOR1". Owner: SPEC-006.
pub const MAGIC_MORALE: u32 = 0x4D4F_5231;
/// Magic of the command block, "CMD1". Owner: SPEC-007.
pub const MAGIC_COMMAND: u32 = 0x434D_4431;
/// Magic of the environment block, "ENV1". Owner: SPEC-009.
pub const MAGIC_ENVIRONMENT: u32 = 0x454E_5631;
/// Magic of the fidelity block that `Battle::hash` appends after `battle_hash`, "FID1". Owner:
/// SPEC-008.
pub const MAGIC_FIDELITY: u32 = 0x4649_4431;

/// A block magic with the ASCII label it spells (most significant byte first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Magic {
    /// Constant name.
    pub name: &'static str,
    /// The `u32` written at the start of the block.
    pub value: u32,
    /// Four ASCII characters equal to `value.to_be_bytes()`.
    pub label: &'static str,
}

/// Every hash block magic of REGISTRY section 5.
pub const MAGICS: &[Magic] = &[
    Magic {
        name: "MAGIC_MORALE",
        value: MAGIC_MORALE,
        label: "MOR1",
    },
    Magic {
        name: "MAGIC_COMMAND",
        value: MAGIC_COMMAND,
        label: "CMD1",
    },
    Magic {
        name: "MAGIC_ENVIRONMENT",
        value: MAGIC_ENVIRONMENT,
        label: "ENV1",
    },
    Magic {
        name: "MAGIC_FIDELITY",
        value: MAGIC_FIDELITY,
        label: "FID1",
    },
];

/// Order of the world block in `battle_hash`. Owner: SPEC-004.
pub const HASH_BLOCK_WORLD: u8 = 1;
/// Order of the regiments block. Owner: SPEC-004.
pub const HASH_BLOCK_REGIMENTS: u8 = 2;
/// Order of the slots (`LocoExt`) block. Owner: SPEC-004.
pub const HASH_BLOCK_SLOTS: u8 = 3;
/// Order of the fire block. Owner: SPEC-005.
pub const HASH_BLOCK_FIRE: u8 = 4;
/// Order of the smoke cells block. Owner: SPEC-005.
pub const HASH_BLOCK_SMOKE: u8 = 5;
/// Order of the casualty records block. Owner: SPEC-005.
pub const HASH_BLOCK_CASUALTIES: u8 = 6;
/// Order of the morale block (written when `morale` is `Some`). Owner: SPEC-006.
pub const HASH_BLOCK_MORALE: u8 = 7;
/// Order of the command block (written when `cmd` is `Some`). Owner: SPEC-007.
pub const HASH_BLOCK_COMMAND: u8 = 8;
/// Order of the environment block (written when `env` is `Some`). Owner: SPEC-009.
pub const HASH_BLOCK_ENVIRONMENT: u8 = 9;

/// One block of `battle_hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HashBlock {
    /// Position in the stream, from 1.
    pub order: u8,
    /// Short block name.
    pub name: &'static str,
    /// Owning spec.
    pub owner: &'static str,
    /// The leading magic of an optional block; `None` for blocks that are always written.
    pub magic: Option<u32>,
}

/// The blocks of `battle_hash` in the order they are written.
pub const HASH_BLOCKS: &[HashBlock] = &[
    HashBlock {
        order: HASH_BLOCK_WORLD,
        name: "world",
        owner: "SPEC-004",
        magic: None,
    },
    HashBlock {
        order: HASH_BLOCK_REGIMENTS,
        name: "regiments",
        owner: "SPEC-004",
        magic: None,
    },
    HashBlock {
        order: HASH_BLOCK_SLOTS,
        name: "slots",
        owner: "SPEC-004",
        magic: None,
    },
    HashBlock {
        order: HASH_BLOCK_FIRE,
        name: "fire",
        owner: "SPEC-005",
        magic: None,
    },
    HashBlock {
        order: HASH_BLOCK_SMOKE,
        name: "smoke",
        owner: "SPEC-005",
        magic: None,
    },
    HashBlock {
        order: HASH_BLOCK_CASUALTIES,
        name: "casualties",
        owner: "SPEC-005",
        magic: None,
    },
    HashBlock {
        order: HASH_BLOCK_MORALE,
        name: "morale",
        owner: "SPEC-006",
        magic: Some(MAGIC_MORALE),
    },
    HashBlock {
        order: HASH_BLOCK_COMMAND,
        name: "command",
        owner: "SPEC-007",
        magic: Some(MAGIC_COMMAND),
    },
    HashBlock {
        order: HASH_BLOCK_ENVIRONMENT,
        name: "environment",
        owner: "SPEC-009",
        magic: Some(MAGIC_ENVIRONMENT),
    },
];

// ---------------------------------------------------------------------------------------------
// Section 6: tick order
// ---------------------------------------------------------------------------------------------

/// One step of the full tick order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickStep {
    /// Step id as written in REGISTRY section 6 (`PR-1`, `TK-04`, `PS-2`).
    pub id: &'static str,
    /// What the step does, in a few words.
    pub what: &'static str,
}

const fn step(id: &'static str, what: &'static str) -> TickStep {
    TickStep { id, what }
}

/// The ordered list of every step around and inside one tick (REGISTRY 6.1, 6.2, 6.3): the shell
/// steps before the step (`PR-*`, SPEC-002, 004, 005, 007, 010, 011, 015), the `TK-*` steps of
/// `BattleState::step` and `Battle::step` (SPEC-002, 004 to 009), then the steps after it (`PS-*`,
/// SPEC-002, 003, 011, 012, 016). A layer that is off skips its steps; the rest keep their order.
pub const TICK_STEPS: &[TickStep] = &[
    step("PR-1", "apply the tick's inputs in (actor, seq) order"),
    step("PR-2", "AI step and UI capture through ports"),
    step(
        "TK-01",
        "departures, arrivals, triggers, classification, hand-overs",
    ),
    step("TK-02", "weather refresh"),
    step("TK-03", "command step"),
    step("TK-04", "movement"),
    step("TK-05", "rebuild grid, release expired disables"),
    step("TK-06", "fire phase"),
    step("TK-07", "flight and hit"),
    step("TK-08", "F1 bundle release"),
    step("TK-09", "F2 exchange"),
    step("TK-10", "apply casualties"),
    step("TK-11", "morale step"),
    step("TK-12", "smoke"),
    step("TK-13", "peek update and observers"),
    step("TK-14", "roster updates"),
    step("TK-15", "advance tick"),
    step("PS-1", "checkpoint hash"),
    step("PS-2", "evaluate history observations"),
    step("PS-3", "drain sound log and events"),
];
