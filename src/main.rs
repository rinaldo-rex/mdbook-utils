// Architecture Decision Records: see adr.md at the repo root.
// Key conventions:
//   - One [preprocessor.util-*] section per utility (ADR-001).
//   - No name-prefix on keys inside a utility section (ADR-002).
//   - All utilities share this single binary (ADR-003).
//   - Global banner injection (ADR-004).
//   - Theme-aware CSS via mdBook variables (ADR-005).
//   - localStorage + date-stamp dismiss pattern (ADR-006).

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
}

// Keys are plain (no prefix) — scoped by [preprocessor.util-banner]  (ADR-001, ADR-002)
struct BannerConfig {
    position: String, // "top" or "bottom"
    sticky: bool,
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

        Self { banner }
    }

    fn process_banner(&self, book: &mut Book) {
        let cfg = match &self.banner {
            Some(c) => c,
            None => return,
        };

        // ── Pass 1: collect banner contents from all chapters ──────
        let mut banners: Vec<String> = Vec::new();
        for item in book.iter() {
            if let BookItem::Chapter(chap) = item {
                for caps in BANNER_RE.captures_iter(&chap.content) {
                    if let Some(m) = caps.name("content") {
                        banners.push(m.as_str().to_string());
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
            .map(|(i, content)| build_banner(content, i, cfg.sticky))
            .collect();
        let banners_html = banner_html_blocks.join("\n");

        let header = if cfg.sticky {
            BANNER_CSS.to_string()
        } else {
            format!("{BANNER_CSS}\n{BANNER_SCRIPT}")
        };

        // ── Pass 2: mutate every chapter ───────────────────────────
        book.for_each_mut(|item| {
            if let BookItem::Chapter(chap) = item {
                // Remove banner markers from this chapter.
                chap.content = BANNER_RE.replace_all(&chap.content, "").to_string();

                // Inject banner HTML into this chapter.
                match cfg.position.as_str() {
                    "bottom" => {
                        chap.content.push_str(&format!("\n\n{header}\n{banners_html}"));
                    }
                    _ => {
                        // Blank line required between HTML block and markdown,
                        // otherwise pulldown-cmark treats the heading text as
                        // part of the raw HTML block.
                        chap.content = format!("{header}\n{banners_html}\n\n{}", chap.content);
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
