# mdutils

Utility preprocessors for [mdBook](https://rust-lang.github.io/mdBook/).

The binaries behind every utility share a single command (`mdutils`). Add the
utilities you want as separate `[preprocessor.util-*]` sections in `book.toml`.

## Utilities

| Utility | Purpose |
|---------|---------|
| [`util-banner`](#util-banner) | Insert a dismissible, theme-aware warning banner on every page. |
| [`util-private-page`](#util-private-page) | Hide a chapter from the nav sidebar (and prev/next) while still building its HTML. |

---

## `util-banner`

Display a dismissible, theme-aware warning banner across **all pages** of your book.

**Syntax**

````markdown
{{util-banner}}
**Heads up!** This is a banner with **markdown** support.

- You can use lists
- And [links](https://example.com)
{{/util-banner}}
````

**Before** — the banner block sits in any chapter:

```markdown
{{util-banner}}
Preview build — not for production.
{{/util-banner}}

# Welcome
```

**After** — every chapter gets the banner injected:

```
[ BANNER: "Preview build — not for production."  [×] ]

# Welcome
...
```

### Configuration

All options live under `[preprocessor.util-banner]`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `command` | string | — | **Required.** Set to `"mdutils"`. |
| `position` | string | `"top"` | Where the banner appears: `"top"` or `"bottom"`. |
| `sticky` | bool | `false` | If `true`, the close button is hidden and the banner cannot be dismissed. |

```toml
[preprocessor.util-banner]
command = "mdutils"
position = "top"     # "top" (default) or "bottom"
sticky = false       # true → no close button; banner is permanent
```

### How it works

1. Scans every chapter for `{{util-banner}}...{{/util-banner}}` blocks.
2. Renders the inner markdown to HTML via [pulldown-cmark](https://crates.io/crates/pulldown-cmark).
3. Removes the banner blocks from the source chapters.
4. Injects styled banner `<div>` elements (plus CSS and optional JavaScript) into **every** chapter, at the configured position.
5. When `sticky = false`, a small `<script>` stores yesterday's dismiss date in `localStorage`. On the next page load the banner checks the stored date and stays hidden until the calendar date changes.

Banners use mdBook-compatible CSS custom properties (`--util-banner-*`) that adapt across **light**, **coal**, **navy**, **ayu**, and **rust** themes. Define these variables in a custom theme to restyle.

---

## `util-private-page`

Hide a chapter **from the sidebar** (and skip it in the prev/next navigation)
**without** removing it from the build — the page's HTML is still generated and
reachable by its direct URL.

**Mark a chapter as private** by placing this HTML comment anywhere in the
`.md` file (it is stripped from the built page):

```markdown
<!--util-private-page-->

# Secret Draft
This page won't show in the sidebar, but `secret_draft.html` still exists.
```

**Before** — the page appears in the TOC normally:

```
1. Introduction
2. Secret Draft     ← visible
```

**After** — on every page the sidebar drops the entry, and prev/next skip it:

```
1. Introduction
2. (skipped in sidebar and in prev/next navigation)
```

### Configuration

All options live under `[preprocessor.util-private-page]`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `command` | string | — | **Required.** Set to `"mdutils"`. |
| `hide_sidebar` | bool | `true` | Remove the private page from the table-of-contents sidebar. |
| `hide_nav_buttons` | bool | `true` | Make prev/next skip the private page instead of linking to it. |

```toml
[preprocessor.util-private-page]
command = "mdutils"
hide_sidebar = true        # false → keep the entry in the sidebar
hide_nav_buttons = true    # false → prev/next still step through private pages
```

### How it works

mdBook's `Book` structure drives both HTML output *and* the sidebar from the
same data, so a preprocessor cannot separate them structurally. Instead,
`util-private-page` **keeps the chapter in the book (so it builds)** and hides
it with a small runtime script injected into every chapter:

1. Collects every chapter's output href (`chapter.path` with `.html`) in
   render order, and flags those containing the marker.
2. Strips the marker from the private pages.
3. Injects a `<script>` into **every** chapter. On `DOMContentLoaded` — after
   mdBook's `toc.js` has populated the sidebar WebComponent — it:
   - removes the sidebar `<li>` whose link points at a private page; and
   - rewrites the prev/next links to jump over private pages.

### Limitations

- **Not a security boundary.** Private pages are fully reachable by URL and
  still appear in the search index and `print.html`. Use it to *unlist* /
  declutter, not to protect confidential content.
- **Sidebar numbering may show gaps** (e.g. `1.` then `3.`), because mdBook
  bakes the numbers into `toc.js` at build time and they cannot be renumbered
  from a preprocessor.
- Depends on the client running JavaScript (standard for the mdBook front end).

---

## Installation

```bash
cargo install mdutils
```

## Example

See the [`example/`](example/) directory for a minimal mdBook that demos both
utilities. Build it with:

```bash
cd example
mdbook build
```

Then open `example/book/index.html` in your browser. Try visiting
`example/book/secret_page.html` directly — it builds and renders, but is
missing from the sidebar.

---

## License

MIT