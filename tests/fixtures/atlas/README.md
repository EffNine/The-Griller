# Atlas Fact Store

Atlas is a deterministic fact store. It records structured engineering
facts — symbols, modules, packages, build targets and tests — with
provenance. Milestone P10.5.1 covers the fact store ingestion pipeline.

The fact store stores facts. Each fact is stored with a confidence score.
Facts are served through a query API. The store is append-only and every
fact carries a receipt.

## Layout

- `src/fact_store.rs` — the fact store core.
- `src/lib.rs` — crate root.
