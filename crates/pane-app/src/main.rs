//! Pane v0.2: a GPU window that renders either a huge file (lazy, virtualized)
//! or a colored diff, with an optional blocking review verdict.
//!
//! Modes:
//!   pane <file>                     open the window (prints lazy-open metrics)
//!   pane --stat <file>              print metrics only, no window (headless bench)
//!   pane --review <file>            blocking review; verdict → exit code (0/1/2)
//!   pane --review --json <file>     also print {"verdict":"..."} to stdout
//!   pane --diff <old> <new>         view a unified colored diff
//!   pane --review --diff <old> <new>  review a diff with a verdict
//!
//! Review verdict keys: A/Enter approve · R/Esc reject · Q/close cancel.
//! Exit codes: 0 approved · 1 rejected · 2 cancelled.

use std::sync::Arc;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Wrap,
};
use pane_core::TextFile;
use similar::{ChangeTag, TextDiff};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

mod syntax;

/// Max file size we syntax-highlight (bigger files stay plain & lazily loaded).
const HL_MAX_BYTES: usize = 4_000_000;

// Visual constants (dark theme, centralized — see PRD: no config in v1).
const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 20.0;
const PADDING: f32 = 8.0;
// Dark theme following Material dark-theme rules (near-neutral ~#121212 surface,
// desaturated accents, 87/60/38% emphasis, ≥3:1 for the dim gutter). See
// docs/theme-proposal.md for rationale and contrast ratios.
const BG: wgpu::Color = wgpu::Color {
    r: 0.07451,
    g: 0.07451,
    b: 0.08627,
    a: 1.0,
};
const FG: Color = Color::rgb(0xe2, 0xe4, 0xe9);
const FG_DIM: Color = Color::rgb(0x8f, 0x95, 0xa0);
const ADD: Color = Color::rgb(0x8f, 0xc9, 0xa6);
const DEL: Color = Color::rgb(0xe7, 0x90, 0x98);
const ACCENT: Color = Color::rgb(0x9b, 0xbc, 0xf2);
const GUTTER_FG: Color = Color::rgb(0x6c, 0x72, 0x7c);
const GUTTER_GAP: f32 = 12.0;
const HL: Color = Color::rgb(0xff, 0xd7, 0x6e); // current search match
const FOOTER: &str = "  REVIEW    approve: A / Enter      reject: R / Esc      cancel: Q";

/// The outcome of a review session.
#[derive(Clone, Copy)]
enum Verdict {
    Approved,
    Rejected,
    Cancelled,
}

impl Verdict {
    fn label(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::Rejected => "rejected",
            Verdict::Cancelled => "cancelled",
        }
    }
    fn exit_code(self) -> i32 {
        match self {
            Verdict::Approved => 0,
            Verdict::Rejected => 1,
            Verdict::Cancelled => 2,
        }
    }
}

/// One rendered line: ordered colored pieces (no trailing newline) plus the
/// line number for the gutter (`None` for separators and file headers).
#[derive(Clone)]
struct ViewLine {
    spans: Vec<(String, Color)>,
    num: Option<usize>,
}

impl ViewLine {
    /// A single-color line (used for plain text and diff lines).
    fn plain(text: String, color: Color, num: Option<usize>) -> Self {
        ViewLine {
            spans: vec![(text, color)],
            num,
        }
    }
}

/// What the window renders: a lazily-indexed file, or precomputed styled lines
/// (a diff, a changeset, or a syntax-highlighted file).
enum Source {
    File(TextFile),
    Lines(Vec<ViewLine>),
}

impl Source {
    /// Clamps the scroll position so the last screenful stays full — you can
    /// never scroll into empty space past the end of the content.
    ///
    /// For a lazily-indexed file this indexes one screen ahead of `idx`; if
    /// that hits EOF the index is complete, so the true line count is known
    /// and we can clamp exactly. While more lines remain, no clamp is needed.
    fn clamp_scroll(&self, idx: usize, visible: usize) -> usize {
        match self {
            Source::File(f) => {
                // Index one screen past the target so we know what's below.
                f.clamp_to_line(idx.saturating_add(visible));
                let max = if f.is_complete() {
                    // Whole file known: keep the last screenful full.
                    f.indexed_line_count().saturating_sub(visible)
                } else {
                    // Still discovering: allow scrolling down to the last known
                    // line (never into blank), and it grows as we index more.
                    f.indexed_line_count().saturating_sub(1)
                };
                idx.min(max)
            }
            Source::Lines(d) => idx.min(d.len().saturating_sub(visible)),
        }
    }

    /// Visible lines `[start, start+count)`.
    fn visible(&self, start: usize, count: usize) -> Vec<ViewLine> {
        match self {
            Source::File(f) => {
                let mut out = Vec::with_capacity(count);
                for i in start..start + count {
                    match f.line(i) {
                        Some(l) => out.push(ViewLine::plain(l.into_owned(), FG, Some(i + 1))),
                        None => break,
                    }
                }
                out
            }
            Source::Lines(d) => d.iter().skip(start).take(count).cloned().collect(),
        }
    }

    /// Lines known so far (exact for `Lines`; grows while a lazy file indexes).
    fn total_lines(&self) -> usize {
        match self {
            Source::File(f) => f.indexed_line_count(),
            Source::Lines(d) => d.len(),
        }
    }

    /// Searches all content, returning matching line indices (capped at `max`).
    fn search(&self, pat: &str, use_regex: bool, max: usize) -> Vec<usize> {
        match self {
            Source::File(f) => f.search(pat, use_regex, max),
            Source::Lines(d) => {
                let mut hits = Vec::new();
                if pat.is_empty() || max == 0 {
                    return hits;
                }
                let re = if use_regex {
                    match regex::Regex::new(pat) {
                        Ok(r) => Some(r),
                        Err(_) => return hits,
                    }
                } else {
                    None
                };
                for (i, l) in d.iter().enumerate() {
                    let line_text: String = l.spans.iter().map(|s| s.0.as_str()).collect();
                    let m = match &re {
                        Some(r) => r.is_match(&line_text),
                        None => line_text.contains(pat),
                    };
                    if m {
                        hits.push(i);
                        if hits.len() >= max {
                            break;
                        }
                    }
                }
                hits
            }
        }
    }
}

/// Runs `git` with `args`, returning stdout on success.
fn git_output(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Builds a colored review of the current git working-tree changes.
///
/// This is what agents call: no manifest, no helper script — just run
/// `pane --review --git` inside the repo the agent just modified.
fn build_git_changeset() -> (Vec<ViewLine>, usize) {
    if git_output(&["rev-parse", "--is-inside-work-tree"]).is_none() {
        eprintln!("not a git repository (run inside one, or use --changeset/--diff)");
        std::process::exit(2);
    }
    let status = git_output(&["status", "--porcelain", "--untracked-files=all"]).unwrap_or_default();

    let mut out = Vec::new();
    let mut count = 0usize;
    for line in status.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[0..2];
        let rest = line[3..].trim();

        // Renames arrive as `old -> new`.
        let (old_path, new_path) = match rest.split_once(" -> ") {
            Some((a, b)) => (unquote(a), unquote(b)),
            None => {
                let p = unquote(rest);
                (p.clone(), p)
            }
        };

        if std::path::Path::new(&new_path).is_dir() {
            continue;
        }

        let untracked = code == "??";
        let added = untracked || code.starts_with('A');
        let deleted = code.contains('D');

        let old = if added {
            String::new()
        } else {
            git_output(&["show", &format!("HEAD:{old_path}")]).unwrap_or_default()
        };
        let new = if deleted {
            String::new()
        } else {
            std::fs::read_to_string(&new_path).unwrap_or_default()
        };

        let tag = if added {
            " (new)"
        } else if deleted {
            " (deleted)"
        } else {
            ""
        };

        if count > 0 {
            out.push(ViewLine::plain(String::new(), FG_DIM, None));
        }
        out.push(ViewLine::plain(
            format!("──── {new_path}{tag} ────"),
            ACCENT,
            None,
        ));
        out.extend(build_diff(&old, &new));
        count += 1;
    }

    if count == 0 {
        eprintln!("no changes to review");
        std::process::exit(0);
    }
    eprintln!("reviewing {count} changed file(s)");
    (out, count)
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').to_string()
}

/// Builds a colored review of a multi-file changeset from a TSV manifest.
///
/// Each manifest line is `old_path<TAB>new_path<TAB>label` (label optional; an
/// empty path means "no such side" — a new or deleted file). Paths point to
/// files on disk. Returns the combined diff lines and the number of files.
fn build_changeset(manifest_path: &str) -> (Vec<ViewLine>, usize) {
    let raw = read_or_exit(manifest_path);
    let mut out = Vec::new();
    let mut count = 0usize;
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let old_path = parts.next().unwrap_or("");
        let new_path = parts.next().unwrap_or("");
        let label = parts
            .next()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                short_name(if !new_path.is_empty() { new_path } else { old_path }).to_string()
            });

        let old = if old_path.is_empty() {
            String::new()
        } else {
            read_or_exit(old_path)
        };
        let new = if new_path.is_empty() {
            String::new()
        } else {
            read_or_exit(new_path)
        };

        if count > 0 {
            out.push(ViewLine::plain(String::new(), FG_DIM, None));
        }
        out.push(ViewLine::plain(format!("──── {label} ────"), ACCENT, None));
        out.extend(build_diff(&old, &new));
        count += 1;
    }
    (out, count)
}

/// Turns a whole file's text into syntax-highlighted lines (colored spans per
/// line), using the Tree-sitter ranges from `syntax::highlight`.
fn build_highlighted(text: &str, lang: syntax::Lang) -> Vec<ViewLine> {
    let segs = syntax::highlight(text, lang); // sorted, non-overlapping (Range, Color)
    let mut out = Vec::new();
    let mut pos = 0usize; // byte offset of the current line's start
    let mut num = 1usize;
    let mut seg = 0usize; // cursor into `segs`, advances monotonically

    for piece in text.split_inclusive('\n') {
        let ls = pos;
        let le = if piece.ends_with('\n') {
            ls + piece.len() - 1
        } else {
            ls + piece.len()
        };
        while seg < segs.len() && segs[seg].0.end <= ls {
            seg += 1;
        }

        let mut spans: Vec<(String, Color)> = Vec::new();
        let mut x = ls;
        let mut k = seg;
        while x < le {
            if k < segs.len() && segs[k].0.end <= x {
                k += 1;
                continue;
            }
            let (end, color) = if k < segs.len() && segs[k].0.start <= x {
                (segs[k].0.end.min(le), segs[k].1)
            } else {
                let next = segs.get(k).map(|s| s.0.start).unwrap_or(le).min(le);
                (next, FG)
            };
            if let Some(s) = text.get(x..end) {
                spans.push((s.to_string(), color));
            }
            x = end;
        }
        if spans.is_empty() {
            spans.push((String::new(), FG));
        }
        out.push(ViewLine { spans, num: Some(num) });
        pos += piece.len();
        num += 1;
    }
    out
}

/// Builds a unified line diff, each line prefixed and colored (+/-/context).
fn build_diff(old: &str, new: &str) -> Vec<ViewLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut out = Vec::new();
    for change in diff.iter_all_changes() {
        let (prefix, color) = match change.tag() {
            ChangeTag::Delete => ("- ", DEL),
            ChangeTag::Insert => ("+ ", ADD),
            ChangeTag::Equal => ("  ", FG_DIM),
        };
        let value = change.value();
        let line = value.strip_suffix('\n').unwrap_or(value);
        // Number lines by their position in the NEW file; deleted lines have no
        // counterpart there, so their gutter stays blank.
        out.push(ViewLine::plain(
            format!("{prefix}{line}"),
            color,
            change.new_index().map(|i| i + 1),
        ));
    }
    out
}

fn main() {
    // Flag parsing: any number of `--flags` plus positional paths.
    let mut review = false;
    let mut json = false;
    let mut stat = false;
    let mut diff = false;
    let mut changeset = false;
    let mut git = false;
    let mut positionals: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("pane {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--review" => review = true,
            "--json" => json = true,
            "--stat" => stat = true,
            "--diff" => diff = true,
            "--changeset" => changeset = true,
            "--git" => git = true,
            s if !s.starts_with("--") => positionals.push(s.to_string()),
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
        }
    }

    let (source, title) = if git {
        let (lines, n) = build_git_changeset();
        (
            Source::Lines(lines),
            format!("Pane review — {n} changed file(s)"),
        )
    } else if changeset {
        let [manifest] = match positionals.as_slice() {
            [m] => [m.clone()],
            _ => {
                eprintln!("usage: pane [--review] --changeset <manifest.tsv>");
                std::process::exit(2);
            }
        };
        let (lines, n) = build_changeset(&manifest);
        (Source::Lines(lines), format!("Pane review — changeset ({n} files)"))
    } else if diff {
        let [old_path, new_path] = match positionals.as_slice() {
            [a, b] => [a.clone(), b.clone()],
            _ => {
                eprintln!("usage: pane [--review] --diff <old> <new>");
                std::process::exit(2);
            }
        };
        let old = read_or_exit(&old_path);
        let new = read_or_exit(&new_path);
        let title = format!(
            "Pane diff — {} ↔ {}",
            short_name(&old_path),
            short_name(&new_path)
        );
        (Source::Lines(build_diff(&old, &new)), title)
    } else {
        let [path] = match positionals.as_slice() {
            [p] => [p.clone()],
            _ => {
                eprintln!("usage: pane [--stat] [--review [--json]] <file>");
                std::process::exit(2);
            }
        };
        let file = match load_file(&path, /* report */ true) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("could not open {path}: {e}");
                std::process::exit(1);
            }
        };
        if stat {
            return;
        }
        let title = if review {
            format!("Pane review — {}", short_name(&path))
        } else {
            format!("Pane — {}", short_name(&path))
        };
        // Syntax-highlight small files of a known language; huge files (logs)
        // stay plain and lazily loaded.
        let source = match syntax::Lang::from_path(&path) {
            Some(lang) if file.byte_len() <= HL_MAX_BYTES => {
                match std::fs::read_to_string(&path) {
                    Ok(text) => Source::Lines(build_highlighted(&text, lang)),
                    Err(_) => Source::File(file),
                }
            }
            _ => Source::File(file),
        };
        (source, title)
    };

    let mut app = Application {
        source,
        title,
        review,
        verdict: None,
        state: None,
    };
    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut app).unwrap();

    // In review mode the exit code carries the verdict back to the caller (agent).
    if review {
        let verdict = app.verdict.unwrap_or(Verdict::Cancelled);
        eprintln!("verdict: {}", verdict.label());
        if json {
            println!("{{\"verdict\":\"{}\"}}", verdict.label());
        }
        std::process::exit(verdict.exit_code());
    }
}

fn read_or_exit(path: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not read {path}: {e}");
            std::process::exit(1);
        }
    }
}

/// Opens a file via mmap and (if `report`) prints lazy-open metrics.
fn load_file(path: &str, report: bool) -> std::io::Result<TextFile> {
    use std::time::Instant;

    let t0 = Instant::now();
    let file = TextFile::open(path)?;
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;

    if report {
        let t1 = Instant::now();
        let mut touched = 0usize;
        for i in 0..60 {
            match file.line_bytes(i) {
                Some(b) => touched += b.len(),
                None => break,
            }
        }
        let first_view_ms = t1.elapsed().as_secs_f64() * 1000.0;

        println!("─ Pane · lazy open metrics ───────────────────────");
        println!("file:              {path}");
        println!("size:              {:.1} MB", file.byte_len() as f64 / 1e6);
        println!("open (mmap only):  {open_ms:.3} ms");
        println!(
            "first viewport:    {first_view_ms:.3} ms  ({touched} bytes, {} lines indexed)",
            file.indexed_line_count()
        );
        println!("index (heap):      {:.4} MB", file.index_heap_bytes() as f64 / 1e6);
        println!("peak RSS:          {:.1} MB", peak_rss_bytes() as f64 / 1e6);
        println!("──────────────────────────────────────────────────");
    }
    Ok(file)
}

fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

struct Application {
    source: Source,
    title: String,
    review: bool,
    verdict: Option<Verdict>,
    state: Option<WindowState>,
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(winit::dpi::LogicalSize::new(1000.0, 700.0));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.state = Some(pollster::block_on(WindowState::new(
            window,
            event_loop,
            self.review,
        )));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let review = self.review;
        let source = &self.source;
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                if review && self.verdict.is_none() {
                    self.verdict = Some(Verdict::Cancelled);
                }
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                state.surface_config.width = size.width.max(1);
                state.surface_config.height = size.height.max(1);
                state.surface.configure(&state.device, &state.surface_config);
                // Re-clamp: the number of visible lines changed, so the old
                // scroll position may now be past the end (or leave blank space).
                state.scroll = source.clamp_scroll(state.scroll, state.page_lines());
                state.window.request_redraw();
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, lines) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 40.0, -(y * 3.0) as i64),
                    MouseScrollDelta::PixelDelta(p) => {
                        (p.x as f32, -(p.y as f32 / LINE_HEIGHT) as i64)
                    }
                };
                let next = (state.scroll as i64 + lines).max(0) as usize;
                let vis = state.page_lines();
                state.scroll = source.clamp_scroll(next, vis);
                // Pan horizontally; the upper bound is clamped at layout time,
                // when the widths of the visible lines are known.
                state.hscroll = (state.hscroll - dx).max(0.0);
                state.window.request_redraw();
            }

            WindowEvent::KeyboardInput { event: key, .. } if key.state == ElementState::Pressed => {
                // 1) While typing a query, every key edits the search — nothing else.
                if state.search_input {
                    match &key.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            state.search_input = false;
                            state.query.clear();
                            state.matches.clear();
                        }
                        Key::Named(NamedKey::Enter) => {
                            state.search_input = false;
                            state.matches = source.search(&state.query, false, 100_000);
                            state.match_idx = 0;
                            if let Some(&t) = state.matches.first() {
                                let vis = state.page_lines();
                                // Center the match rather than pinning it to the top.
                                state.scroll = source.clamp_scroll(t.saturating_sub(vis / 2), vis);
                            }
                        }
                        Key::Named(NamedKey::Backspace) => {
                            state.query.pop();
                        }
                        Key::Named(NamedKey::Space) => state.query.push(' '),
                        Key::Character(c) => state.query.push_str(c),
                        _ => {}
                    }
                    state.window.request_redraw();
                    return;
                }

                // 2) '/' opens the search input.
                if matches!(&key.logical_key, Key::Character(c) if c.as_str() == "/") {
                    state.search_input = true;
                    state.query.clear();
                    state.window.request_redraw();
                    return;
                }

                // 3) With an active search: Esc clears it (does NOT quit), and
                //    n / N jump between matches (centering each match on screen).
                if !state.matches.is_empty() {
                    if matches!(&key.logical_key, Key::Named(NamedKey::Escape)) {
                        state.query.clear();
                        state.matches.clear();
                        state.window.request_redraw();
                        return;
                    }
                    let dir = match &key.logical_key {
                        Key::Character(c) if c.as_str() == "n" => Some(1i64),
                        Key::Character(c) if c.as_str() == "N" => Some(-1i64),
                        _ => None,
                    };
                    if let Some(d) = dir {
                        let len = state.matches.len() as i64;
                        state.match_idx = (((state.match_idx as i64 + d) % len + len) % len) as usize;
                        let t = state.matches[state.match_idx];
                        let vis = state.page_lines();
                        state.scroll = source.clamp_scroll(t.saturating_sub(vis / 2), vis);
                        state.window.request_redraw();
                        return;
                    }
                }

                // 4) Review verdict keys.
                if review {
                    let verdict = match &key.logical_key {
                        Key::Named(NamedKey::Enter) => Some(Verdict::Approved),
                        Key::Named(NamedKey::Escape) => Some(Verdict::Rejected),
                        Key::Character(c) if c.eq_ignore_ascii_case("a") => Some(Verdict::Approved),
                        Key::Character(c) if c.eq_ignore_ascii_case("r") => Some(Verdict::Rejected),
                        Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(Verdict::Cancelled),
                        _ => None,
                    };
                    if let Some(v) = verdict {
                        self.verdict = Some(v);
                        event_loop.exit();
                        return;
                    }
                } else if matches!(key.logical_key, Key::Named(NamedKey::Escape)) {
                    event_loop.exit();
                    return;
                }

                // 5) Scrolling works in every mode so you can read before deciding.
                let vis = state.page_lines();
                let page = vis.saturating_sub(2);
                let s = state.scroll;
                state.scroll = match &key.logical_key {
                    Key::Named(NamedKey::ArrowDown) => source.clamp_scroll(s + 1, vis),
                    Key::Named(NamedKey::ArrowUp) => s.saturating_sub(1),
                    Key::Named(NamedKey::PageDown) => source.clamp_scroll(s + page, vis),
                    Key::Named(NamedKey::PageUp) => s.saturating_sub(page),
                    Key::Named(NamedKey::Home) => {
                        state.hscroll = 0.0;
                        0
                    }
                    Key::Named(NamedKey::End) => source.clamp_scroll(usize::MAX, vis),
                    _ => s,
                };
                match &key.logical_key {
                    Key::Named(NamedKey::ArrowRight) => state.hscroll += 60.0 * state.scale,
                    Key::Named(NamedKey::ArrowLeft) => {
                        state.hscroll = (state.hscroll - 60.0 * state.scale).max(0.0);
                    }
                    _ => {}
                }
                state.window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                state.render(source);
            }

            _ => {}
        }
    }
}

struct WindowState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,

    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffer: Buffer,
    footer_buffer: Buffer,
    gutter_buffer: Buffer,

    scroll: usize,
    /// Horizontal scroll offset in physical pixels (no-wrap → long lines pan).
    hscroll: f32,
    scale: f32,
    review: bool,
    quads: QuadRenderer,

    // Search state.
    search_input: bool,   // typing a query
    query: String,
    matches: Vec<usize>,  // matching line indices
    match_idx: usize,     // which match is current

    // Keep the window last so it drops after the surface (avoids a wgpu crash).
    window: Arc<Window>,
}

impl WindowState {
    async fn new(window: Arc<Window>, event_loop: &ActiveEventLoop, review: bool) -> Self {
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("no GPU adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("no device");

        let surface = instance.create_surface(window.clone()).expect("surface");
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);

        let metrics = Metrics::new(FONT_SIZE * scale, LINE_HEIGHT * scale);
        let mut text_buffer = Buffer::new(&mut font_system, metrics);
        let mut footer_buffer = Buffer::new(&mut font_system, metrics);
        let mut gutter_buffer = Buffer::new(&mut font_system, metrics);
        // No word-wrap: one logical line = one visual row. This keeps the gutter
        // line numbers aligned with the content and makes the visible-line count
        // exact, so scrolling reaches the last line. (Horizontal scroll for long
        // lines is a separate future feature; for now they clip at the edge.)
        text_buffer.set_wrap(Wrap::None);
        footer_buffer.set_wrap(Wrap::None);
        gutter_buffer.set_wrap(Wrap::None);

        let quads = QuadRenderer::new(&device, format);

        Self {
            device,
            queue,
            surface,
            surface_config,
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffer,
            footer_buffer,
            gutter_buffer,
            quads,
            scroll: 0,
            hscroll: 0.0,
            scale,
            review,
            search_input: false,
            query: String::new(),
            matches: Vec::new(),
            match_idx: 0,
            window,
        }
    }

    /// How many lines to DRAW: enough to fill the viewport plus one partial row
    /// at the bottom. Used only for rendering, never for clamping.
    fn visible_lines(&self) -> usize {
        let h = self.surface_config.height as f32;
        ((h / (LINE_HEIGHT * self.scale)).ceil() as usize) + 1
    }

    /// How many lines are FULLY visible (floor). Used for scroll clamping and
    /// paging, so the last line lands exactly at the bottom — not clipped below
    /// it. Using the draw count here would leave the bottom lines unreachable,
    /// badly in a small window (where the over-count is proportionally large).
    fn page_lines(&self) -> usize {
        let h = self.surface_config.height as f32;
        ((h / (LINE_HEIGHT * self.scale)).floor() as usize).max(1)
    }

    /// Lays out the gutter (right-aligned line numbers) and the content, and
    /// returns how far from the left edge the content starts.
    fn layout_text(&mut self, lines: &[ViewLine]) -> f32 {
        let width = self.surface_config.width as f32;
        let height = self.surface_config.height as f32;
        let mono = Attrs::new().family(Family::Monospace);

        // Gutter: right-align to the widest number currently on screen.
        let digits = lines
            .iter()
            .filter_map(|l| l.num)
            .max()
            .map(|m| m.to_string().len())
            .unwrap_or(1);
        let mut gutter = String::new();
        for l in lines {
            if let Some(n) = l.num {
                gutter.push_str(&format!("{n:>digits$}", digits = digits));
            }
            gutter.push('\n');
        }
        self.gutter_buffer.set_size(Some(width), Some(height));
        self.gutter_buffer
            .set_text(&gutter, &mono, Shaping::Basic, None);
        self.gutter_buffer
            .shape_until_scroll(&mut self.font_system, false);

        // Measure the real shaped width so the content never overlaps it.
        let gutter_w = self
            .gutter_buffer
            .layout_runs()
            .map(|r| r.line_w)
            .fold(0.0f32, f32::max);
        let content_left = PADDING + gutter_w + GUTTER_GAP;

        // One rich-text run per colored span, with an explicit newline between
        // logical lines.
        let mut rich: Vec<(&str, Attrs)> = Vec::new();
        for l in lines {
            for (text, color) in &l.spans {
                rich.push((
                    text.as_str(),
                    Attrs::new().family(Family::Monospace).color(*color),
                ));
            }
            rich.push(("\n", Attrs::new().family(Family::Monospace)));
        }
        // Width None: with Wrap::None lines never wrap, and leaving the width
        // unbounded avoids culling glyphs we pan to; TextArea bounds clip.
        self.text_buffer.set_size(None, Some(height));
        self.text_buffer
            .set_rich_text(rich, &mono, Shaping::Advanced, None);
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);

        // Clamp the horizontal pan to the widest visible line.
        let max_line_w = self
            .text_buffer
            .layout_runs()
            .map(|r| r.line_w)
            .fold(0.0f32, f32::max);
        let avail = (width - content_left).max(1.0);
        self.hscroll = self.hscroll.clamp(0.0, (max_line_w - avail).max(0.0));

        content_left
    }

    fn render(&mut self, source: &Source) {
        let want = self.visible_lines();
        let mut lines = source.visible(self.scroll, want);

        // Highlight the current search match if it is on screen.
        if let Some(&target) = self.matches.get(self.match_idx) {
            if target >= self.scroll && target < self.scroll + lines.len() {
                for span in &mut lines[target - self.scroll].spans {
                    span.1 = HL;
                }
            }
        }

        let content_left = self.layout_text(&lines);

        let width = self.surface_config.width as f32;
        let height = self.surface_config.height as f32;

        // Bottom status bar: search state takes over; otherwise the review footer.
        let status: Option<(String, Color)> = if self.search_input {
            Some((format!("/{}_", self.query), ACCENT))
        } else if !self.matches.is_empty() {
            Some((
                format!(
                    "/{}    {}/{}    n next · N prev · / new · Esc clear",
                    self.query,
                    self.match_idx + 1,
                    self.matches.len()
                ),
                FG_DIM,
            ))
        } else if self.review {
            Some((FOOTER.to_string(), ACCENT))
        } else {
            None
        };

        let footer_h = if status.is_some() {
            (LINE_HEIGHT * self.scale).ceil() + 6.0
        } else {
            0.0
        };
        let content_bottom = (height - footer_h) as i32;

        if let Some((text, _)) = &status {
            self.footer_buffer.set_size(Some(width), Some(footer_h.max(1.0)));
            self.footer_buffer.set_text(
                text,
                &Attrs::new().family(Family::Monospace),
                Shaping::Basic,
                None,
            );
            self.footer_buffer
                .shape_until_scroll(&mut self.font_system, false);
        }

        // Scrollbar: subtle track + proportional thumb at the right edge. With a
        // lazy file `total` is the lines discovered so far, so the thumb shrinks
        // as more of the file gets indexed — an honest progress indicator.
        self.quads.clear();
        let total = source.total_lines();
        let page = self.page_lines();
        if total > page {
            let track_w = 6.0 * self.scale;
            let (x0, x1) = (width - track_w, width);
            let track_h = content_bottom as f32;
            let thumb_h = (track_h * page as f32 / total as f32).max(24.0 * self.scale);
            let denom = (total - page) as f32;
            let frac = (self.scroll as f32 / denom).min(1.0);
            let thumb_y = frac * (track_h - thumb_h);
            self.quads
                .push(x0, 0.0, x1, track_h, width, height, [1.0, 1.0, 1.0, 0.05]);
            self.quads.push(
                x0,
                thumb_y,
                x1,
                thumb_y + thumb_h,
                width,
                height,
                [1.0, 1.0, 1.0, 0.22],
            );
        }

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.surface_config.width,
                height: self.surface_config.height,
            },
        );

        let mut areas = vec![
            // Line-number gutter, pinned at the left.
            TextArea {
                buffer: &self.gutter_buffer,
                left: PADDING,
                top: PADDING,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: content_left as i32,
                    bottom: content_bottom,
                },
                default_color: GUTTER_FG,
                custom_glyphs: &[],
            },
            TextArea {
                buffer: &self.text_buffer,
                // Pan left by the horizontal scroll; bounds still clip at the
                // gutter's right edge so panned text never overlaps the numbers.
                left: content_left - self.hscroll,
                top: PADDING,
                scale: 1.0,
                bounds: TextBounds {
                    left: content_left as i32,
                    top: 0,
                    right: self.surface_config.width as i32,
                    bottom: content_bottom,
                },
                default_color: FG,
                custom_glyphs: &[],
            },
        ];
        if let Some((_, color)) = status {
            areas.push(TextArea {
                buffer: &self.footer_buffer,
                left: PADDING,
                top: content_bottom as f32 + 2.0,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: content_bottom,
                    right: self.surface_config.width as i32,
                    bottom: self.surface_config.height as i32,
                },
                default_color: color,
                custom_glyphs: &[],
            });
        }

        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
            )
            .unwrap();

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Suboptimal(_)
            | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.surface_config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("surface validation error");
                return;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)
                .unwrap();
            self.quads.draw(&self.queue, &mut pass);
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.atlas.trim();
    }
}

/// Minimal solid-color quad renderer (wgpu pipeline). glyphon only draws text,
/// so UI chrome like the scrollbar needs its own tiny pipeline.
struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    vbuf: wgpu::Buffer,
    verts: Vec<f32>, // interleaved: x, y (NDC), r, g, b, a
}

const QUAD_SHADER: &str = "
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};
@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    out.color = color;
    return out;
}
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
";

impl QuadRenderer {
    /// Room for a handful of quads (track + thumb + future chrome).
    const MAX_QUADS: usize = 8;
    const FLOATS_PER_VERT: usize = 6;

    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(QUAD_SHADER.into()),
        });
        let attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: (Self::FLOATS_PER_VERT * 4) as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad-verts"),
            size: (Self::MAX_QUADS * 6 * Self::FLOATS_PER_VERT * 4) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            vbuf,
            verts: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.verts.clear();
    }

    /// Queues a quad given in physical pixels (origin top-left).
    fn push(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, h: f32, color: [f32; 4]) {
        if self.verts.len() / (6 * Self::FLOATS_PER_VERT) >= Self::MAX_QUADS {
            return;
        }
        let nx = |x: f32| x / w * 2.0 - 1.0;
        let ny = |y: f32| 1.0 - y / h * 2.0;
        let quad = [
            (x0, y0),
            (x1, y0),
            (x0, y1),
            (x1, y0),
            (x1, y1),
            (x0, y1),
        ];
        for (x, y) in quad {
            self.verts.extend_from_slice(&[nx(x), ny(y)]);
            self.verts.extend_from_slice(&color);
        }
    }

    fn draw(&self, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass) {
        if self.verts.is_empty() {
            return;
        }
        let bytes: Vec<u8> = self.verts.iter().flat_map(|f| f.to_ne_bytes()).collect();
        queue.write_buffer(&self.vbuf, 0, &bytes);
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        pass.draw(0..(self.verts.len() / Self::FLOATS_PER_VERT) as u32, 0..1);
    }
}

/// RSS peak of the process. On macOS `ru_maxrss` is in BYTES.
#[cfg(target_os = "macos")]
fn peak_rss_bytes() -> u64 {
    use std::mem::MaybeUninit;
    let mut usage = MaybeUninit::<libc::rusage>::zeroed();
    let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if ret == 0 {
        (unsafe { usage.assume_init() }).ru_maxrss as u64
    } else {
        0
    }
}

#[cfg(not(target_os = "macos"))]
fn peak_rss_bytes() -> u64 {
    0
}
