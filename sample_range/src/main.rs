mod app;
mod flighthook;
mod hud;
mod scene;
mod shot_data;
mod vulkan;

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use app::App;
use shot_data::ShotData;

struct Handler {
    shot:              ShotData,
    loop_csv:          bool,
    live_rx:           Option<std::sync::mpsc::Receiver<ShotData>>,
    app:               Option<App>,
    screenshot_path:   Option<String>,
    screenshot_frame:  u32,  // render this many frames before capturing
    frame_count:       u32,
    screenshot_done:   bool,
}

impl ApplicationHandler for Handler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title("FlightScope Vulkan")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280u32, 720u32))
            .with_resizable(false);
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let mut app = App::new(window, &self.shot, self.loop_csv);
        if let Some(rx) = self.live_rx.take() {
            app.set_live_receiver(rx);
        }
        self.app = Some(app);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed =>
            {
                if let PhysicalKey::Code(KeyCode::F11) = event.physical_key {
                    if let Some(app) = &self.app {
                        let w = app.window();
                        if w.fullscreen().is_some() {
                            w.set_fullscreen(None);
                        } else {
                            w.set_fullscreen(Some(Fullscreen::Borderless(None)));
                        }
                    }
                }
                if let PhysicalKey::Code(KeyCode::Escape) = event.physical_key {
                    if let Some(app) = &self.app {
                        app.window().set_fullscreen(None);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(app) = &mut self.app {
                    app.tick();
                    app.render();
                }

                if let Some(ref path) = self.screenshot_path.clone() {
                    if !self.screenshot_done {
                        self.frame_count += 1;
                        if self.frame_count >= self.screenshot_frame {
                            self.screenshot_done = true;
                            if let Some(app) = &mut self.app {
                                app.save_screenshot(path);
                            }
                            std::process::exit(0);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = &self.app {
            app.window().request_redraw();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut csv_path = None;
    let mut connect_url = None;
    let mut screenshot_path = None;
    let mut screenshot_frame = 1u32;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--csv" && i + 1 < args.len() {
            csv_path = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--connect" && i + 1 < args.len() {
            connect_url = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot_path = Some(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--screenshot-frame" && i + 1 < args.len() {
            screenshot_frame = args[i + 1].parse().unwrap_or(1);
            i += 2;
        } else {
            i += 1;
        }
    }

    // Live mode: --connect takes priority; starts in Idle, loops off.
    let (shot, loop_csv, live_rx) = if let Some(ref url) = connect_url {
        eprintln!("Live mode: connecting to {url}");
        let rx = flighthook::spawn(url);
        (ShotData::default(), false, Some(rx))
    } else if let Some(path) = csv_path {
        let s = ShotData::from_csv(&path).unwrap_or_else(|| {
            eprintln!("Warning: could not load CSV, using example shot.");
            ShotData::example()
        });
        (s, true, None)
    } else {
        (ShotData::example(), true, None)
    };

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut handler = Handler {
        shot,
        loop_csv,
        live_rx,
        app: None,
        screenshot_path,
        screenshot_frame,
        frame_count: 0,
        screenshot_done: false,
    };
    event_loop.run_app(&mut handler).unwrap();
}
