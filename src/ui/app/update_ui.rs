//! Self-update UI: the Check for updates dialog (progress, errors, Restart now), the status bar's
//! "Restart to update" button, and the automatic checks. The work itself is in crate::update.

use super::*;
use crate::update::{self, Status};

/// Automatic checks: at start-up, then every this many minutes.
const CHECK_EVERY_MIN: u32 = 6 * 60;

impl App {
    /// Every frame: automatic checks, and a toast once a download is ready.
    pub(super) fn update_housekeeping(&mut self, ctx: &egui::Context) {
        let schedule = update::Schedule { on_startup: true, every_min: Some(CHECK_EVERY_MIN) };
        update::auto_check(ctx, self.s.auto_update, &schedule);
        if let Status::Ready(tag) = update::status() {
            if self.update_announced.as_deref() != Some(tag.as_str()) {
                self.update_announced = Some(tag.clone());
                if !self.update_dialog {
                    self.notify(ctx, format!("ASM Test Bench {tag} is downloaded. Restart to update."), GREEN);
                }
            }
        }
    }

    pub(super) fn open_update_dialog(&mut self, ctx: &egui::Context) {
        self.update_dialog = true;
        if !matches!(update::status(), Status::Checking | Status::Downloading { .. }) {
            update::check(ctx);
        }
    }

    /// Closes the window to restart into the downloaded version (open tabs are kept with the
    /// settings, as on any quit).
    fn restart_to_update(&mut self, ctx: &egui::Context) {
        update::RELAUNCH.store(true, std::sync::atomic::Ordering::Relaxed);
        self.update_dialog = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Status bar (right side): shown while an update waits for a restart.
    pub(super) fn update_notice(&mut self, ui: &mut Ui) {
        let Status::Ready(tag) = update::status() else { return };
        let b = egui::Button::new(RichText::new("Restart to update").color(Color32::WHITE).size(12.0)).fill(ACCENT);
        let r = ui.add(b).on_hover_text(format!("ASM Test Bench {tag} is downloaded. It installs when ASM Test Bench restarts."));
        if r.clicked() {
            self.restart_to_update(&ui.ctx().clone());
        }
        ui.add_space(8.0);
    }

    pub(super) fn update_dialog(&mut self, ctx: &egui::Context) {
        if !self.update_dialog {
            return;
        }
        let mut status = update::status();
        if status == Status::Idle {
            // Opened by --check-updates: start the check.
            update::check(ctx);
            status = update::status();
        }
        let busy = matches!(status, Status::Checking | Status::Downloading { .. });
        let mut close = false;
        let mut restart = false;
        let mut retry = false;
        let m = egui::Modal::new(egui::Id::unique("update_dialog")).frame(egui::Frame::new().fill(PANEL).corner_radius(10).stroke(Stroke::new(1.0, BORDER)).inner_margin(20)).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new("Check for updates").size(18.0).family(bold()).color(Color32::WHITE));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("This is version {}.", update::VERSION)).color(TEXT_DIM));
            if let Some(exe) = update::target_exe() {
                ui.label(RichText::new(format!("Updates {}", exe.display())).size(12.0).color(TEXT_FAINT));
            }
            ui.add_space(12.0);
            match &status {
                Status::Idle | Status::Checking => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Checking GitHub for a new release…").color(TEXT));
                    });
                }
                Status::Downloading { tag, done, total } => {
                    ui.label(RichText::new(format!("Downloading {tag}…")).color(TEXT));
                    ui.add_space(6.0);
                    let (frac, text) = if *total > 0 {
                        (*done as f32 / *total as f32, format!("{} of {}", update::mb(*done), update::mb(*total)))
                    } else {
                        (0.0, update::mb(*done))
                    };
                    ui.add(egui::ProgressBar::new(frac.clamp(0.0, 1.0)).text(text).fill(ACCENT).desired_height(18.0));
                }
                Status::UpToDate(tag) => {
                    ui.label(RichText::new("✔ ASM Test Bench is up to date.").color(GREEN));
                    ui.label(RichText::new(format!("The latest release is {tag}.")).size(12.5).color(TEXT_FAINT));
                }
                Status::Ready(tag) => {
                    ui.label(RichText::new(format!("✔ {tag} is downloaded.")).color(GREEN));
                    ui.label(RichText::new("Restart ASM Test Bench to update. It also installs the next time you quit.").color(TEXT));
                }
                Status::Failed(e) => {
                    ui.label(RichText::new("Couldn't check for updates").color(RED));
                    ui.add_space(4.0);
                    ui.label(RichText::new(e).color(TEXT));
                }
            }
            if update::is_dev_build() {
                ui.add_space(8.0);
                ui.label(RichText::new("This is a development build (run from a cargo target folder); automatic checks are off for it.").size(12.0).color(TEXT_FAINT));
            }
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &status {
                Status::Ready(_) => {
                    if ui.add(egui::Button::new(RichText::new("Restart now").color(Color32::WHITE)).fill(ACCENT).min_size(egui::vec2(110.0, 28.0))).clicked() {
                        restart = true;
                    }
                    if ui.add(egui::Button::new("Later").min_size(egui::vec2(70.0, 28.0))).clicked() {
                        close = true;
                    }
                }
                Status::Failed(_) => {
                    if ui.add(egui::Button::new("Close").min_size(egui::vec2(70.0, 28.0))).clicked() {
                        close = true;
                    }
                    if ui.add(egui::Button::new("Try again").min_size(egui::vec2(90.0, 28.0))).clicked() {
                        retry = true;
                    }
                }
                _ => {
                    // Closing doesn't stop a check or download; it carries on in the background.
                    let label = if busy { "Hide" } else { "Close" };
                    if ui.add(egui::Button::new(label).min_size(egui::vec2(70.0, 28.0))).clicked() {
                        close = true;
                    }
                }
            });
        });
        if busy {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if m.should_close() {
            close = true;
        }
        if retry {
            update::check(ctx);
        }
        if restart {
            self.restart_to_update(ctx);
        } else if close {
            self.update_dialog = false;
        }
    }
}
