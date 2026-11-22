//! bench-scene: the picture every stack streams in Helio's benchmarks.
//!
//! The host runs this full screen; the client is measured on what arrives.
//! It draws one of five scenes, all from `scene.wgsl`, so the content is the
//! same frame for frame on every machine and every stack:
//!
//! | Scene | Keys | For |
//! |---|---|---|
//! | `flash` | 1 | Click to screen: black, white while a mouse button is held |
//! | `counter` | 2 | Smoothness: a frame counter over a gray that steps each frame |
//! | `shapes` | 3 | Picture quality on motion: moving discs |
//! | `noise` | 4 | Full bitrate: new noise in every pixel of every frame |
//! | `clock` | 5 | Glass to glass: a millisecond clock, on every monitor with `--all-monitors` |
//!
//! With `--monitor WxH@HZ` the window opens on the display in the stream's
//! mode, because a host that adds a display for the stream may leave the
//! primary one where it was; and the scene moves the pointer onto its window,
//! so clicks land on it wherever the host left the pointer.
//!
//! Decisions worth knowing:
//!
//! - `flash` draws only on input, and draws in the input handler itself
//!   rather than waiting for a redraw event, with the lowest-latency present
//!   mode the surface has. The app then adds as little as it can to the
//!   latency being measured, and loads the GPU not at all between clicks: a
//!   click on a still desktop. With `--fps N` it draws every refresh instead,
//!   white while a button is held: a click in a game, where the next frame
//!   always follows.
//! - The other scenes draw continuously with V-Sync. With `--fps N` the
//!   frame number comes from the time since start, so a hitch here skips a
//!   number rather than shifting every later frame; the log says whether the
//!   host itself kept up, which separates its drops from the stream's.
//! - `--cap N` draws at most N frames a second, as a game capped at the
//!   stream's rate does: on a display whose refresh follows the app (VRR)
//!   the panel then runs at N too. It sleeps to a millisecond before each
//!   tick and spins the rest, since a sleep alone can wake a tick late.
//! - `--heavy N` adds N rounds of hashing per pixel to every frame: a stand-in
//!   for a game's GPU load, for measuring what a stream costs the game.
//! - On Windows it draws with Direct3D 12 (see `instance`).
//! - On Windows it is a GUI program (no console window over the scene), so
//!   everything it reports goes to `--log`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

const SHADER: &str = include_str!("scene.wgsl");
const SCENES: [&str; 5] = ["flash", "counter", "shapes", "noise", "clock"];
/// Bytes in the shader's `Params`.
const PARAMS_SIZE: u64 = 32;

const USAGE: &str = "\
usage: bench-scene [options]
  --scene NAME        flash, counter, shapes, noise or clock (default counter)
  --fps N             frames per second of content; 0 advances one per present (default 0)
  --present MODE      fifo, mailbox, immediate or auto (default: immediate-first for flash, fifo otherwise)
  --cap N             at most N frames a second (default: as many as the present mode lets)
  --heavy N           rounds of per-pixel work in every frame (default 0)
  --seconds S         quit after S seconds
  --log PATH          write every present and input as CSV
  --windowed WxH      a window instead of full screen
  --all-monitors      one full-screen window on every monitor
  --monitor WxH@HZ    full screen on the monitor in that mode (the stream's), else the primary
  --dump FRAME WxH PATH   render one frame of --scene to a binary PPM and quit
keys: 1-5 switch scene, Escape quits";

#[derive(Clone)]
struct Options {
    scene: u32,
    fps: f64,
    present: Option<wgpu::PresentMode>,
    heavy: u32,
    cap: f64,
    seconds: Option<f64>,
    log: Option<String>,
    windowed: Option<(u32, u32)>,
    all_monitors: bool,
    monitor: Option<(u32, u32, f64)>,
    dump: Option<(u32, u32, u32, String)>,
}

fn size_arg(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once('x').ok_or(format!("{s}: expected WxH"))?;
    Ok((
        w.parse().map_err(|_| format!("{s}: bad width"))?,
        h.parse().map_err(|_| format!("{s}: bad height"))?,
    ))
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        scene: 1,
        fps: 0.0,
        present: None,
        heavy: 0,
        cap: 0.0,
        seconds: None,
        log: None,
        windowed: None,
        all_monitors: false,
        monitor: None,
        dump: None,
    };
    let mut it = args.iter();
    let next = |it: &mut std::slice::Iter<String>, flag: &str| {
        it.next().cloned().ok_or(format!("{flag} needs a value"))
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--scene" => {
                let s = next(&mut it, a)?;
                o.scene = SCENES
                    .iter()
                    .position(|n| *n == s)
                    .ok_or(format!("unknown scene {s}"))? as u32;
            }
            "--fps" => o.fps = next(&mut it, a)?.parse().map_err(|_| "bad --fps")?,
            "--present" => {
                o.present = match next(&mut it, a)?.as_str() {
                    "fifo" => Some(wgpu::PresentMode::Fifo),
                    "mailbox" => Some(wgpu::PresentMode::Mailbox),
                    "immediate" => Some(wgpu::PresentMode::Immediate),
                    "auto" => None,
                    m => return Err(format!("unknown present mode {m}")),
                }
            }
            "--cap" => o.cap = next(&mut it, a)?.parse().map_err(|_| "bad --cap")?,
            "--heavy" => o.heavy = next(&mut it, a)?.parse().map_err(|_| "bad --heavy")?,
            "--seconds" => {
                o.seconds = Some(next(&mut it, a)?.parse().map_err(|_| "bad --seconds")?)
            }
            "--log" => o.log = Some(next(&mut it, a)?),
            "--windowed" => o.windowed = Some(size_arg(&next(&mut it, a)?)?),
            "--all-monitors" => o.all_monitors = true,
            "--monitor" => {
                let v = next(&mut it, a)?;
                let (size, hz) = v.split_once('@').ok_or(format!("{v}: expected WxH@HZ"))?;
                let (w, h) = size_arg(size)?;
                o.monitor = Some((
                    w,
                    h,
                    hz.parse().map_err(|_| format!("{v}: bad refresh rate"))?,
                ));
            }
            "--dump" => {
                let frame = next(&mut it, a)?.parse().map_err(|_| "bad --dump frame")?;
                let (w, h) = size_arg(&next(&mut it, a)?)?;
                o.dump = Some((frame, w, h, next(&mut it, a)?));
            }
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    Ok(o)
}

/// The uniform block, in the shader's field order.
#[allow(clippy::too_many_arguments)]
fn params(
    frame: u32,
    scene: u32,
    flash: bool,
    heavy: u32,
    w: u32,
    h: u32,
    clock_ms: u32,
) -> [u8; 32] {
    let words = [
        frame,
        scene,
        flash as u32,
        heavy,
        (w as f32).to_bits(),
        (h as f32).to_bits(),
        clock_ms,
        0,
    ];
    let mut b = [0u8; 32];
    for (i, v) in words.iter().enumerate() {
        b[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    b
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl Gpu {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> Gpu {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("params"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Gpu {
            device,
            queue,
            module,
            layout,
            pipeline_layout,
            pipelines: HashMap::new(),
        }
    }

    fn pipeline(&mut self, format: wgpu::TextureFormat) -> &wgpu::RenderPipeline {
        let (device, module, layout) = (&self.device, &self.module, &self.pipeline_layout);
        self.pipelines.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("scene"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("v_full"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("f_scene"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    fn uniform(&self) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: PARAMS_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("params"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        (buffer, bind)
    }

    fn draw(
        &mut self,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        bind: &wgpu::BindGroup,
    ) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let pipeline = self.pipeline(format).clone();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }
}

/// The GPU instance. On Windows the scene draws with Direct3D 12, as games
/// there mostly do: how Vulkan presents on Windows depends on a driver setting
/// (NVIDIA's Vulkan/OpenGL present method) that some hosts change, which would
/// give the scene a different path under each. `WGPU_BACKEND` still wins.
fn instance() -> wgpu::Instance {
    let mut d = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    if cfg!(windows) && std::env::var_os("WGPU_BACKEND").is_none() {
        d.backends = wgpu::Backends::DX12;
    }
    wgpu::Instance::new(d)
}

/// Renders one frame offscreen and writes it as a binary PPM: the reference a
/// streamed frame with the same counter is compared against.
fn dump(o: &Options, frame: u32, w: u32, h: u32, path: &str) -> Result<(), String> {
    let instance = instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .map_err(|e| format!("no GPU: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
        .map_err(|e| format!("no GPU device: {e}"))?;
    let mut gpu = Gpu::new(device, queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dump"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let (buffer, bind) = gpu.uniform();
    gpu.queue
        .write_buffer(&buffer, 0, &params(frame, o.scene, false, o.heavy, w, h, 0));
    gpu.draw(&target.create_view(&Default::default()), format, &bind);
    let row = (w * 4).div_ceil(256) * 256;
    let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dump"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &read,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    if !matches!(rx.recv(), Ok(Ok(()))) {
        return Err("not read back".into());
    }
    let data = read
        .slice(..)
        .get_mapped_range()
        .map_err(|e| format!("not mapped: {e:?}"))?;
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    for y in 0..h as usize {
        for px in data[y * row as usize..y * row as usize + w as usize * 4]
            .as_chunks::<4>()
            .0
        {
            out.extend_from_slice(&px[..3]);
        }
    }
    std::fs::write(path, out).map_err(|e| format!("{path}: {e}"))
}

struct View {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    modes: Vec<wgpu::PresentMode>,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    presents: u64,
}

struct App {
    o: Options,
    instance: wgpu::Instance,
    adapter: Option<wgpu::Adapter>,
    gpu: Option<Gpu>,
    views: Vec<View>,
    start: Instant,
    flash: bool,
    log: Option<std::io::BufWriter<std::fs::File>>,
    second: (u64, u64),
    flushed: Instant,
    /// When the next frame may be drawn, with `--cap`.
    next_frame: Instant,
    error: Option<String>,
}

impl App {
    fn us(&self) -> u128 {
        self.start.elapsed().as_micros()
    }

    /// Writes a line to the log, and the log to disk. The scene is usually
    /// ended by killing it, which no buffer survives.
    fn note(&mut self, line: std::fmt::Arguments) {
        if let Some(l) = &mut self.log {
            let _ = writeln!(l, "{line}");
            let _ = l.flush();
            self.flushed = Instant::now();
        }
    }

    /// A present's line: these come every frame, so they reach the disk once
    /// a second.
    fn note_present(&mut self, line: std::fmt::Arguments) {
        if let Some(l) = &mut self.log {
            let _ = writeln!(l, "{line}");
            if self.flushed.elapsed() >= Duration::from_secs(1) {
                let _ = l.flush();
                self.flushed = Instant::now();
            }
        }
    }

    /// The present mode for the current scene: what `--present` asked for
    /// when the surface has it, else the lowest-latency mode for `flash` and
    /// V-Sync for the rest.
    fn present_mode(&self, modes: &[wgpu::PresentMode]) -> wgpu::PresentMode {
        use wgpu::PresentMode::*;
        let wanted: &[wgpu::PresentMode] = match self.o.present {
            Some(m) => &[m],
            None if self.on_input() => &[Immediate, Mailbox],
            None => &[Fifo],
        };
        wanted
            .iter()
            .copied()
            .find(|m| modes.contains(m))
            .unwrap_or(Fifo)
    }

    fn reconfigure(&mut self) {
        let Some(gpu) = &self.gpu else { return };
        let latency = if self.on_input() { 1 } else { 2 };
        let modes: Vec<_> = self
            .views
            .iter()
            .map(|v| self.present_mode(&v.modes))
            .collect();
        for (v, mode) in self.views.iter_mut().zip(modes) {
            v.config.present_mode = mode;
            v.config.desired_maximum_frame_latency = latency;
            v.surface.configure(&gpu.device, &v.config);
        }
        let names: Vec<String> = self
            .views
            .iter()
            .map(|v| format!("{:?}", v.config.present_mode))
            .collect();
        self.note(format_args!(
            "scene,{},{},{}",
            self.us(),
            SCENES[self.o.scene as usize],
            names.join("/")
        ));
    }

    fn open(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let monitors: Vec<_> = if self.o.all_monitors {
            el.available_monitors().map(Some).collect()
        } else if let Some((w, h, hz)) = self.o.monitor {
            // A host that adds a display for the stream may leave the
            // primary one where it was (Apollo, by default): the stream's
            // display is the one in the stream's mode.
            let found = el.available_monitors().find(|m| {
                let size = m.size();
                let mhz = m.refresh_rate_millihertz().unwrap_or(0) as f64;
                size.width == w && size.height == h && (mhz / 1000.0 - hz).abs() < 1.5
            });
            if found.is_none() {
                self.note(format_args!(
                    "monitor,0,no monitor at {w}x{h}@{hz}: the primary one,"
                ));
            }
            vec![found]
        } else {
            vec![None]
        };
        for monitor in monitors {
            let mut a = Window::default_attributes().with_title("bench-scene");
            a = match self.o.windowed {
                Some((w, h)) => a.with_inner_size(PhysicalSize::new(w, h)),
                None => a.with_fullscreen(Some(Fullscreen::Borderless(monitor))),
            };
            let window = Arc::new(el.create_window(a).map_err(|e| format!("no window: {e}"))?);
            window.set_cursor_visible(false);
            // The pointer may sit on another display; a click must land here.
            let size = window.inner_size();
            let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(
                size.width / 2,
                size.height / 2,
            ));
            if let Some(m) = window.current_monitor() {
                let mhz = m.refresh_rate_millihertz().unwrap_or(0);
                let name = m.name().unwrap_or_default().replace(',', " ");
                self.note(format_args!(
                    "monitor,0,{name},{}x{}@{:.3}",
                    m.size().width,
                    m.size().height,
                    mhz as f64 / 1000.0
                ));
            }
            let surface = self
                .instance
                .create_surface(window.clone())
                .map_err(|e| format!("no surface: {e}"))?;
            if self.adapter.is_none() {
                let adapter = pollster::block_on(self.instance.request_adapter(
                    &wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface: Some(&surface),
                        ..Default::default()
                    },
                ))
                .map_err(|e| format!("no GPU: {e}"))?;
                let (device, queue) =
                    pollster::block_on(adapter.request_device(&Default::default()))
                        .map_err(|e| format!("no GPU device: {e}"))?;
                let info = adapter.get_info();
                self.note(format_args!(
                    "gpu,0,{},{:?}",
                    info.name.replace(',', " "),
                    info.backend
                ));
                self.gpu = Some(Gpu::new(device, queue));
                self.adapter = Some(adapter);
            }
            let (Some(adapter), Some(gpu)) = (&self.adapter, &self.gpu) else {
                unreachable!()
            };
            let size = window.inner_size();
            let caps = surface.get_capabilities(adapter);
            let mut config = surface
                .get_default_config(adapter, size.width.max(1), size.height.max(1))
                .ok_or("the surface has no configuration")?;
            // The shader writes the values to display: a surface that does not encode.
            if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
                config.format = *f;
            }
            let (uniform, bind) = gpu.uniform();
            self.note(format_args!(
                "window,0,{}x{},{:?}",
                size.width, size.height, config.format
            ));
            self.views.push(View {
                window,
                surface,
                config,
                modes: caps.present_modes,
                uniform,
                bind,
                presents: 0,
            });
        }
        self.reconfigure();
        Ok(())
    }

    /// Whether the scene draws only when input comes: `flash` without
    /// `--fps`. With `--fps` it draws every refresh, as a game does, and the
    /// click shows in the next frame drawn.
    fn on_input(&self) -> bool {
        self.o.scene == 0 && self.o.fps <= 0.0
    }

    fn frame(&self, presents: u64) -> u32 {
        if self.o.fps > 0.0 {
            (self.start.elapsed().as_secs_f64() * self.o.fps) as u32
        } else {
            presents as u32
        }
    }

    /// With `--cap`, waits for the next frame's tick.
    fn pace(&mut self) {
        if self.o.cap <= 0.0 || self.on_input() {
            return;
        }
        let period = Duration::from_secs_f64(1.0 / self.o.cap);
        let now = Instant::now();
        if self.next_frame > now {
            let left = self.next_frame - now;
            if left > Duration::from_millis(1) {
                std::thread::sleep(left - Duration::from_millis(1));
            }
            while Instant::now() < self.next_frame {
                std::hint::spin_loop();
            }
            self.next_frame += period;
        } else if now - self.next_frame < period {
            // A little late: keep the cadence.
            self.next_frame += period;
        } else {
            // Behind by more than a frame: count again from now.
            self.next_frame = now + period;
        }
    }

    fn render(&mut self, i: usize) {
        if i == 0 {
            self.pace();
        }
        let frame = self.frame(self.views[i].presents);
        let clock_ms = self.start.elapsed().as_millis() as u32;
        let (scene, flash, heavy) = (self.o.scene, self.flash, self.o.heavy);
        let Some(gpu) = &mut self.gpu else { return };
        let v = &mut self.views[i];
        let texture = match v.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                v.surface.configure(&gpu.device, &v.config);
                return;
            }
            _ => return,
        };
        let (w, h) = (v.config.width, v.config.height);
        gpu.queue.write_buffer(
            &v.uniform,
            0,
            &params(frame, scene, flash, heavy, w, h, clock_ms),
        );
        let view = texture.texture.create_view(&Default::default());
        gpu.draw(&view, v.config.format, &v.bind);
        v.window.pre_present_notify();
        gpu.queue.present(texture);
        v.presents += 1;
        let t = self.us();
        self.note_present(format_args!("present,{t},{i},{frame},{}", flash as u8));
        if i == 0 {
            let sec = (t / 1_000_000) as u64;
            if sec != self.second.0 {
                let (s, n) = self.second;
                self.note(format_args!("fps,{s},{n}"));
                self.second = (sec, 0);
            }
            self.second.1 += 1;
        }
    }

    fn resize(&mut self, id: WindowId, size: PhysicalSize<u32>) {
        let Some(gpu) = &self.gpu else { return };
        if let Some(v) = self.views.iter_mut().find(|v| v.window.id() == id) {
            v.config.width = size.width.max(1);
            v.config.height = size.height.max(1);
            v.surface.configure(&gpu.device, &v.config);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.views.is_empty() {
            if let Err(e) = self.open(el) {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => self.resize(id, size),
            WindowEvent::MouseInput { state, .. } => {
                self.flash = state == ElementState::Pressed;
                let t = self.us();
                self.note(format_args!(
                    "input,{t},{},{}",
                    if self.flash { "down" } else { "up" },
                    0
                ));
                if self.on_input() {
                    // Drawn here, not on a redraw event, which can wait behind
                    // other messages.
                    for i in 0..self.views.len() {
                        self.render(i);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let scene = match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => return el.exit(),
                    PhysicalKey::Code(KeyCode::Digit1) => 0,
                    PhysicalKey::Code(KeyCode::Digit2) => 1,
                    PhysicalKey::Code(KeyCode::Digit3) => 2,
                    PhysicalKey::Code(KeyCode::Digit4) => 3,
                    PhysicalKey::Code(KeyCode::Digit5) => 4,
                    _ => return,
                };
                if scene != self.o.scene {
                    self.o.scene = scene;
                    self.flash = false;
                    self.reconfigure();
                    for i in 0..self.views.len() {
                        self.render(i);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(i) = self.views.iter().position(|v| v.window.id() == id) {
                    self.render(i);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let deadline = self
            .o
            .seconds
            .map(|s| self.start + Duration::from_secs_f64(s));
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return el.exit();
        }
        if self.on_input() {
            // Nothing changes between clicks: sleep until input or the deadline.
            el.set_control_flow(match deadline {
                Some(d) => ControlFlow::WaitUntil(d),
                None => ControlFlow::Wait,
            });
        } else {
            el.set_control_flow(ControlFlow::Poll);
            for v in &self.views {
                v.window.request_redraw();
            }
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let o = parse(&args)?;
    if let Some((frame, w, h, path)) = o.dump.clone() {
        return dump(&o, frame, w, h, &path);
    }
    let log = match &o.log {
        Some(p) => Some(std::io::BufWriter::new(
            std::fs::File::create(p).map_err(|e| format!("{p}: {e}"))?,
        )),
        None => None,
    };
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App {
        o,
        instance: instance(),
        adapter: None,
        gpu: None,
        views: Vec::new(),
        start: Instant::now(),
        flash: false,
        log,
        second: (0, 0),
        flushed: Instant::now(),
        next_frame: Instant::now(),
        error: None,
    };
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    app.note(format_args!("start,0,{epoch_ms},{}", args.join(" ")));
    el.run_app(&mut app).map_err(|e| e.to_string())?;
    let t = app.us();
    app.note(format_args!("end,{t},,"));
    if let Some(l) = &mut app.log {
        let _ = l.flush();
    }
    app.error.map_or(Ok(()), Err)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("bench-scene: {e}");
        std::process::exit(2);
    }
}
