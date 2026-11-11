//! Cosmetic handoff between independent greeter and desktop compositors.
//! Authentication has already succeeded before the greeter calls this module.
use gtk4::{self as gtk, gdk, glib, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const FADE_OUT: f64 = 0.24;
const FADE_IN: f64 = 0.32;
const MAX_COVER: Duration = Duration::from_secs(4);

fn reduced_motion() -> bool {
    std::env::var("SHELL_SESSION_REDUCED_MOTION").as_deref() == Ok("1")
        || gtk::Settings::default().is_some_and(|s| !s.is_gtk_enable_animations())
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct Surface {
    window: gtk::ApplicationWindow,
    area: gtk::DrawingArea,
    painted_black: Rc<Cell<bool>>,
}
struct Curtain {
    app: gtk::Application,
    alpha: Cell<f64>,
    surfaces: RefCell<Vec<Surface>>,
}
impl Curtain {
    fn new(app: &gtk::Application, alpha: f64) -> Option<Rc<Self>> {
        if reduced_motion() || !gtk4_layer_shell::is_supported() {
            return None;
        }
        let display = gdk::Display::default()?;
        let css = gtk::CssProvider::new();
        css.load_from_string(
            "window.session-transition { background: transparent; box-shadow: none; }",
        );
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 3,
        );
        let curtain = Rc::new(Self {
            app: app.clone(),
            alpha: Cell::new(alpha),
            surfaces: RefCell::new(Vec::new()),
        });
        curtain.monitors();
        let weak = Rc::downgrade(&curtain);
        display.monitors().connect_items_changed(move |_, _, _, _| {
            if let Some(curtain) = weak.upgrade() {
                curtain.monitors();
            }
        });
        Some(curtain)
    }
    fn monitors(self: &Rc<Self>) {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let mut surfaces = Vec::new();
        for monitor in display
            .monitors()
            .iter::<gdk::Monitor>()
            .filter_map(Result::ok)
            .filter(|m| m.is_valid())
        {
            let window = gtk::ApplicationWindow::builder()
                .application(&self.app)
                .title("Session transition")
                .build();
            window.add_css_class("session-transition");
            window.init_layer_shell();
            window.set_namespace(Some("session-transition"));
            window.set_layer(Layer::Overlay);
            window.set_keyboard_mode(KeyboardMode::None);
            window.set_exclusive_zone(-1);
            window.set_monitor(Some(&monitor));
            for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                window.set_anchor(edge, true);
            }
            window.connect_map(|window| {
                if let Some(surface) = window.surface() {
                    surface.set_input_region(&gtk::cairo::Region::create());
                }
            });
            let area = gtk::DrawingArea::new();
            let painted_black = Rc::new(Cell::new(false));
            let painted = painted_black.clone();
            let weak = Rc::downgrade(self);
            area.set_draw_func(move |_, cr, _, _| {
                if let Some(curtain) = weak.upgrade() {
                    let alpha = curtain.alpha.get();
                    cr.set_source_rgba(0.0, 0.0, 0.0, alpha);
                    let _ = cr.paint();
                    painted.set(alpha >= 1.0);
                }
            });
            window.set_child(Some(&area));
            window.present();
            surfaces.push(Surface {
                window,
                area,
                painted_black,
            });
        }
        for old in self.surfaces.replace(surfaces) {
            old.window.close();
        }
    }
    fn alpha(&self, alpha: f64) {
        self.alpha.set(alpha);
        for surface in self.surfaces.borrow().iter() {
            surface.area.queue_draw();
        }
    }
    fn painted(&self) -> bool {
        let surfaces = self.surfaces.borrow();
        !surfaces.is_empty() && surfaces.iter().all(|s| s.painted_black.get())
    }
    fn close(&self) {
        for surface in self.surfaces.borrow_mut().drain(..) {
            surface.window.close();
        }
    }
}

/// Called only after greetd acknowledges StartSession; errors remain fully visible.
pub fn greeter(app: &gtk::Application, preview: bool) {
    let Some(curtain) = Curtain::new(app, 0.0) else {
        if !preview {
            app.quit();
        }
        return;
    };
    let app = app.clone();
    let started = Instant::now();
    let mut covered = None;
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let elapsed = started.elapsed();
        if reduced_motion() || elapsed >= Duration::from_secs(2) {
            if preview {
                curtain.close();
            } else {
                app.quit();
            }
            return glib::ControlFlow::Break;
        }
        if let Some(covered_at) = covered {
            if !preview {
                // An opaque frame was drawn on every output before compositor teardown.
                app.quit();
                return glib::ControlFlow::Break;
            }
            let t = Instant::now().duration_since(covered_at).as_secs_f64() / FADE_IN;
            curtain.alpha(1.0 - smooth(t));
            if t >= 1.0 {
                curtain.close();
                return glib::ControlFlow::Break;
            }
        } else if curtain.painted() {
            covered = Some(Instant::now());
        } else {
            curtain.alpha(smooth(elapsed.as_secs_f64() / FADE_OUT));
        }
        glib::ControlFlow::Continue
    });
}

// Require actual image content and a visible bar on every active output, rather
// than merely systemd's "active" (both services map their windows asynchronously).
fn desktop_ready(layers: &str, wallpapers: &str) -> bool {
    let Ok(serde_json::Value::Object(outputs)) = serde_json::from_str(layers) else {
        return false;
    };
    !outputs.is_empty()
        && outputs.iter().all(|(output, data)| {
            let Some(levels) = data.get("levels").and_then(|v| v.as_object()) else {
                return false;
            };
            let mapped = |name| {
                levels
                    .values()
                    .filter_map(|v| v.as_array())
                    .flatten()
                    .any(|layer| {
                        layer["namespace"].as_str() == Some(name)
                            && layer["alpha"].as_f64().unwrap_or(0.0) >= 0.99
                            && layer["w"].as_i64().unwrap_or(0) > 0
                            && layer["h"].as_i64().unwrap_or(0) > 0
                    })
            };
            mapped("vigil")
                && mapped("awww-daemon")
                && wallpapers.lines().any(|line| {
                    line.trim_start_matches(": ")
                        .starts_with(&format!("{output}: "))
                        && line.contains("currently displaying: image: ")
                })
        })
}
fn query(program: &str, args: &[&str]) -> Option<String> {
    // Run outside the GTK thread; bound a stuck compositor IPC request as well.
    let output = Command::new("timeout")
        .args(["--kill-after=0.2s", "0.5s", program])
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn desktop(app: &gtk::Application) {
    // Keep the helper alive if output discovery is briefly empty.
    let hold = app.hold();
    let Some(curtain) = Curtain::new(app, 1.0) else {
        app.quit();
        return;
    };
    let ready = Arc::new(AtomicBool::new(false));
    let worker_ready = ready.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let mut previous = false;
        while started.elapsed() < MAX_COVER {
            let now = query("hyprctl", &["-j", "layers"])
                .zip(query("awww", &["query"]))
                .is_some_and(|(layers, images)| desktop_ready(&layers, &images));
            if now && previous {
                worker_ready.store(true, Ordering::Relaxed);
                break;
            }
            previous = now;
            std::thread::sleep(Duration::from_millis(80));
        }
    });
    let app = app.clone();
    let started = Instant::now();
    let mut announced = false;
    let mut revealing = None;
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let _keep_alive = &hold;
        if reduced_motion() {
            app.quit();
            return glib::ControlFlow::Break;
        }
        if !announced && curtain.painted() {
            // The startup script may now start wallpaper/bar services underneath us.
            let _ = writeln!(std::io::stdout(), "ready");
            announced = true;
        }
        if revealing.is_none()
            && ((announced && ready.load(Ordering::Relaxed)) || started.elapsed() >= MAX_COVER)
        {
            revealing = Some(Instant::now());
        }
        if let Some(start) = revealing {
            let t = start.elapsed().as_secs_f64() / FADE_IN;
            curtain.alpha(1.0 - smooth(t));
            if t >= 1.0 {
                app.quit();
                return glib::ControlFlow::Break;
            }
        }
        glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readiness_requires_image_and_visible_bar_on_every_output() {
        let layer = |namespace| serde_json::json!({"namespace": namespace, "alpha": 1.0, "w": 1920, "h": 1080});
        let mut layers = serde_json::json!({
            "DP-1": {"levels": {"0": [layer("awww-daemon")], "2": [layer("vigil")]}},
            "DP-2": {"levels": {"0": [layer("awww-daemon")], "2": [layer("vigil")]}}
        });
        let images = ": DP-1: 1920x1080, scale: 1, currently displaying: image: /image\n: DP-2: 1920x1080, scale: 1, currently displaying: image: /image";
        assert!(desktop_ready(&layers.to_string(), images));
        assert!(!desktop_ready(
            &layers.to_string(),
            images.lines().next().unwrap()
        ));
        assert!(!desktop_ready(
            &layers.to_string(),
            &images.replace("image: /image", "color: 000000")
        ));
        layers["DP-2"]["levels"]["2"][0]["alpha"] = 0.5.into();
        assert!(!desktop_ready(&layers.to_string(), images));
        assert!(!desktop_ready("{}", images));
        assert!(!desktop_ready("broken", images));
    }
}
