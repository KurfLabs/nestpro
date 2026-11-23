//! The Feedback window and the crash note (FB1–FB8): the window's side of
//! `engine::feedback`, which the agents' `feedback` operation calls too.

use crate::engine::feedback::{self, Crash, Kind, Note};
use crate::theme::th;
use crate::App;
use eframe::egui;
use egui::RichText;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

/// What the Feedback window holds between frames.
#[derive(Default)]
pub struct FeedbackUi {
    pub open: bool,
    /// The window opens after this frame: the picture of the window is
    /// taken first, without the Feedback window in it (FB1).
    opens_after: Option<u64>,
    pub kind: Kind,
    pub text: String,
    pub with_picture: bool,
    pub with_job: bool,
    /// The window as it was when Feedback was pressed, as PNG.
    picture: Option<Vec<u8>>,
    picture_rx: Option<Receiver<Option<Vec<u8>>>>,
    /// The screen in front when Feedback was pressed ("Nesting › Home").
    screen: String,
    /// The crash this note goes with (FB7).
    pub crash: Option<Crash>,
    /// The crash remembered from last time, not answered yet (FB7).
    pub crash_card: Option<Crash>,
    /// What the last Save note or Post on GitHub said.
    said: Option<Result<String, String>>,
    saved_folder: Option<PathBuf>,
}

/// The window's picture as PNG bytes.
fn png(img: &egui::ColorImage) -> Option<Vec<u8>> {
    let [w, h] = img.size;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for c in &img.pixels {
        bytes.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
    }
    let im = image::RgbaImage::from_raw(w as u32, h as u32, bytes)?;
    let mut out = std::io::Cursor::new(Vec::new());
    im.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// This PC's settings folder: the last crash is kept there (FB7).
pub fn pc_dir() -> PathBuf {
    crate::settings::AppSettings::file().parent().map(std::path::Path::to_path_buf).unwrap_or_default()
}

/// Open a web link in the PC's own browser.
pub fn open_link(url: &str) {
    #[cfg(windows)]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" }).arg(url).spawn();
    let _ = r;
}

impl App {
    /// The screen and ribbon tab in front, as a person names them.
    pub fn screen_name(&self) -> String {
        use crate::{Tab, Workspace};
        let screen = match self.workspace {
            Workspace::Jobs => "Jobs",
            Workspace::Nesting => "Nesting",
            Workspace::Part => "Part",
        };
        let tab = match self.tab {
            Tab::Home => "Home",
            Tab::Sheet => "Sheet",
            Tab::Technology => "Technology",
            Tab::View => "View",
            Tab::Machining => "Machining",
            Tab::Simulation => "Simulation",
            Tab::Notes => "Notes",
            Tab::Utilities => "Utilities",
        };
        if self.workspace == Workspace::Jobs { screen.into() } else { format!("{screen} › {tab}") }
    }

    /// The Feedback button (FB1): a picture of the window is taken as it is
    /// now, then the window opens.
    pub fn open_feedback(&mut self, ctx: &egui::Context) {
        let f = &mut self.feedback;
        f.said = None;
        f.saved_folder = None;
        if f.open || f.opens_after.is_some() {
            return;
        }
        // FB1: both tick boxes start off each time.
        f.picture = None;
        f.with_picture = false;
        f.with_job = false;
        let (tx, rx) = channel();
        ctx.request_screenshot(move |img| {
            let _ = tx.send(png(&img));
        });
        f.picture_rx = Some(rx);
        f.opens_after = Some(ctx.cumulative_frame_nr());
        self.feedback.screen = self.screen_name();
    }

    /// FB7: the note at the top of the window after a crash.
    pub fn crash_card(&mut self, ctx: &egui::Context) {
        let Some(c) = self.feedback.crash_card.clone() else { return };
        if self.feedback.open {
            return;
        }
        let (mut write, mut close) = (false, false);
        let top = 70.0 + crate::ui_parts::RIBBON_H;
        egui::Area::new(egui::Id::unique("crash_card")).order(egui::Order::Foreground).anchor(egui::Align2::CENTER_TOP, [0.0, top]).show(ctx, |ui| {
            egui::Frame::new().fill(th().warn_bg).stroke(egui::Stroke::new(1.0, th().border)).corner_radius(6).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("The program closed unexpectedly {}.", self.clock.when(c.when))).strong().color(th().text_strong));
                    ui.label(RichText::new("A short note on what you were doing helps fix it.").color(th().text));
                    ui.add_space(6.0);
                    write = ui.button("Write a note…").on_hover_text("Opens Feedback with the error's details added to the note").clicked();
                    close = ui.small_button("✖").on_hover_text("Not now: don't ask about this crash again").clicked();
                });
            });
        });
        if write {
            self.feedback.crash = self.feedback.crash_card.take();
            self.feedback.kind = Kind::Problem;
            self.open_feedback(ctx);
        }
        if close {
            self.feedback.crash_card = None;
            if self.shot.is_none() {
                feedback::forget_crash(&pc_dir());
            }
        }
    }

    /// The Feedback window (FB1–FB6).
    pub fn feedback_window(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.feedback.picture_rx {
            if let Ok(p) = rx.try_recv() {
                self.feedback.picture = p;
                self.feedback.picture_rx = None;
            }
        }
        if let Some(f) = self.feedback.opens_after {
            if ctx.cumulative_frame_nr() > f {
                self.feedback.opens_after = None;
                self.feedback.open = true;
            }
            ctx.request_repaint();
        }
        if !self.feedback.open {
            return;
        }
        let (target, in_library) = feedback::target(&self.library_dir());
        let mut open = true;
        let (mut save, mut post, mut show) = (false, false, false);
        let has_job = !self.parts.is_empty();
        // Screenshots show what gets saved opened out.
        let shot = self.shot.is_some();
        egui::Window::new("Feedback").open(&mut open).collapsible(false).resizable(false).default_width(480.0).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center()).show(ctx, |ui| {
            let f = &mut self.feedback;
            ui.set_width(480.0);
            let where_to = if in_library {
                "It's saved in the Library's Feedback folder, where whoever looks after the program in your shop reads it."
            } else {
                "The Library can't be reached, so it's saved on this PC, in Documents › Sheet Nesting › Feedback."
            };
            ui.label(RichText::new(format!("Tell us about a problem, an idea or a question. {where_to}")).color(th().text_dim));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for k in Kind::ALL {
                    ui.selectable_value(&mut f.kind, k, k.label()).on_hover_text(k.hint());
                }
            });
            ui.add_space(4.0);
            let hint = match f.kind {
                Kind::Problem => "What were you doing, what happened, and what did you expect?",
                Kind::Idea => "What should the program do, and what would it save you?",
                Kind::Question => "What are you trying to do?",
            };
            ui.add(egui::TextEdit::multiline(&mut f.text).hint_text(hint).desired_rows(7).desired_width(f32::INFINITY));
            if let Some(c) = &f.crash {
                ui.label(RichText::new(format!("Goes with it: {}", c.text(&self.clock))).size(11.5).color(th().text_dim));
            }
            ui.add_space(4.0);
            ui.add_enabled_ui(f.picture.is_some(), |ui| {
                ui.checkbox(&mut f.with_picture, "Attach a picture of the window")
                    .on_hover_text("The window as it was when you pressed Feedback")
                    .on_disabled_hover_text("No picture of the window could be taken");
            });
            ui.add_enabled_ui(has_job, |ui| {
                ui.checkbox(&mut f.with_job, "Attach the open job")
                    .on_hover_text("A copy of the job as it is now, changes not yet saved included, so the problem can be repeated")
                    .on_disabled_hover_text("No job is open");
            });
            if !has_job {
                f.with_job = false;
            }
            egui::CollapsingHeader::new(RichText::new("What gets saved").size(12.0)).id_salt("feedback_saved").default_open(shot).show(ui, |ui| {
                let mut lines = vec![
                    format!("Your text, and that it's a {}", f.kind.label().to_lowercase()),
                    format!("{}, {}", self.who.label(), "and the date and time"),
                    format!("Version {} ({})", crate::updates::VERSION, feedback::system()),
                    format!("The screen: {}", f.screen),
                    format!("The machine: {}", self.cfg.machines.get(self.cfg.active_machine).map_or_else(|| self.spec.machine.label().to_string(), |m| m.name.clone())),
                ];
                if !self.job_name.is_empty() {
                    lines.push(format!("The open job's name: {}", self.job_name));
                }
                if f.with_picture {
                    lines.push("The picture of the window".into());
                }
                if f.with_job {
                    lines.push("A copy of the open job".into());
                }
                if f.crash.is_some() {
                    lines.push("The error's details".into());
                }
                for l in lines {
                    ui.label(RichText::new(format!("• {l}")).size(12.0).color(th().text));
                }
                ui.label(RichText::new(format!("In {}", target.display())).size(11.5).color(th().text_dim));
            });
            if let Some(said) = &f.said {
                ui.add_space(4.0);
                match said {
                    Ok(m) => ui.label(RichText::new(m).color(th().ok_fg)),
                    Err(m) => ui.label(RichText::new(m).color(th().warn_fg)),
                };
            }
            ui.add_space(8.0);
            let typed = !f.text.trim().is_empty() || f.crash.is_some();
            ui.horizontal(|ui| {
                save = ui
                    .add_enabled_ui(typed, |ui| {
                        ui.button("Save note")
                            .on_hover_text(if in_library { "Save it in the Library's Feedback folder" } else { "Save it on this PC (the Library can't be reached)" })
                            .on_disabled_hover_text("Write something first")
                            .clicked()
                    })
                    .inner;
                post = ui
                    .add_enabled_ui(!f.text.trim().is_empty(), |ui| {
                        ui.button("Post on GitHub…")
                            .on_hover_text("Opens a new public issue in your web browser with only the kind, your text and the version: no names, PC, job or picture. Anyone can read it, so leave out customer names, part numbers and drawings. Needs a free GitHub account.")
                            .on_disabled_hover_text("Write something first")
                            .clicked()
                    })
                    .inner;
                if f.saved_folder.is_some() {
                    show = ui.button("Show folder").on_hover_text("Open the saved note's folder").clicked();
                }
            });
        });
        if save {
            self.save_feedback();
        }
        if post {
            let f = &mut self.feedback;
            open_link(&feedback::github_link(f.kind, &f.text, crate::updates::VERSION));
            f.said = Some(Ok("Your web browser opens GitHub with the text filled in: check it, then press Create there to post it.".into()));
        }
        if show {
            if let Some(d) = &self.feedback.saved_folder {
                crate::open_folder(d);
            }
        }
        if !open {
            self.feedback.open = false;
            self.feedback.picture = None;
        }
    }

    /// Save note (FB2–FB4).
    fn save_feedback(&mut self) {
        let job = if self.feedback.with_job { crate::jobfile::to_text_on(&self.current_jobfile(), crate::updates::VERSION).ok().map(String::into_bytes) } else { None };
        let f = &self.feedback;
        let note = Note {
            kind: f.kind,
            text: f.text.clone(),
            when: self.clock.now(),
            who: self.who.clone(),
            version: crate::updates::VERSION.into(),
            system: feedback::system(),
            screen: f.screen.clone(),
            machine: self.machine_name(),
            job: self.job_name.clone(),
            crash: f.crash.clone(),
            attached: vec![],
        };
        let picture = if f.with_picture { f.picture.clone() } else { None };
        let r = feedback::save(&self.library_dir(), &note, &self.clock, picture.as_deref(), job.as_deref());
        let f = &mut self.feedback;
        match r {
            Ok(s) => {
                f.said = Some(Ok(s.message()));
                f.saved_folder = Some(s.folder);
                f.text.clear();
                f.with_job = false;
                f.with_picture = false;
                if f.crash.take().is_some() && self.shot.is_none() {
                    feedback::forget_crash(&pc_dir());
                }
            }
            Err(e) => f.said = Some(Err(e)),
        }
    }

    /// Screenshots (`--feedback`): the window open on made-up text.
    pub fn demo_feedback(&mut self) {
        let f = &mut self.feedback;
        f.open = true;
        f.kind = Kind::Problem;
        f.text = "Nest all put two DEMO-001 brackets on top of each other on sheet 2.\nI expected them side by side.".into();
        f.with_job = !self.parts.is_empty();
        self.feedback.screen = self.screen_name();
    }

    /// Screenshots (`--crash-note`): the note after a made-up crash.
    pub fn demo_crash_note(&mut self) {
        self.feedback.crash_card = Some(Crash { when: self.clock.now().saturating_sub(5 * 60), version: crate::updates::VERSION.into(), message: "index out of bounds".into(), place: "src/nest.rs:120".into() });
    }
}
