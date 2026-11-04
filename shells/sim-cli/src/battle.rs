//! `hd-sim battle`: runs a library scenario (`battle_rules::scenarios`) for a number of ticks and
//! prints the final state hash. `command_chain` (SPEC-007) steps with the command layer; the
//! movement scenarios step plainly.

use battle_rules::command::CommandTable;
use battle_rules::scenarios::{by_name, command::command_chain};
use battle_rules::Tables;
use std::path::Path;

/// Runs scenario `name` for `ticks` ticks with `seed` under the rules in `<data>/rules` and
/// returns the report line.
///
/// Errors: a message when the rules cannot be read or the scenario is unknown.
pub fn run_battle(data: &Path, name: &str, seed: u64, ticks: u64) -> Result<String, String> {
    let rules = data.join("rules");
    let dir = rules.to_str().ok_or("data directory is not valid UTF-8")?;
    let tables = Tables::load_dir(dir).map_err(|e| format!("rules: {e}"))?;
    let hash = if name == "command_chain" {
        let path = rules.join("command.toml");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let table = CommandTable::from_toml_str(&text).map_err(|e| format!("command.toml: {e}"))?;
        let mut b = command_chain(&tables, &table, seed);
        (0..ticks).for_each(|_| b.step_with_command(&table));
        b.hash()
    } else {
        let mut b = by_name(name, &tables, seed).ok_or(format!("unknown scenario {name:?}"))?;
        (0..ticks).for_each(|_| b.step());
        b.hash()
    };
    Ok(format!(
        "scenario={name} seed={seed} ticks={ticks} final_hash={hash:016x}\n"
    ))
}
