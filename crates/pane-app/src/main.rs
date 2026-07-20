//! Pane v0.2 spike: a GPU window that renders a (possibly huge) file with a
//! virtualized viewport — only the visible lines are ever handed to glyphon, so
//! rendering cost is independent of file size. Scroll with the mouse wheel or
//! the arrow / page / home / end keys.
//!
//! On open it also prints the load metrics to stdout (the v0 benchmark story).

use std::sync::Arc;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use pane_core::TextFile;
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

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: pane <file>");
            std::process::exit(2);
        }
    };

    let file = match load_and_report(&path) {
        Ok(f) => Arc::new(f),
        Err(e) => {
            eprintln!("could not open {path}: {e}");
            std::process::exit(1);
        }
    };

    let title = format!("Pane — {}", short_name(&path));
    let event_loop = EventLoop::new().unwrap();
    event_loop
        .run_app(&mut Application {
            file,
            title,
            state: None,
        })
        .unwrap();
}

/// Opens the file, builds the index and prints the load metrics.
fn load_and_report(path: &str) -> std::io::Result<TextFile> {
    use std::time::Instant;
    let t0 = Instant::now();
    let file = TextFile::open(path)?;
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let bytes = file.byte_len();
    println!("─ Pane v0 · open metrics ─────────────────────────");
    println!("file:            {path}");
    println!("size:            {:.1} MB", bytes as f64 / 1e6);
    println!("lines:           {}", file.line_count());
    println!("open + index:    {open_ms:.1} ms");
    println!("index (heap):    {:.1} MB", file.index_heap_bytes() as f64 / 1e6);
    println!("peak RSS:        {:.1} MB", peak_rss_bytes() as f64 / 1e6);
    println!("──────────────────────────────────────────────────");
    Ok(file)
}

fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

struct Application {
    file: Arc<TextFile>,
    title: String,
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
        self.state = Some(pollster::block_on(WindowState::new(window, event_loop)));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let file = self.file.clone();
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

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
                state.scroll_by(lines, file.line_count());
                state.window.request_redraw();
            }

            WindowEvent::KeyboardInput { event: key, .. } if key.state == ElementState::Pressed => {
                let page = state.visible_lines().saturating_sub(2) as i64;
                let count = file.line_count();
                match key.logical_key {
                    Key::Named(NamedKey::ArrowDown) => state.scroll_by(1, count),
                    Key::Named(NamedKey::ArrowUp) => state.scroll_by(-1, count),
                    Key::Named(NamedKey::PageDown) => state.scroll_by(page, count),
                    Key::Named(NamedKey::PageUp) => state.scroll_by(-page, count),
                    Key::Named(NamedKey::Home) => state.scroll = 0,
                    Key::Named(NamedKey::End) => state.scroll = count.saturating_sub(1),
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    _ => {}
                }
                state.window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                state.render(&file);
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

    scroll: usize,
    scale: f32,

    // Keep the window last so it drops after the surface (avoids a wgpu crash).
    window: Arc<Window>,
}

impl WindowState {
    async fn new(window: Arc<Window>, event_loop: &ActiveEventLoop) -> Self {
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

        let text_buffer = Buffer::new(
            &mut font_system,
            Metrics::new(FONT_SIZE * scale, LINE_HEIGHT * scale),
        );

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
            scroll: 0,
            scale,
            window,
        }
    }

    /// Number of text lines that fit in the current viewport height.
    fn visible_lines(&self) -> usize {
        let h = self.surface_config.height as f32;
        ((h / (LINE_HEIGHT * self.scale)).ceil() as usize) + 1
    }

    fn scroll_by(&mut self, delta: i64, line_count: usize) {
        let max = line_count.saturating_sub(1) as i64;
        let next = (self.scroll as i64 + delta).clamp(0, max.max(0));
        self.scroll = next as usize;
    }

    /// Feeds only the visible slice of the file to the text buffer.
    fn set_visible_text(&mut self, file: &TextFile) {
        let start = self.scroll;
        let end = (start + self.visible_lines()).min(file.line_count());
        let mut text = String::new();
        for i in start..end {
            if let Some(line) = file.line(i) {
                text.push_str(&line);
            }
            text.push('\n');
        }
        self.text_buffer.set_size(
            Some(self.surface_config.width as f32),
            Some(self.surface_config.height as f32),
        );
        self.text_buffer.set_text(
            &text,
            &Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
    }

    fn render(&mut self, file: &TextFile) {
        self.set_visible_text(file);

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.surface_config.width,
                height: self.surface_config.height,
            },
        );

        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                [TextArea {
                    buffer: &self.text_buffer,
                    left: PADDING,
                    top: PADDING,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: self.surface_config.width as i32,
                        bottom: self.surface_config.height as i32,
                    },
                    default_color: FG,
                    custom_glyphs: &[],
                }],
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
