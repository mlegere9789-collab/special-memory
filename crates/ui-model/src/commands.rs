//! `UiCommand`: the only way the interface changes the simulation, by becoming an input record
//! (SPEC-015 section 3, item 3). Nothing here touches a battle; the shell applies the record.

use battle_rules::command::{encode_dispatch, Latitude, OfficerId};
use battle_rules::registry::seat_actor;
use battle_rules::{encode_order, BattleState, Order, RegimentId};
use sim_state::InputRecord;

/// A request from the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiCommand {
    /// Give a regiment an order.
    Order {
        /// Regiment addressed.
        regiment: RegimentId,
        /// The order.
        order: Order,
    },
}

/// How an order travels: straight to the regiment, or as a dispatch from the seat officer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// `encode_order` payload; used when the command layer is off or the seat has instant orders.
    Direct,
    /// SPEC-007 dispatch (tag `0x10`).
    Dispatch {
        /// Sending officer, the seat officer.
        from: OfficerId,
        /// Addressee officer.
        to: OfficerId,
        /// 0 for automatic, else a medium tag.
        medium: u8,
        /// How strictly the order binds.
        latitude: Latitude,
    },
}

impl Route {
    /// Direct when the command layer is off or `side` has instant orders; otherwise an automatic
    /// strict dispatch from `seat` to `addressee`.
    pub fn choose(state: &BattleState, side: u8, seat: OfficerId, addressee: OfficerId) -> Route {
        match state.cmd.as_ref() {
            Some(c) if !c.opt[usize::from(side & 1)].instant_orders => Route::Dispatch {
                from: seat,
                to: addressee,
                medium: 0,
                latitude: Latitude::Strict,
            },
            _ => Route::Direct,
        }
    }
}

impl UiCommand {
    /// The input record at `tick` for the seat of `side`, with sequence number `seq`.
    pub fn to_input(&self, tick: u64, side: u8, seq: u16, route: Route) -> InputRecord {
        let UiCommand::Order { regiment, order } = self;
        let payload = match route {
            Route::Direct => encode_order(*regiment, order),
            Route::Dispatch {
                from,
                to,
                medium,
                latitude,
            } => encode_dispatch(from, to, medium, latitude, order),
        };
        InputRecord {
            tick,
            actor: seat_actor(side),
            seq,
            payload,
        }
    }
}
