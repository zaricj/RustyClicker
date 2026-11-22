//! GUI. egui is reactive: it only repaints on input or explicit request, so idle CPU is ~0.

use crate::clicker::{new_enigo, Clicker};
use crate::config::{ClickType, Config, Hotkey, MouseButton, Settings};
use eframe::egui::{self, Button, ComboBox, DragValue, RichText, TextEdit};
use enigo::{Enigo, Mouse};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PICK_SECONDS: u64 = 3;

enum NameDialog {
    SaveAs(String),
    Rename(String),
}

pub struct App {
    cfg: Config,          // mirrors what's on disk
    profile: String,      // selected profile name
    cur: Settings,        // working copy shown in the UI
    clicker: Clicker,
    hk_mgr: Option<GlobalHotKeyManager>,
    hk_active: Option<HotKey>,
    hk_rx: mpsc::Receiver<()>,
    name_dialog: Option<NameDialog>,
    hotkey_dialog: Option<Hotkey>,
    picking: Option<Instant>,
    enigo: Option<Enigo>,
    status: String,
    window_pos: Option<[f32; 2]>, // last known non-minimized position, saved on exit
    trim_at: Option<Instant>,     // when to do the one-time post-startup memory trim
    was_minimized: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let cfg = Config::load();
        let profile = cfg.last_profile.clone();
        let cur = cfg.profiles.get(&profile).cloned().unwrap_or_default();

        // Hotkey events arrive on another thread; forward + wake the UI.
        let (tx, hk_rx) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.state == HotKeyState::Pressed {
                let _ = tx.send(());
                ctx.request_repaint();
            }
        }));

        let mut app = Self {
            cfg,
            profile,
            cur,
            clicker: Clicker::new(),
            hk_mgr: None,
            hk_active: None,
            hk_rx,
            name_dialog: None,
            hotkey_dialog: None,
            picking: None,
            enigo: None,
            status: String::new(),
            window_pos: None,
            trim_at: Some(Instant::now() + Duration::from_secs(2)),
            was_minimized: false,
        };
        match GlobalHotKeyManager::new() {
            Ok(m) => {
                app.hk_mgr = Some(m);
                if let Err(e) = app.register_hotkey(app.cfg.hotkey) {
                    app.status = e;
                }
            }
            Err(e) => app.status = format!("Global hotkey unavailable: {e}"),
        }
        app
    }

    fn register_hotkey(&mut self, hk: Hotkey) -> Result<(), String> {
        let mgr = self.hk_mgr.as_ref().ok_or("Global hotkey unavailable")?;
        let mut mods = Modifiers::empty();
        mods.set(Modifiers::CONTROL, hk.ctrl);
        mods.set(Modifiers::SHIFT, hk.shift);
        mods.set(Modifiers::ALT, hk.alt);
        let new = HotKey::new((!mods.is_empty()).then_some(mods), fkey_code(hk.fkey));
        if let Some(old) = self.hk_active.take() {
            let _ = mgr.unregister(old);
        }
        mgr.register(new).map_err(|e| format!("Couldn't register {hk}: {e}"))?;
        self.hk_active = Some(new);
        Ok(())
    }

    fn persist(&mut self, ok_msg: &str) {
        self.status = match self.cfg.save() {
            Ok(()) => ok_msg.to_owned(),
            Err(e) => format!("Save failed: {e}"),
        };
    }

    fn dirty(&self) -> bool {
        self.cfg.profiles.get(&self.profile) != Some(&self.cur)
    }

    fn toggle(&mut self, ctx: &egui::Context) {
        if self.clicker.is_running() {
            self.clicker.stop();
        } else {
            self.picking = None;
            self.clicker.start(&self.cur, ctx.clone());
        }
    }

    fn select_profile(&mut self, name: String) {
        if let Some(s) = self.cfg.profiles.get(&name) {
            self.cur = s.clone();
            self.profile = name.clone();
            self.cfg.last_profile = name;
            self.persist("");
        }
    }

    // ---------- UI sections ----------

    fn profile_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(dlg) = &mut self.name_dialog {
                let (label, buf) = match dlg {
                    NameDialog::SaveAs(b) => ("Save as:", b),
                    NameDialog::Rename(b) => ("Rename to:", b),
                };
                ui.label(label);
                let r = ui.add(TextEdit::singleline(buf).desired_width(170.0));
                r.request_focus();
                let name = buf.trim().to_owned();
                let valid = !name.is_empty() && !self.cfg.profiles.contains_key(&name);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let ok = ui.add_enabled(valid, Button::new("OK")).clicked() || (enter && valid);
                let cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
                if ok {
                    match self.name_dialog.take() {
                        Some(NameDialog::SaveAs(_)) => {
                            self.cfg.profiles.insert(name.clone(), self.cur.clone());
                        }
                        Some(NameDialog::Rename(_)) => {
                            if let Some(s) = self.cfg.profiles.remove(&self.profile) {
                                self.cfg.profiles.insert(name.clone(), s);
                            }
                        }
                        None => {}
                    }
                    self.profile = name.clone();
                    self.cfg.last_profile = name.clone();
                    self.persist(&format!("Saved \"{name}\""));
                } else if cancel {
                    self.name_dialog = None;
                }
                return;
            }

            ui.label("Profile:");
            let mut selected = self.profile.clone();
            ComboBox::from_id_salt("profile")
                .width(140.0)
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for name in self.cfg.profiles.keys() {
                        ui.selectable_value(&mut selected, name.clone(), name);
                    }
                });
            if selected != self.profile {
                self.select_profile(selected);
            }
            let dirty = self.dirty();
            if ui.add_enabled(dirty, Button::new("Save")).on_hover_text("Save changes to this profile").clicked() {
                self.cfg.profiles.insert(self.profile.clone(), self.cur.clone());
                self.cfg.last_profile = self.profile.clone();
                self.persist(&format!("Saved \"{}\"", self.profile));
            }
            if ui.button("Save as…").clicked() {
                self.name_dialog = Some(NameDialog::SaveAs(String::new()));
            }
            if ui.button("Rename").clicked() {
                self.name_dialog = Some(NameDialog::Rename(self.profile.clone()));
            }
            let can_delete = self.cfg.profiles.len() > 1;
            if ui.add_enabled(can_delete, Button::new("Delete")).clicked() {
                self.cfg.profiles.remove(&self.profile);
                let first = self.cfg.profiles.keys().next().cloned().unwrap_or_default();
                let deleted = std::mem::replace(&mut self.profile, String::new());
                self.select_profile(first);
                self.status = format!("Deleted \"{deleted}\"");
            }
            if dirty {
                ui.label(RichText::new("●").color(egui::Color32::from_rgb(230, 150, 30)))
                    .on_hover_text("Unsaved changes");
            }
        });
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.cur;

        section(ui, "Click interval", |ui| {
            ui.horizontal(|ui| {
                num(ui, &mut s.hours, 999);
                ui.label("hours");
                num(ui, &mut s.mins, 59);
                ui.label("mins");
                num(ui, &mut s.secs, 59);
                ui.label("secs");
                num(ui, &mut s.millis, 999);
                ui.label("ms");
            });
        });

        ui.columns(2, |cols| {
            section(&mut cols[0], "Click options", |ui| {
                egui::Grid::new("opts").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("Mouse button:");
                    ComboBox::from_id_salt("btn")
                        .width(80.0)
                        .selected_text(format!("{:?}", s.button))
                        .show_ui(ui, |ui| {
                            for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                                ui.selectable_value(&mut s.button, b, format!("{b:?}"));
                            }
                        });
                    ui.end_row();
                    ui.label("Click type:");
                    ComboBox::from_id_salt("type")
                        .width(80.0)
                        .selected_text(format!("{:?}", s.click_type))
                        .show_ui(ui, |ui| {
                            for t in [ClickType::Single, ClickType::Double] {
                                ui.selectable_value(&mut s.click_type, t, format!("{t:?}"));
                            }
                        });
                    ui.end_row();
                });
            });
            section(&mut cols[1], "Click repeat", |ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.repeat_until_stopped, false, "Repeat");
                    ui.add_enabled(
                        !s.repeat_until_stopped,
                        DragValue::new(&mut s.repeat_count).range(1..=u32::MAX),
                    );
                    ui.label("times");
                });
                ui.radio_value(&mut s.repeat_until_stopped, true, "Repeat until stopped");
            });
        });

        section(ui, "Cursor position", |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.use_fixed_pos, false, "Current location");
                ui.add_space(12.0);
                ui.radio_value(&mut s.use_fixed_pos, true, "");
                let label = match self.picking {
                    Some(t) => {
                        let left = PICK_SECONDS.saturating_sub(t.elapsed().as_secs());
                        format!("Move mouse… {left}")
                    }
                    None => "Pick location".to_owned(),
                };
                if ui
                    .add_sized([110.0, 22.0], Button::new(label))
                    .on_hover_text(format!("Captures the cursor position after {PICK_SECONDS} s"))
                    .clicked()
                {
                    self.picking = if self.picking.is_some() { None } else { Some(Instant::now()) };
                }
                ui.label("X");
                ui.add(DragValue::new(&mut s.x));
                ui.label("Y");
                ui.add(DragValue::new(&mut s.y));
            });
        });
    }

    fn update_picking(&mut self, ctx: &egui::Context) {
        let Some(t) = self.picking else { return };
        if self.enigo.is_none() {
            match new_enigo() {
                Ok(e) => self.enigo = Some(e),
                Err(e) => {
                    self.status = format!("Can't read cursor: {e}");
                    self.picking = None;
                    return;
                }
            }
        }
        if let Some(Ok((x, y))) = self.enigo.as_ref().map(|e| e.location()) {
            self.cur.x = x;
            self.cur.y = y;
        }
        if t.elapsed() >= Duration::from_secs(PICK_SECONDS) {
            self.picking = None;
            self.cur.use_fixed_pos = true;
            self.status = format!("Picked ({}, {})", self.cur.x, self.cur.y);
        } else {
            ctx.request_repaint_after(Duration::from_millis(33)); // only while picking
        }
    }

    fn hotkey_modal(&mut self, ctx: &egui::Context) {
        let Some(hk) = &mut self.hotkey_dialog else { return };
        let mut apply = false;
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::unique("hotkey_modal")).show(ctx, |ui| {
            ui.heading("Start / Stop hotkey");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut hk.ctrl, "Ctrl");
                ui.checkbox(&mut hk.shift, "Shift");
                ui.checkbox(&mut hk.alt, "Alt");
                ComboBox::from_id_salt("fkey")
                    .width(60.0)
                    .selected_text(format!("F{}", hk.fkey))
                    .show_ui(ui, |ui| {
                        for k in 1..=12u8 {
                            ui.selectable_value(&mut hk.fkey, k, format!("F{k}"));
                        }
                    });
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                apply = ui.button("OK").clicked();
                close = ui.button("Cancel").clicked();
            });
        });
        if modal.should_close() {
            close = true;
        }
        if apply {
            let new = *hk;
            let old = self.cfg.hotkey;
            match self.register_hotkey(new) {
                Ok(()) => {
                    self.cfg.hotkey = new;
                    self.persist(&format!("Hotkey set to {new}"));
                }
                Err(e) => {
                    let _ = self.register_hotkey(old);
                    self.status = e;
                }
            }
            close = true;
        }
        if close {
            self.hotkey_dialog = None;
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Runs even while minimized, so the hotkey always works.
        let presses = self.hk_rx.try_iter().count();
        if presses % 2 == 1 {
            self.toggle(ctx);
        }
        self.update_picking(ctx);

        // Track position; skip while minimized (Windows reports ~-32000 then).
        let minimized = ctx.input(|i| {
            let vp = i.viewport();
            if vp.minimized != Some(true)
                && let Some(r) = vp.outer_rect
            {
                self.window_pos = Some([r.min.x, r.min.y]);
            }
            vp.minimized == Some(true)
        });

        // Release memory that's only touched during startup (mostly OpenGL driver
        // init) once the UI has settled, and again whenever the window is minimized.
        match self.trim_at {
            Some(at) if Instant::now() >= at => {
                self.trim_at = None;
                trim_working_set();
            }
            Some(at) => ctx.request_repaint_after(at - Instant::now()),
            None => {}
        }
        if minimized && !self.was_minimized {
            trim_working_set();
        }
        self.was_minimized = minimized;
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Only the position is written: self.cfg mirrors disk, so unsaved
        // profile edits (kept in self.cur) are not persisted by this.
        if self.window_pos.is_some() && self.window_pos != self.cfg.window_pos {
            self.cfg.window_pos = self.window_pos;
            let _ = self.cfg.save();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let running = self.clicker.is_running();
        let hk = self.cfg.hotkey;

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_enabled_ui(!running && self.hotkey_dialog.is_none(), |ui| {
                self.profile_bar(ui);
                ui.add_space(4.0);
                self.settings_ui(ui);
            });

            ui.add_space(6.0);
            let big = [ui.available_width() / 2.0 - 4.0, 40.0];
            ui.horizontal(|ui| {
                if ui.add_enabled(!running, Button::new(format!("Start ({hk})")).min_size(big.into())).clicked()
                    || ui.add_enabled(running, Button::new(format!("Stop ({hk})")).min_size(big.into())).clicked()
                {
                    self.toggle(&ctx);
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!running, Button::new("Hotkey setting").min_size(big.into())).clicked() {
                    self.hotkey_dialog = Some(self.cfg.hotkey);
                }
                ui.add_enabled(false, Button::new("Record & Playback").min_size(big.into()))
                    .on_disabled_hover_text("Not implemented yet");
            });

            if !self.status.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&self.status).small().weak());
            }
        });

        self.hotkey_modal(&ctx);
    }
}

// ---------- helpers ----------

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong());
        ui.add_space(2.0);
        add(ui);
    });
}

fn num(ui: &mut egui::Ui, v: &mut u32, max: u32) {
    ui.add_sized([56.0, 20.0], DragValue::new(v).range(0..=max).speed(0.2));
}

fn fkey_code(n: u8) -> Code {
    match n {
        1 => Code::F1,
        2 => Code::F2,
        3 => Code::F3,
        4 => Code::F4,
        5 => Code::F5,
        7 => Code::F7,
        8 => Code::F8,
        9 => Code::F9,
        10 => Code::F10,
        11 => Code::F11,
        12 => Code::F12,
        _ => Code::F6,
    }
}

/// Ask Windows to page out memory the process isn't actively using. Pages that
/// are needed again come back transparently; startup-only memory (GPU driver
/// init, font loading) stays out, which is what Task Manager reports.
#[cfg(windows)]
fn trim_working_set() {
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessWorkingSetSize(process: isize, min: usize, max: usize) -> i32;
    }
    // (usize::MAX, usize::MAX) = "trim as much as possible" per the Win32 docs.
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

#[cfg(not(windows))]
fn trim_working_set() {}
