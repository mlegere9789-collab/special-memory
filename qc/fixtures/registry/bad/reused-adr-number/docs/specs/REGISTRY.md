# REGISTRY: fixture, well-formed

## 1. Spec index

| Spec | Topic | CT prefix | Tickets |
|---|---|---|---|
| 004 | movement | `CT-004-n` | T-014 to T-016 |
| 005 | weapons | `CT-005-n` | T-020 |
| registry | this file | none | T-025 |

## 2. Replay inputs

### 2.2 Payload tags

| Tag | Owner | Meaning |
|---|---|---|
| `0x01` Hold, `0x02` Move | SPEC-004 | regiment order |
| `0x06` to `0x0F` | reserved | free |
| `0x10` | SPEC-007 | dispatch |

## 3. Random streams

| Stream id | Owner | Meaning |
|---|---|---|
| `1`, `2` | SPEC-003 | toy scenario |
| `0x0B01` | SPEC-004 | setup |
| `0x0D00..0x0DFF` | reserved | free |
| `0x4000_0000 + slot` | SPEC-005 | soldiers |

## 9. Amendments to base specs

### 9.3 SPEC-004 (movement)

| Label | Content | Source | Ticket | Status |
|---|---|---|---|---|
| A1 | gait reuse | T-016 | T-016 | landed |
| A2 | command tag space | SPEC-007 | T-071 | pending |

Reserved: SPEC-004 A3 has no ticket yet.

## 17. ADR numbers

| ADR | Topic | Owner |
|---|---|---|
| 0001 to 0002 | existing | |
| 0003 | identifiers | registry |
| 0002 | second claim on a number | SPEC-007 |
| 0004 and above | free | |
