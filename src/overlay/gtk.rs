//! The overlay process (Linux): GTK 4 on a layer-shell surface, fed by the daemon.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::BufRead;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::Value;

use gtk4::prelude::*;
use gtk4::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const BARS: usize = 24;

const CSS: &str = "
window { background: transparent; }
.pill {
    background: rgba(22, 22, 26, 0.94);
    border: 1px solid rgba(255, 255, 255, 0.09);
    border-radius: 21px;
    padding: 7px 16px 7px 13px;
    color: #ececec;
    font-size: 13px;
}
.dim { color: rgba(236, 236, 236, 0.55); }
.time { font-feature-settings: 'tnum'; color: rgba(236, 236, 236, 0.55); font-size: 12px; }
.mark { font-weight: bold; }
.ok .mark { color: #4ade80; }
.error .mark { color: #fbbf24; }
";

#[derive(Clone, Copy, PartialEq)]
enum State {
    Hidden,
    Recording {
        answer: bool,
    },
    Working,
    Speaking,
    /// Shown until then, then hidden.
    Note {
        until: Instant,
    },
}

struct Model {
    state: State,
    since: Instant,
    levels: VecDeque<f32>,
    tick: u32,
}

/// Runs the overlay until stdin closes (the daemon quit).
pub fn run() -> Result<()> {
    let app = gtk4::Application::builder()
        .application_id("ai.parley.overlay")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(build);
    app.run_with_args::<&str>(&[]);
    Ok(())
}

fn build(app: &gtk4::Application) {
    let Some(display) = gdk::Display::default() else {
        return app.quit();
    };
    if !gtk4_layer_shell::is_supported() {
        eprintln!("parley overlay: the compositor has no layer-shell; not showing it");
        return app.quit();
    }
    let css = gtk4::CssProvider::new();
    css.load_from_string(CSS);
    gtk4::style_context_add_provider_for_display(
        &display,
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = gtk4::ApplicationWindow::new(app);
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("parley"));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Bottom, 56);
    window.set_decorated(false);
    // Clicks go through to whatever is underneath.
    window.connect_realize(|w| {
        if let Some(surface) = w.surface() {
            surface.set_input_region(Some(&gtk4::cairo::Region::create()));
        }
    });

    let pill = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    pill.add_css_class("pill");
    let meter = gtk4::DrawingArea::new();
    meter.set_content_width(14 + BARS as i32 * 5);
    meter.set_content_height(24);
    let mark = gtk4::Label::new(None);
    mark.add_css_class("mark");
    let time = gtk4::Label::new(None);
    time.add_css_class("time");
    let text = gtk4::Label::new(None);
    text.set_single_line_mode(true);
    text.set_xalign(0.0);
    text.set_max_width_chars(48);
    let detail = gtk4::Label::new(None);
    detail.add_css_class("dim");
    detail.set_single_line_mode(true);
    detail.set_xalign(0.0);
    detail.set_max_width_chars(28);
    for w in [
        meter.upcast_ref::<gtk4::Widget>(),
        mark.upcast_ref(),
        time.upcast_ref(),
        text.upcast_ref(),
        detail.upcast_ref(),
    ] {
        pill.append(w);
    }
    window.set_child(Some(&pill));

    let model = Rc::new(RefCell::new(Model {
        state: State::Hidden,
        since: Instant::now(),
        levels: VecDeque::from(vec![0.0; BARS]),
        tick: 0,
    }));
    meter.set_draw_func({
        let model = model.clone();
        move |_, cr, w, h| draw_meter(&model.borrow(), cr, w as f64, h as f64)
    });

    // Messages from the daemon, read on their own thread.
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str(&line)
                && tx.send(v).is_err()
            {
                break;
            }
        }
    });

    let app = app.clone();
    glib::timeout_add_local(Duration::from_millis(40), move || {
        let mut m = model.borrow_mut();
        m.tick = m.tick.wrapping_add(1);
        loop {
            let msg = match rx.try_recv() {
                Ok(msg) => msg,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    app.quit();
                    return glib::ControlFlow::Break;
                }
            };
            let t = msg["t"].as_str().unwrap_or_default();
            let show = |state: State, m: &mut Model| {
                m.state = state;
                m.since = Instant::now();
                pill.remove_css_class("ok");
                pill.remove_css_class("error");
            };
            match msg["s"].as_str().unwrap_or_default() {
                "rec" => {
                    let answer = msg["answer"] == true;
                    show(State::Recording { answer }, &mut m);
                    m.levels = VecDeque::from(vec![0.0; BARS]);
                    fit(
                        &text,
                        if answer {
                            "Your answer…"
                        } else {
                            "Listening…"
                        },
                    );
                    text.add_css_class("dim");
                    text.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
                    fit(&detail, "");
                }
                "level" => {
                    m.levels.pop_front();
                    m.levels.push_back(msg["v"].as_f64().unwrap_or(0.0) as f32);
                }
                "heard" if matches!(m.state, State::Recording { .. }) => {
                    fit(&text, t);
                    text.remove_css_class("dim");
                }
                "heard" if m.state == State::Working => fit(&detail, t),
                "work" => {
                    let heard = if matches!(m.state, State::Recording { .. })
                        && !text.has_css_class("dim")
                    {
                        text.text().to_string()
                    } else {
                        detail.text().to_string()
                    };
                    show(State::Working, &mut m);
                    fit(&text, &format!("{t}…"));
                    text.remove_css_class("dim");
                    text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    fit(&detail, &heard);
                    detail.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
                }
                kind @ ("done" | "error" | "info") => {
                    let secs = match kind {
                        "error" => 3.5,
                        "done" => 1.8,
                        _ => 2.2,
                    };
                    show(
                        State::Note {
                            until: Instant::now() + Duration::from_secs_f64(secs),
                        },
                        &mut m,
                    );
                    match kind {
                        "done" => pill.add_css_class("ok"),
                        "error" => pill.add_css_class("error"),
                        _ => {}
                    }
                    mark.set_text(match kind {
                        "done" => "✓",
                        "error" => "!",
                        _ => "",
                    });
                    fit(&text, t);
                    text.remove_css_class("dim");
                    text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                    fit(&detail, "");
                }
                "speak" => {
                    show(State::Speaking, &mut m);
                    fit(&text, "Reading the answer");
                    text.remove_css_class("dim");
                    fit(&detail, "press the key to stop");
                    detail.set_ellipsize(gtk4::pango::EllipsizeMode::None);
                }
                "hide" => m.state = State::Hidden,
                _ => {}
            }
        }
        if let State::Note { until } = m.state
            && Instant::now() >= until
        {
            m.state = State::Hidden;
        }

        let state = m.state;
        meter.set_visible(matches!(
            state,
            State::Recording { .. } | State::Working | State::Speaking
        ));
        mark.set_visible(matches!(state, State::Note { .. }) && !mark.text().is_empty());
        time.set_visible(matches!(state, State::Recording { .. }));
        detail.set_visible(
            matches!(state, State::Working | State::Speaking) && !detail.text().is_empty(),
        );
        if let State::Recording { .. } = state {
            let secs = m.since.elapsed().as_secs();
            time.set_text(&format!("{}:{:02}", secs / 60, secs % 60));
        }
        if state == State::Hidden {
            if window.is_visible() {
                window.set_visible(false);
            }
        } else {
            if !window.is_visible() {
                window.present();
            }
            // The labels' minimum widths fit their text; shrink to them.
            window.set_default_size(1, 1);
            meter.queue_draw();
        }
        glib::ControlFlow::Continue
    });
}

/// The status dot and level bars while recording; bars that breathe while working or
/// speaking.
fn draw_meter(m: &Model, cr: &gtk4::cairo::Context, _w: f64, h: f64) {
    let mid = h / 2.0;
    let t = m.tick as f64;
    let (dot, bar): ((f64, f64, f64), (f64, f64, f64)) = match m.state {
        State::Recording { answer: false } => ((1.0, 0.30, 0.37), (0.93, 0.93, 0.93)),
        State::Recording { answer: true } => ((0.30, 0.64, 1.0), (0.93, 0.93, 0.93)),
        _ => ((0.62, 0.55, 1.0), (0.62, 0.55, 1.0)),
    };
    // A dot that pulses while you talk.
    let speaking = m.levels.iter().rev().take(3).any(|&l| l > 0.35);
    let r = if matches!(m.state, State::Recording { .. }) {
        if speaking {
            5.5
        } else {
            4.5 + (t / 6.0).sin().abs()
        }
    } else {
        4.5
    };
    cr.set_source_rgb(dot.0, dot.1, dot.2);
    cr.arc(6.0, mid, r, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();

    for i in 0..BARS {
        let level = match m.state {
            State::Recording { .. } => m.levels[i] as f64,
            // A wave running across the bars.
            _ => 0.25 + 0.2 * ((t / 3.0) - i as f64 * 0.45).sin(),
        };
        let bh = (2.0 + level * (h - 6.0)).min(h - 2.0);
        let x = 16.0 + i as f64 * 5.0;
        let alpha = if level > 0.2 { 1.0 } else { 0.45 };
        cr.set_source_rgba(bar.0, bar.1, bar.2, alpha);
        rounded_rect(cr, x, mid - bh / 2.0, 3.0, bh, 1.5);
        let _ = cr.fill();
    }
}

/// Sets a label's text and makes its minimum width fit it (up to its maximum), which
/// is what the layer surface sizes itself by.
fn fit(label: &gtk4::Label, text: &str) {
    label.set_text(text);
    let max = label.max_width_chars();
    label.set_width_chars((text.chars().count() as i32).min(max));
}

fn rounded_rect(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -pi / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, pi / 2.0);
    cr.arc(x + r, y + h - r, r, pi / 2.0, pi);
    cr.arc(x + r, y + r, r, pi, 3.0 * pi / 2.0);
    cr.close_path();
}
