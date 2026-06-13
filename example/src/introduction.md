{{util-banner}}
**Heads up!** This is a demo of the `util-banner` preprocessor.

- Banner content supports **markdown**
- Dismiss it with the `×` button — it'll stay hidden until tomorrow
- Try switching themes in the top-right menu to see theme-aware colours
{{/util-banner}}

# Introduction

Welcome to the **mdbook-utils** example book.

This page defines a banner block above. After the preprocessor runs, that
block is removed from this chapter's source and the rendered HTML banner is
injected into **every** chapter (this one and [Another Page](another_page.md)).

## How to test

1. Build with `mdbook build`
2. Open `book/index.html` in your browser
3. The warning banner should appear at the top of every page
4. Click `×` to dismiss — refresh the page and it stays hidden
5. Try changing your system clock to tomorrow (or manipulate `localStorage`) to see it reappear
