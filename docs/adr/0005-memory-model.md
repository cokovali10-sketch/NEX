# ADR 0005: Defer ownership and lifetimes

## Status

Accepted.

## Decision

NEX 0.1 does not implement ownership or lifetime checking.

## Rationale

The first milestone focuses on a correct vertical compiler slice. The memory
model is specified before implementation to avoid incompatible later changes.
