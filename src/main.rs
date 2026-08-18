// Architecture Decision Records: see adr.md at the repo root.
// Key conventions:
//   - One [preprocessor.util-*] section per utility (ADR-001).
//   - No name-prefix on keys inside a utility section (ADR-002).
//   - All utilities share this single binary (ADR-003).
//   - Global banner injection (ADR-004).
//   - Theme-aware CSS via mdBook variables (ADR-005).
//   - localStorage + date-stamp dismiss pattern (ADR-006).
//   - Hide-from-sidebar-but-keep-building via runtime JS (ADR-007).

use clap::{Arg, Command};
use mdbook_preprocessor::{
    book::{Book, BookItem},
    errors::Error,
    Preprocessor, PreprocessorContext,
};
use pulldown_cmark::{Options, Parser};
use regex::Regex;
use std::io;
use std::process;
use std::sync::LazyLock;

// ── CSS injected into every chapter that contains banners ─────────────
//
// Uses mdBook theme CSS variables (--bg, --fg, etc.) as backdrop, plus
// --util-banner-* fallbacks that users can override with custom CSS.
const BANNER_CSS: &str = r#"<style>
.util-banner {
  background: var(--util-banner-bg, hsl(48, 88%, 92%));
  color: var(--util-banner-fg, hsl(30, 60%, 22%));
  border: 1px solid var(--util-banner-border, hsl(45, 90%, 50%));
  border-left-width: 4px;
  padding: 0.75rem 1rem;
  margin: 1rem 0;
  border-radius: 4px;
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
}
.util-banner-content {
  flex: 1;
  min-width: 0;
}
.util-banner-content > p:first-child { margin-top: 0; }
.util-banner-content > p:last-child  { margin-bottom: 0; }
.util-banner-close {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 1.3rem;
  line-height: 1;
  padding: 0 0.25rem;
  color: var(--util-banner-fg, inherit);
  opacity: 0.55;
  flex-shrink: 0;
}
.util-banner-close:hover { opacity: 1; }
</style>"#;

// ── Dismiss script (injected when sticky = false) ────────────────────
//
// On page load, reads yesterday's dismiss date from localStorage.
// If it matches today, banners are hidden.
// Clicking any close button persists today's date and hides all banners.
const BANNER_SCRIPT: &str = r#"<script>
document.addEventListener('DOMContentLoaded', function() {
  var banners = document.querySelectorAll('.util-banner');
  if (!banners.length) return;
  var today = new Date().toISOString().split('T')[0];
  if (localStorage.getItem('util-banner-dismissed-date') === today) {
    banners.forEach(function(b) { b.style.display = 'none'; });
  }
  banners.forEach(function(b) {
    var btn = b.querySelector('.util-banner-close');
    if (btn) {
      btn.addEventListener('click', function() {
        localStorage.setItem('util-banner-dismissed-date', today);
        banners.forEach(function(b2) { b2.style.display = 'none'; });
      });
    }
  });
});
</script>"#;

// ── Regex ─────────────────────────────────────────────────────────────

static BANNER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\{\{util-banner\}\}\s*(?P<content>.*?)\{\{/util-banner\}\}").unwrap()
});

// A chapter is marked private by placing this HTML comment anywhere in it.
// The marker is removed before rendering, then the chapter is hidden from the
// sidebar (and skipped in prev/next) by an injected front-end script.
static PRIVATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[ \t]*<!--[ \t]*util-private-page[ \t]*-->[ \t]*(?:\r?\n)?").unwrap()
});

// A regex to find relative links in rendered banner HTML.
// Matches <a href="./foo.html"> but NOT absolute URLs, root-relative paths,
// anchors, or parent-relative links (../).
//
// The banner content always uses `./` for same-directory links (e.g.
// `./my_journey_into_ai.html`). By stripping the `./` prefix, the link
// becomes root-relative (e.g. `my_journey_into_ai.html`), which resolves
// correctly from any chapter in the book.
static BANNER_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(<a\s[^>]*href=")\.\/([^"]+)"#).unwrap());

// ── CLI ───────────────────────────────────────────────────────────────

fn make_app() -> Command {
    Command::new("mdutils")
        .about(concat!(
            "Utility preprocessors for mdBook.\n",
            "Detected preprocessors are dispatched automatically."
        ))
        .subcommand(
            Command::new("supports")
                .arg(Arg::new("renderer").required(true))
                .about("Check whether a renderer is supported by this preprocessor"),
        )
}

fn main() {
    let matches = make_app().get_matches();

    // mdBook calls `mdbook-utils supports <renderer>` to probe support.
    if let Some(sub_args) = matches.subcommand_matches("supports") {
        let renderer = sub_args
            .get_one::<String>("renderer")
            .expect("Required argument");
        // All utilities target the HTML renderer.
        process::exit(if renderer == "html" { 0 } else { 1 });
    }

    // Normal run: read [PreprocessorContext, Book] from stdin.
    let (ctx, book) = mdbook_preprocessor::parse_input(io::stdin()).unwrap();

    // Dispatch all enabled utility preprocessors.
    let preprocessor = UtilsPreprocessor::new(&ctx);
    if let Err(e) = run_preprocessing(&preprocessor, &ctx, book) {
        eprintln!("{e:?}");
        process::exit(1);
    }
}

fn run_preprocessing(
    pre: &UtilsPreprocessor,
    _ctx: &PreprocessorContext,
    book: Book,
) -> Result<(), Error> {
    let processed = pre.run(_ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed)?;
    Ok(())
}

// ── Preprocessor ──────────────────────────────────────────────────────

struct UtilsPreprocessor {
    banner: Option<BannerConfig>,
    private: Option<PrivateConfig>,
}

// Keys are plain (no prefix) — scoped by [preprocessor.util-banner]  (ADR-001, ADR-002)
struct BannerConfig {
    position: String, // "top" or "bottom"
    sticky: bool,
}

// Scoped by [preprocessor.util-private-page] (ADR-001, ADR-002).
struct PrivateConfig {
    hide_sidebar: bool,
    hide_nav_buttons: bool,
}

impl UtilsPreprocessor {
    fn new(ctx: &PreprocessorContext) -> Self {
        // Detect whether [preprocessor.util-banner] is configured.
        let banner = ctx
            .config
            .get::<bool>("preprocessor.util-banner.sticky")
            .ok()
            .flatten()
            .map(|_| BannerConfig {
                position: ctx
                    .config
                    .get::<String>("preprocessor.util-banner.position")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "top".to_string()),
                sticky: ctx
                    .config
                    .get::<bool>("preprocessor.util-banner.sticky")
                    .ok()
                    .flatten()
                    .unwrap_or(false),
            });

        // Detect whether [preprocessor.util-private-page] is configured by
        // checking for the section as a whole (later code reads its flags).
        let private = ctx
            .config
            .get::<serde_json::Value>("preprocessor.util-private-page")
            .ok()
            .flatten()
            .map(|_| PrivateConfig {
                hide_sidebar: ctx
                    .config
                    .get::<bool>("preprocessor.util-private-page.hide_sidebar")
                    .ok()
                    .flatten()
                    .unwrap_or(true),
                hide_nav_buttons: ctx
                    .config
                    .get::<bool>("preprocessor.util-private-page.hide_nav_buttons")
                    .ok()
                    .flatten()
                    .unwrap_or(true),
            });

        Self { banner, private }
    }

    fn process_private(&self, book: &mut Book) {
        let cfg = match &self.private {
            Some(c) => c,
            None => return,
        };

        // ── Pass 1: collect page hrefs in render order; mark private ones ──
        //
        // mdBook's renderer flattens the book via `book.chapters()` (pre-order
        // of non-draft chapters) and emits one page per chapter at
        // `path.with_extension("html")`. This same order drives the sidebar
        // and the prev/next nav, so we reproduce it here exactly.
        let mut all: Vec<String> = Vec::new();
        let mut private: Vec<String> = Vec::new();
        for item in book.iter() {
            if let BookItem::Chapter(ch) = item {
                if let Some(p) = &ch.path {
                    let key = p.with_extension("html").to_string_lossy().replace('\\', "/");
                    all.push(key.clone());
                    if PRIVATE_RE.is_match(&ch.content) {
                        private.push(key);
                    }
                }
            }
        }

        if private.is_empty() {
            return;
        }

        let script = build_private_script(&all, &private, cfg);

        // ── Pass 2: strip markers and inject the script into every chapter ──
        book.for_each_mut(|item| {
            if let BookItem::Chapter(ch) = item {
                ch.content = PRIVATE_RE.replace_all(&ch.content, "").to_string();
                // Blank line required between the raw HTML block and the
                // following markdown, otherwise pulldown-cmark would treat the
                // heading text as part of the HTML block.
                ch.content = format!("{script}\n\n{}", ch.content);
            }
        });
    }

    fn process_banner(&self, book: &mut Book) {
        let cfg = match &self.banner {
            Some(c) => c,
            None => return,
        };

        // ── Pass 1: collect banner contents and source paths ──────
        //
        // Each banner's relative links (e.g. `./foo.html`) are written
        // relative to the chapter where the banner was defined. We track
        // the source chapter's path so we can adjust links later.
        let mut banners: Vec<(String, std::path::PathBuf)> = Vec::new();
        for item in book.iter() {
            if let BookItem::Chapter(chap) = item {
                if let Some(chapter_path) = &chap.path {
                    for caps in BANNER_RE.captures_iter(&chap.content) {
                        if let Some(m) = caps.name("content") {
                            banners.push((m.as_str().to_string(), chapter_path.clone()));
                        }
                    }
                }
            }
        }

        if banners.is_empty() {
            return;
        }

        // ── Build HTML for each banner ─────────────────────────────
        let banner_html_blocks: Vec<String> = banners
            .iter()
            .enumerate()
            .map(|(i, (content, _))| build_banner(content, i, cfg.sticky))
            .collect();
        let banners_html = banner_html_blocks.join("\n");

        let header = if cfg.sticky {
            BANNER_CSS.to_string()
        } else {
            format!("{BANNER_CSS}\n{BANNER_SCRIPT}")
        };

        // ── Pass 2: mutate every chapter ───────────────────────────
        //
        // Banner content is extracted once and injected into every chapter.
        // Relative links in the banner (e.g. `./foo.html`) are written
        // relative to the chapter where the banner markdown was defined.
        // When the banner appears in a different chapter at a different
        // directory depth, those links break (404).
        //
        // Fix: compute the relative path from the source chapter's
        // directory to the book root, then prepend that to each link.
        // For example, if the banner is defined in `2026/foreword.md`
        // and has a link `./foo.html`, the adjusted link becomes
        // `2026/foo.html` (root-relative), which works from any chapter.
        book.for_each_mut(|item| {
            if let BookItem::Chapter(chap) = item {
                // Remove banner markers from this chapter.
                chap.content = BANNER_RE.replace_all(&chap.content, "").to_string();

                // Adjust relative links in banner HTML to be root-relative.
                let source_path = banners.first().map(|(_, p)| p.clone());
                let adjusted_banners = adjust_banner_links(&banners_html, source_path.as_ref(), &chap.path);

                // Inject banner HTML into this chapter.
                match cfg.position.as_str() {
                    "bottom" => {
                        chap.content
                            .push_str(&format!("\n\n{header}\n{adjusted_banners}"));
                    }
                    _ => {
                        // Blank line required between HTML block and markdown,
                        // otherwise pulldown-cmark treats the heading text as
                        // part of the raw HTML block.
                        chap.content =
                            format!("{header}\n{adjusted_banners}\n\n{}", chap.content);
                    }
                }
            }
        });
    }
}

impl Preprocessor for UtilsPreprocessor {
    fn name(&self) -> &str {
        "mdutils"
    }

    fn run(&self, _ctx: &PreprocessorContext, mut book: Book) -> Result<Book, Error> {
        self.process_banner(&mut book);
        self.process_private(&mut book);
        Ok(book)
    }

    fn supports_renderer(&self, renderer: &str) -> Result<bool, Error> {
        Ok(renderer == "html")
    }
}

// ── Banner HTML builder ───────────────────────────────────────────────

fn render_markdown(md: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(md, options);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

fn build_banner(md_content: &str, id: usize, sticky: bool) -> String {
    let body = render_markdown(md_content);
    let close_btn = if sticky {
        String::new()
    } else {
        r#"<button class="util-banner-close" aria-label="Close banner">&times;</button>"#.to_string()
    };
    format!(
        r#"<div class="util-banner" id="util-banner-{id}">
  <div class="util-banner-content">{body}</div>
  {close_btn}
</div>"#
    )
}

// ── Banner link adjuster ────────────────────────────────────────────
//
// Banner content is extracted once from the source markdown and rendered
// to HTML. The relative links (e.g. `./foo.html`) are written relative to
// the chapter where the banner was defined (the "source chapter"). When
// the same banner is injected into chapters at different directory depths,
// those links break (404). Additionally, mdBook copies the first chapter
// to `index.html` at the book root — relative links like `./foo.html`
// resolve from the root, not from the chapter's directory.
//
// Solution: strip the `./` prefix and prepend the source chapter's
// directory, making links root-relative (e.g. `2026/foo.html`). These
// resolve correctly from ANY page — the chapter itself, the root
// `index.html`, or any other depth in the book.
fn adjust_banner_links(
    html: &str,
    source_path: Option<&std::path::PathBuf>,
    _chapter_path: &Option<std::path::PathBuf>,
) -> String {
    // Get the source chapter's directory (e.g. "2026" from "2026/foreword.md").
    let source_dir = match source_path.and_then(|p| p.parent()) {
        Some(dir) if !dir.as_os_str().is_empty() => {
            format!("/{}/", dir.to_string_lossy())
        }
        _ => return html.to_string(),
    };

    // Rewrite `./foo.html` → `/2026/foo.html` (absolute from server root).
    BANNER_LINK_RE
        .replace_all(html, |caps: &regex::Captures| {
            format!("{}{}{}", &caps[1], source_dir, &caps[2])
        })
        .to_string()
}

// ── Private-page script builder ────────────────────────────────────────
//
// Builds the runtime script injected into every chapter when at least one
// page is marked private. The script runs on `DOMContentLoaded` (after
// mdBook's `toc.js` has populated the sidebar) and:
//
//   1. Removes the sidebar <li> whose <a href> points at a private page;
//   2. Rewrites the prev/next links to skip over private pages.
//
// Identification uses mdBook's global `path_to_root` (`const` declared in the
// page <head>, readable by any subsequent script) to resolve each baked-in
// relative href (e.g. `sub/secret.html`) to the same absolute URL mdBook's
// `toc.js` computes for the sidebar links, so the two always match.
fn build_private_script(all: &[String], private: &[String], cfg: &PrivateConfig) -> String {
    let all_json = serde_json::to_string(all).unwrap_or_default();
    let private_json = serde_json::to_string(private).unwrap_or_default();
    format!(
        r##"<script>
(function () {{
  var ALL = {all_json};
  var PRIVATE = {private_json};
  var HIDE_SIDEBAR = {hide_sidebar};
  var HIDE_NAV = {hide_nav};
  function toAbs(key) {{ return new URL(path_to_root + key, document.baseURI).href; }}
  var privAbs = {{}};
  for (var i = 0; i < PRIVATE.length; i++) privAbs[toAbs(PRIVATE[i])] = true;
  document.addEventListener('DOMContentLoaded', function () {{
    var ci = -1, cur = location.href;
    for (var i = 0; i < ALL.length; i++) {{
      if (toAbs(ALL[i]) === cur) {{ ci = i; break; }}
    }}
    // index.html is an alias copy of the first chapter.
    if (ci < 0 && /\/index\.html$/.test(cur)) ci = 0;
    if (HIDE_SIDEBAR) {{
      var links = document.querySelectorAll('#mdbook-sidebar a');
      for (var j = 0; j < links.length; j++) {{
        if (privAbs[links[j].href]) {{
          var li = links[j].closest('li');
          if (li) li.parentNode.removeChild(li);
        }}
      }}
    }}
    if (HIDE_NAV && ci >= 0) {{
      var p = ci - 1; while (p >= 0 && privAbs[toAbs(ALL[p])]) p--;
      var n = ci + 1; while (n < ALL.length && privAbs[toAbs(ALL[n])]) n++;
      var prevs = document.querySelectorAll('a[rel^="prev"]');
      for (var k = 0; k < prevs.length; k++) {{
        if (p >= 0) prevs[k].setAttribute('href', toAbs(ALL[p]));
        else prevs[k].parentNode.removeChild(prevs[k]);
      }}
      var nexts = document.querySelectorAll('a[rel^="next"]');
      for (var m = 0; m < nexts.length; m++) {{
        if (n < ALL.length) nexts[m].setAttribute('href', toAbs(ALL[n]));
        else nexts[m].parentNode.removeChild(nexts[m]);
      }}
    }}
  }});
}})();
</script>
"##,
        all_json = all_json,
        private_json = private_json,
        hide_sidebar = cfg.hide_sidebar,
        hide_nav = cfg.hide_nav_buttons
    )
}
