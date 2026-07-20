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
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use pane_core::TextFile;
use similar::{ChangeTag, TextDiff};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

// Visual constants (dark theme, centralized — see PRD: no config in v1).
const FONT_SIZE: f32 = 14.0;
const LINE_HEIGHT: f32 = 20.0;
const PADDING: f32 = 8.0;
const BG: wgpu::Color = wgpu::Color {
    r: 0.078,
    g: 0.086,
    b: 0.102,
    a: 1.0,
};
const FG: Color = Color::rgb(0xd0, 0xd4, 0xdc);
const FG_DIM: Color = Color::rgb(0x8a, 0x8f, 0x99);
const ADD: Color = Color::rgb(0x7e, 0xc6, 0x99);
const DEL: Color = Color::rgb(0xe0, 0x6c, 0x75);
const ACCENT: Color = Color::rgb(0x8a, 0xb4, 0xf8);
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

/// One rendered line plus the color to draw it in.
struct DiffLine {
    text: String,
    color: Color,
}

/// What the window renders: a lazily-indexed file, or a precomputed diff.
enum Source {
    File(TextFile),
    Diff(Vec<DiffLine>),
}

impl Source {
    /// Clamps `idx` to a valid line (indexing a file on demand).
    fn clamp_to_line(&self, idx: usize) -> usize {
        match self {
            Source::File(f) => f.clamp_to_line(idx),
            Source::Diff(d) => idx.min(d.len().saturating_sub(1)),
        }
    }

    /// Visible lines `[start, start+count)` as `(text_with_newline, color)`.
    fn visible(&self, start: usize, count: usize) -> Vec<(String, Color)> {
        match self {
            Source::File(f) => {
                let mut out = Vec::with_capacity(count);
                for i in start..start + count {
                    match f.line(i) {
                        Some(l) => out.push((format!("{l}\n"), FG)),
                        None => break,
                    }
                }
                out
            }
            Source::Diff(d) => d
                .iter()
                .skip(start)
                .take(count)
                .map(|dl| (dl.text.clone(), dl.color))
                .collect(),
        }
    }
}

/// Builds a unified line diff, each line prefixed and colored (+/-/context).
fn build_diff(old: &str, new: &str) -> Vec<DiffLine> {
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
        out.push(DiffLine {
            text: format!("{prefix}{line}\n"),
            color,
        });
    }
    out
}

fn main() {
    // Flag parsing: any number of `--flags` plus positional paths.
    let mut review = false;
    let mut json = false;
    let mut stat = false;
    let mut diff = false;
    let mut positionals: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--review" => review = true,
            "--json" => json = true,
            "--stat" => stat = true,
            "--diff" => diff = true,
            s if !s.starts_with("--") => positionals.push(s.to_string()),
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
        }
    }

    let (source, title) = if diff {
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
        (Source::Diff(build_diff(&old, &new)), title)
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
        (Source::File(file), title)
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
                state.window.request_redraw();
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -(y * 3.0) as i64,
                    MouseScrollDelta::PixelDelta(p) => -(p.y as f32 / LINE_HEIGHT) as i64,
                };
                let next = (state.scroll as i64 + lines).max(0) as usize;
                state.scroll = source.clamp_to_line(next);
                state.window.request_redraw();
            }

            WindowEvent::KeyboardInput { event: key, .. } if key.state == ElementState::Pressed => {
                // Review verdict keys take precedence and end the session.
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

                // Scrolling works in every mode so you can read before deciding.
                let page = state.visible_lines().saturating_sub(2);
                let s = state.scroll;
                state.scroll = match &key.logical_key {
                    Key::Named(NamedKey::ArrowDown) => source.clamp_to_line(s + 1),
                    Key::Named(NamedKey::ArrowUp) => s.saturating_sub(1),
                    Key::Named(NamedKey::PageDown) => source.clamp_to_line(s + page),
                    Key::Named(NamedKey::PageUp) => s.saturating_sub(page),
                    Key::Named(NamedKey::Home) => 0,
                    Key::Named(NamedKey::End) => source.clamp_to_line(usize::MAX),
                    _ => s,
                };
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

    scroll: usize,
    scale: f32,
    review: bool,

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
        let text_buffer = Buffer::new(&mut font_system, metrics);
        let footer_buffer = Buffer::new(&mut font_system, metrics);

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
            scroll: 0,
            scale,
            review,
            window,
        }
    }

    /// Number of text lines that fit in the current viewport height.
    fn visible_lines(&self) -> usize {
        let h = self.surface_config.height as f32;
        ((h / (LINE_HEIGHT * self.scale)).ceil() as usize) + 1
    }

    /// Feeds only the visible slice to the text buffer as colored rich text.
    fn set_visible_text(&mut self, source: &Source) {
        let want = self.visible_lines();
        let lines = source.visible(self.scroll, want);
        let base = Attrs::new().family(Family::Monospace);
        let spans = lines
            .iter()
            .map(|(t, c)| (t.as_str(), Attrs::new().family(Family::Monospace).color(*c)));
        self.text_buffer.set_size(
            Some(self.surface_config.width as f32),
            Some(self.surface_config.height as f32),
        );
        self.text_buffer
            .set_rich_text(spans, &base, Shaping::Advanced, None);
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }

    fn render(&mut self, source: &Source) {
        self.set_visible_text(source);

        let width = self.surface_config.width as f32;
        let height = self.surface_config.height as f32;
        let footer_h = if self.review {
            (LINE_HEIGHT * self.scale).ceil() + 6.0
        } else {
            0.0
        };
        let content_bottom = (height - footer_h) as i32;

        if self.review {
            self.footer_buffer.set_size(Some(width), Some(footer_h.max(1.0)));
            self.footer_buffer.set_text(
                FOOTER,
                &Attrs::new().family(Family::Monospace),
                Shaping::Basic,
                None,
            );
            self.footer_buffer
                .shape_until_scroll(&mut self.font_system, false);
        }

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.surface_config.width,
                height: self.surface_config.height,
            },
        );

        let mut areas = vec![TextArea {
            buffer: &self.text_buffer,
            left: PADDING,
            top: PADDING,
            scale: 1.0,
            bounds: TextBounds {
                left: 0,
                top: 0,
                right: self.surface_config.width as i32,
                bottom: content_bottom,
            },
            default_color: FG,
            custom_glyphs: &[],
        }];
        if self.review {
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
                default_color: ACCENT,
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
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.atlas.trim();
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
