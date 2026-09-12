# `--table` and `--ignore-tables` are mutually exclusive

**Status**: accepted

## Context

Two Introspection Filters select Entities by name: `--table` (an allow-list) and `--ignore-tables` (a deny-list). Most CLIs let both be passed and define a precedence rule between them.

## Decision

`clap`'s `conflicts_with` rejects the two together: the user picks the allow-list or the deny-list, never both. `--schema` combines freely with either, because it filters on a different axis.

Combining an allow-list with a deny-list has no meaning the user can predict. `--table users,orders --ignore-tables orders` either means "users" (deny wins) or "users, orders" (allow wins), and whichever we picked, half our users would assume the other. Refusing the combination costs nothing — the same result is always reachable by shortening one of the two lists — and removes a precedence rule nobody would remember.

## Consequences

The filters recorded in a Lock File can be read back without a precedence rule to replay: at most one of the two lists is ever non-empty, which keeps the comparability check of [ADR-0005](0005-erdify-lock-format-and-semantics.md) a plain equality.
