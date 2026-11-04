//! Reports (SPEC-007 section 7): each officer periodically sends his parent the contacts he has
//! and one friend entry per regiment of his formation; the parent merges them with the same
//! association rule and passes them on in his own next report. Reports are not acknowledged.

use super::media::{Arrival, NewMessage};
use super::observe::join_contact;
use super::picture::{confidence_pm, fade_pm};
use super::state::{Contact, FriendEntry, MessageKind, MessagePayload, OrderBody, Picture};
use super::{CommandTable, Latitude, MessageId, OfficerId};
use crate::{BattleState, Order};

/// Words of a report for latency: a fixed header and one word per contact.
const REPORT_HEADER_WORDS: u32 = 4;

/// Records what a report says about a friendly regiment; the entry made later stands.
pub(crate) fn join_friend(pic: &mut Picture, f: FriendEntry) {
    match pic
        .friends
        .binary_search_by_key(&f.regiment, |e| e.regiment)
    {
        Ok(i) if pic.friends[i].seen > f.seen => {}
        Ok(i) => pic.friends[i] = f,
        Err(i) => pic.friends.insert(i, f),
    }
}

struct Outgoing {
    from: OfficerId,
    to: OfficerId,
    contacts: Vec<Contact>,
    friends: Vec<FriendEntry>,
}

impl BattleState {
    /// Sends the reports due this tick: every officer who is up and has a parent, at ticks where
    /// `(tick + id) % report_interval_ticks == 0`, in ascending id. A regiment officer first
    /// enters his own regiment as a friend (he knows it at first hand) and reports the contacts
    /// seen since his last report; any other officer knows only what reports told him and
    /// forwards every contact still held, since a report may reach him long after the
    /// sighting. At most `report_max` contacts go, confidence descending then id ascending.
    /// The medium is chosen automatically (a link if any, else a courier).
    pub fn step_reports(&mut self, table: &CommandTable) {
        let now = self.world.tick;
        let g = &table.globals;
        let Some(cmd) = self.cmd.as_mut() else { return };
        let mut out = Vec::new();
        for i in 0..cmd.officers.len() {
            let o = &cmd.officers[i];
            let due = (now + u64::from(o.id.0)).is_multiple_of(u64::from(g.report_interval_ticks));
            let Some(parent) = o.parent.filter(|_| due && !o.down) else {
                continue;
            };
            let (from, since, leaf) = (o.id, o.last_report, o.regiment);
            if let Some(reg) = leaf.and_then(|r| self.regiments.get(usize::from(r.0))) {
                if !reg.roster.is_empty() {
                    let own = FriendEntry {
                        regiment: reg.id,
                        pos: reg.anchor,
                        strength: reg.roster.len() as u32,
                        seen: now,
                    };
                    join_friend(&mut cmd.pics[i], own);
                }
            }
            let mut ranked: Vec<(u32, &Contact)> = cmd.pics[i]
                .contacts
                .iter()
                .filter(|c| leaf.is_none() || since == 0 || c.seen > since)
                .map(|c| (confidence_pm(c, now, table), c))
                .filter(|(conf, _)| *conf >= g.drop_pm)
                .collect();
            ranked.sort_by_key(|(conf, c)| (std::cmp::Reverse(*conf), c.id));
            let contacts = ranked
                .into_iter()
                .take(g.report_max as usize)
                .map(|(_, c)| c.clone())
                .collect();
            out.push(Outgoing {
                from,
                to: parent,
                contacts,
                friends: cmd.pics[i].friends.clone(),
            });
            cmd.officers[i].last_report = now;
        }
        for r in out {
            let words = REPORT_HEADER_WORDS + r.contacts.len() as u32;
            // A report that cannot be sent (unknown officer) is simply not sent.
            let _ = self.launch(
                table,
                NewMessage {
                    kind: MessageKind::Report,
                    from: r.from,
                    to: r.to,
                    medium: None,
                    seq: 0,
                    body: OrderBody {
                        order: Order::Hold,
                        latitude: Latitude::Strict,
                    },
                    words,
                    root: None::<MessageId>,
                    payload: MessagePayload::Report {
                        contacts: r.contacts,
                        friends: r.friends,
                    },
                },
            );
        }
    }

    /// Merges the reports among `arrivals` into the pictures of their addressees: contacts by the
    /// association rule (those already faded are ignored), friends by regiment.
    pub fn receive_reports(&mut self, table: &CommandTable, arrivals: &[Arrival]) {
        let now = self.world.tick;
        let Some(cmd) = self.cmd.as_mut() else { return };
        for a in arrivals {
            let MessagePayload::Report { contacts, friends } = &a.msg.payload else {
                continue;
            };
            let Some(i) = cmd.index_of(a.msg.to) else {
                continue;
            };
            for c in contacts {
                let age = now.saturating_sub(c.seen);
                if fade_pm(c.quality_pm, age, table) >= table.globals.drop_pm {
                    join_contact(&mut cmd.pics[i], c.clone(), table, now);
                }
            }
            for f in friends {
                join_friend(&mut cmd.pics[i], f.clone());
            }
        }
    }
}
