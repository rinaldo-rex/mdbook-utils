# mdutils

Utility preprocessors for [mdBook](https://rust-lang.github.io/mdBook/).

## Utilities

### `util-banner`

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

---

## Installation

```bash
cargo install mdutils
```

---

## Usage

Add the preprocessor to your `book.toml`:

```toml
[preprocessor.util-banner]
command = "mdutils"

# ── Optional settings ────────────────
position = "top"     # "top" (default) or "bottom"
sticky = false       # true → no close button; banner is permanent
```

Then add one or more `{{util-banner}}` blocks anywhere in your book:

```markdown
{{util-banner}}
**Warning**: This documentation is under active development.
{{/util-banner}}
```

---

## Configuration

All options live under `[preprocessor.util-banner]`:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `command` | string | — | **Required.** Set to `"mdutils"`. |
| `position` | string | `"top"` | Where the banner appears: `"top"` or `"bottom"`. |
| `sticky` | bool | `false` | If `true`, the close button is hidden and the banner cannot be dismissed. |

---

## How it works

1. Scans every chapter for `{{util-banner}}...{{/util-banner}}` blocks.
2. Renders the inner markdown to HTML via [pulldown-cmark](https://crates.io/crates/pulldown-cmark).
3. Removes the banner blocks from the source chapters.
4. Injects styled banner `<div>` elements (plus CSS and optional JavaScript) into **every** chapter, at the configured position.
5. When `sticky = false`, a small `<script>` stores yesterday's dismiss date in `localStorage`. On the next page load the banner checks the stored date and stays hidden until the calendar date changes.

### Theme support

Banners use mdBook-compatible CSS custom properties that work across **light**, **coal**, **navy**, **ayu**, and **rust** themes:

- `--util-banner-bg` — background colour
- `--util-banner-fg` — text colour  
- `--util-banner-border` — border colour

If you use a custom theme, add these variables to your theme's CSS file to match the look.

---

## Example

See the [`example/`](example/) directory for a minimal mdBook that demos `util-banner`. Build it with:

```bash
cd example
mdbook build
```

Then open `example/book/index.html` in your browser.

---

## License

MIT
