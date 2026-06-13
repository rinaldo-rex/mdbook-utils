# Architecture Decision Records — mdbook-utils

## ADR-001: One `[preprocessor.util-*]` section per utility

**Date**: 2026-06-13
**Status**: Accepted

Each utility (banner, tooltip, …) gets its own `[preprocessor.util-<name>]`
section in `book.toml`.  All sections point to the **same binary** via
`command = "mdbook-utils"`:

```toml
[preprocessor.util-banner]
command = "mdbook-utils"
position = "top"
sticky = false

[preprocessor.util-tooltip]
command = "mdbook-utils"
theme = "ayu"
```

The binary dispatches internally by checking which top-level keys exist in
`ctx.config`.  This keeps the user config flat and self-documenting — the
section name **is** the utility name.  No artificial prefix convention is
needed because the TOML section already scopes every key inside it.

*Alternatives considered*:
- A single `[preprocessor.utils]` section with `banner-position`,
  `tooltip-theme`, … keys (`prefix` convention).  Rejected because it
  creates long, noisy key names, mixes unrelated options in one table, and
  makes it harder to discover which utility an option belongs to.

---

## ADR-002: No name-prefix on keys inside a utility section

**Date**: 2026-06-13
**Status**: Accepted

Keys inside a utility section **must not** repeat the utility name as a prefix.

✅  Good — the section scopes every key:

```toml
[preprocessor.util-banner]
position = "top"        # clearly a banner option
sticky = false
```

❌  Avoid — prefix repeats information already in the section key:

```toml
[preprocessor.util-banner]
banner-position = "top"  # noisy; the section says "banner" already
banner-sticky  = false
```

---

## ADR-003: One binary for all utilities

**Date**: 2026-06-13
**Status**: Accepted

`mdbook-utils` is a single binary that hosts every utility preprocessor.
Users install one crate (`cargo install mdbook-utils`) and configure
individual utilities through separate `[preprocessor.util-*]` sections, each
with `command = "mdbook-utils"`.

*Rationale*:
- One install instead of N
- Shared dependency tree → smaller binary footprint
- Utilities that need cross-cutting logic (e.g., deduplicating injected
  assets) can coordinate in a single process

*Implementation*:  `UtilsPreprocessor::new()` reads `PreprocessorContext` and
detects which utilities are configured.  `Preprocessor::run()` dispatches
each utility's processing pass.

---

## ADR-004: Global banner injection

**Date**: 2026-06-13
**Status**: Accepted

When any chapter contains a `{{util-banner}}` block, the rendered banner
HTML is injected into **every chapter** (not just the one where the marker
appeared).

*Rationale*:
- Banners are site-wide warnings; users expect them on every page.
- mdBook's `print.html` (which concatenates all chapters) would miss banners
  if they were injected per-source-chapter only.
- Search results and the table-of-contents sidebar navigate to chapters that
  may not have contained the marker — those pages should still show the
  banner.

*Implementation (two-pass)*:
1. **Pass 1** — iterate all chapters, collect every `{{util-banner}}`
   block's inner content.
2. **Pass 2** — remove all marker blocks from every chapter, then inject the
   collected HTML banners at the configured position.

---

## ADR-005: Theme-aware CSS via mdBook variables

**Date**: 2026-06-13
**Status**: Accepted

All injected HTML uses CSS custom properties that reference mdBook's theme
variables (`--bg`, `--fg`, `--theme-popup-bg`, etc.) with explicit
fallbacks.  Utility-specific variables are prefixed `--util-<name>-*` (e.g.,
`--util-banner-bg`, `--util-banner-border`).  This ensures:

- Automatic dark/light adaptation across light, coal, navy, ayu, and rust
  themes.
- Users who author custom themes can still control utility styling by
  defining the prefixed variables in their theme CSS.
- Fallback values produce a reasonable appearance even when no theme
  variable is set.

---

## ADR-006: Dismiss via localStorage + date stamp

**Date**: 2026-06-13
**Status**: Accepted

Dismissible UI elements (banners, alerts) use `localStorage` with a date
key (`YYYY-MM-DD`) so they reappear on the next calendar day.  The pattern:

1. On `DOMContentLoaded`, compare `localStorage` date-stamp against today.
2. If they match, hide the element with `display: none`.
3. On close-button click, write today's date to `localStorage` and hide all
   matching elements.
4. A `sticky` config flag disables the dismiss logic entirely (no script
   injected).

This works without a server, keeps per-browser state, and requires zero
additional dependencies.
