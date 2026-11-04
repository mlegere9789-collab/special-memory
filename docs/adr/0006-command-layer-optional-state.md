# ADR 0006: The command layer is optional state and its hash block is conditional

**Status:** accepted, 2026-11-04. Source: SPEC-007 sections 4 and 9, `docs/specs/REGISTRY.md` sections 2, 5 and 17 (ticket T-071).
The registry first assigned this decision the number 0005; that number is the supply-chain ADR, so the registry lists it as 0006.

**Decision.**

1. Command friction (messages, officers, pictures) lives in `BattleState::cmd: Option<CommandState>`. `None` is the state of every
   battle that does not enable the layer, and it leaves every SPEC-004 and SPEC-005 behaviour untouched.
2. The command hash block is written only when `cmd` is `Some`. It begins with the magic `0x434D_4431` ("CMD1") and follows the
   morale block in the fixed order of REGISTRY section 5; with `cmd` `None` nothing is written, so every golden recorded before the
   layer existed keeps its value. A fixed block order with a magic each means no combination of present and absent layers can write
   the same bytes as another.
3. With `cmd` `Some`, a seat actor (`0x00`, `0x01`, `0x80`, `0x81`) can no longer address a regiment directly: wire tags `0x01` to
   `0x05` from a seat are refused (`InputError::DirectOrder`) unless the seat's `instant_orders` option is set. Script actors and
   engine-issued orders are never refused. Seats order through Dispatch (`0x10`) instead. SPEC-004 amendment A2 states the same rule
   at the place the order wire format is defined.
4. The data layer ships first and independently of the state: `data/rules/command.toml` (`command.schema.json`), the officers
   sidecar (`officers.schema.json`), `CommandTable`, `load_officers`, `trait_mask`, and the validator plugin
   `tools/validators/command.py`. Loading data changes no battle.

**Test.** T7-01 (`crates/battle-rules/tests/t7_01_loader.rs`, `tools/test_command_validator.py`) covers the loaders; the
conditional block is proved by T7-16 (a battle with `cmd = None` hashes to the existing goldens, and every hashed field of a
battle with `cmd = Some` changes the hash).

**Consequences.** Later layers (morale, environment) follow the same pattern: an `Option` field, a magic, a fixed place in the
block order. A layer that is on cannot be switched off mid-battle without changing the hash, which is the intended behaviour.
