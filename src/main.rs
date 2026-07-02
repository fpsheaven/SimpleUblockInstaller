#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result, bail};
use eframe::egui;
use std::collections::BTreeSet;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

// --- Chrome ---
// uBlock Origin Lite (MV3). Full uBlock Origin is Manifest V2, which current Chrome
// refuses to install by ANY method ("unsupported manifest version"), so Lite is the
// only version that installs on Chrome. Force-installed via policy = silent, no admin.
const UBOL_ID: &str = "ddkjiahejlhfcafbddmgiahcphecmpfh";
const WEBSTORE_UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";
const FORCELIST_KEY: &str = r"HKCU\Software\Policies\Google\Chrome\ExtensionInstallForcelist";

// --- Firefox ---
// Firefox still supports MV2, so Firefox users get FULL uBlock Origin. A GitHub .xpi
// can't be installed from the command line (Firefox only installs extensions through its
// web flow), so we open the AMO add-on page and the user clicks "Add to Firefox".
const AMO_UBO_PAGE: &str = "https://addons.mozilla.org/firefox/addon/ublock-origin/";

const CREATE_NO_WINDOW: u32 = 0x08000000;

// Palette
const BG: egui::Color32 = egui::Color32::from_rgb(24, 25, 29);
const CONSOLE_BG: egui::Color32 = egui::Color32::from_rgb(16, 17, 20);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(46, 164, 104);
const ACCENT_HOVER: egui::Color32 = egui::Color32::from_rgb(56, 184, 120);
const TITLE: egui::Color32 = egui::Color32::from_rgb(238, 239, 242);
const MUTED: egui::Color32 = egui::Color32::from_rgb(140, 143, 150);
const LOG_TEXT: egui::Color32 = egui::Color32::from_rgb(158, 190, 170);
const ERR_RED: egui::Color32 = egui::Color32::from_rgb(226, 96, 96);
const LOVE: egui::Color32 = egui::Color32::from_rgb(228, 152, 178);

/// A background job (install or uninstall) that reports progress lines.
type Task = fn(&dyn Fn(&str)) -> Result<()>;

#[derive(Debug)]
enum WorkerMessage {
    Status(String),
    Done(Result<(), String>),
}

#[derive(Default)]
struct InstallerApp {
    receiver: Option<Receiver<WorkerMessage>>,
    installing: bool,
    busy_uninstall: bool,
    logs: Vec<String>,
    started: Option<Instant>,
    finished: bool,
    error: Option<String>,
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([430.0, 402.0])
            .with_min_inner_size([430.0, 402.0])
            .with_resizable(false)
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
                    .expect("failed to decode embedded app icon"),
            ),
        ..Default::default()
    };

    eframe::run_native(
        "Simple UBlock installer by @FPSHEAVEN",
        options,
        Box::new(|cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(InstallerApp::default()))
        }),
    )
}

fn configure_style(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = BG;
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(210, 212, 218));
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(18.0, 10.0);
    ctx.set_style(style);
}

impl eframe::App for InstallerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_worker_messages();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Simple UBlock installer by @FPSHEAVEN")
                        .size(17.0)
                        .strong()
                        .color(TITLE),
                );
                ui.label(
                    egui::RichText::new("Installs uBlock into Chrome and Firefox")
                        .size(11.5)
                        .color(MUTED),
                );
            });

            ui.add_space(16.0);

            // Install + Uninstall buttons (side by side, centered)
            let installing = self.installing;
            let busy_uninstall = self.busy_uninstall;
            let mut install_clicked = false;
            let mut uninstall_clicked = false;
            ui.horizontal(|ui| {
                let total = 160.0 + 10.0 + 130.0;
                ui.add_space(((ui.available_width() - total) * 0.5).max(0.0));

                // Install (green primary)
                ui.scope(|ui| {
                    let w = &mut ui.style_mut().visuals.widgets;
                    w.inactive.corner_radius = egui::CornerRadius::same(8);
                    w.hovered.corner_radius = egui::CornerRadius::same(8);
                    w.active.corner_radius = egui::CornerRadius::same(8);
                    w.noninteractive.corner_radius = egui::CornerRadius::same(8);
                    w.inactive.weak_bg_fill = ACCENT;
                    w.hovered.weak_bg_fill = ACCENT_HOVER;
                    w.active.weak_bg_fill = ACCENT_HOVER;
                    w.inactive.fg_stroke.color = egui::Color32::WHITE;
                    w.hovered.fg_stroke.color = egui::Color32::WHITE;
                    w.active.fg_stroke.color = egui::Color32::WHITE;
                    w.hovered.expansion = 0.0;
                    w.active.expansion = 0.0;
                    let label = if installing && !busy_uninstall {
                        "Installing…"
                    } else {
                        "Install"
                    };
                    let b = egui::Button::new(egui::RichText::new(label).size(15.0).strong())
                        .min_size(egui::vec2(160.0, 40.0));
                    install_clicked = ui.add_enabled(!installing, b).clicked();
                });

                ui.add_space(10.0);

                // Uninstall (muted-red secondary)
                ui.scope(|ui| {
                    let w = &mut ui.style_mut().visuals.widgets;
                    w.inactive.corner_radius = egui::CornerRadius::same(8);
                    w.hovered.corner_radius = egui::CornerRadius::same(8);
                    w.active.corner_radius = egui::CornerRadius::same(8);
                    w.noninteractive.corner_radius = egui::CornerRadius::same(8);
                    w.inactive.weak_bg_fill = egui::Color32::from_rgb(48, 34, 37);
                    w.hovered.weak_bg_fill = egui::Color32::from_rgb(70, 42, 46);
                    w.active.weak_bg_fill = egui::Color32::from_rgb(70, 42, 46);
                    w.inactive.fg_stroke.color = egui::Color32::from_rgb(224, 150, 150);
                    w.hovered.fg_stroke.color = egui::Color32::from_rgb(244, 176, 176);
                    w.active.fg_stroke.color = egui::Color32::from_rgb(244, 176, 176);
                    w.hovered.expansion = 0.0;
                    w.active.expansion = 0.0;
                    let label = if installing && busy_uninstall {
                        "Uninstalling…"
                    } else {
                        "Uninstall"
                    };
                    let b = egui::Button::new(egui::RichText::new(label).size(14.0))
                        .min_size(egui::vec2(130.0, 40.0));
                    uninstall_clicked = ui.add_enabled(!installing, b).clicked();
                });
            });
            if install_clicked {
                self.start_install();
            }
            if uninstall_clicked {
                self.start_uninstall();
            }

            ui.add_space(16.0);

            // Console
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                ui.label(egui::RichText::new("console").size(10.5).color(MUTED));
                if self.installing {
                    ui.add(egui::Spinner::new().size(12.0).color(ACCENT));
                }
            });
            ui.add_space(3.0);

            egui::Frame::new()
                .fill(CONSOLE_BG)
                .inner_margin(egui::Margin::same(10))
                .corner_radius(egui::CornerRadius::same(6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_height(96.0);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.logs.is_empty() {
                                ui.label(
                                    egui::RichText::new("› ready")
                                        .monospace()
                                        .size(11.5)
                                        .color(MUTED),
                                );
                            }
                            for line in &self.logs {
                                ui.label(
                                    egui::RichText::new(line)
                                        .monospace()
                                        .size(11.5)
                                        .color(LOG_TEXT),
                                );
                            }
                        });
                });

            // Under the final log
            if self.finished {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("I love you and do not give up")
                            .size(12.5)
                            .color(LOVE),
                    );
                });
            }

            if let Some(error) = &self.error {
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("✕").color(ERR_RED).strong());
                    ui.label(egui::RichText::new(error).size(11.5).color(ERR_RED));
                });
            }

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let total = 160.0 + 8.0 + 150.0;
                ui.add_space(((ui.available_width() - total) * 0.5).max(0.0));
                ui.scope(|ui| {
                    // Small link-style buttons.
                    let w = &mut ui.style_mut().visuals.widgets;
                    w.inactive.corner_radius = egui::CornerRadius::same(8);
                    w.hovered.corner_radius = egui::CornerRadius::same(8);
                    w.active.corner_radius = egui::CornerRadius::same(8);
                    w.inactive.weak_bg_fill = egui::Color32::from_rgb(38, 40, 46);
                    w.hovered.weak_bg_fill = egui::Color32::from_rgb(52, 54, 62);
                    w.active.weak_bg_fill = egui::Color32::from_rgb(52, 54, 62);
                    w.inactive.fg_stroke.color = egui::Color32::from_rgb(210, 212, 220);
                    w.hovered.fg_stroke.color = egui::Color32::WHITE;
                    w.active.fg_stroke.color = egui::Color32::WHITE;
                    w.hovered.expansion = 0.0;
                    w.active.expansion = 0.0;

                    let ubo = egui::Button::new(egui::RichText::new("uBlock Origin Lite").size(12.5))
                        .min_size(egui::vec2(160.0, 32.0));
                    if ui.add(ubo).clicked() {
                        open_url("https://github.com/uBlockOrigin/uBOL-home");
                    }

                    ui.add_space(8.0);

                    let x = egui::Button::new(egui::RichText::new("Follow us on X").size(12.5))
                        .min_size(egui::vec2(150.0, 32.0));
                    if ui.add(x).clicked() {
                        open_url("https://x.com/FPSHEAVEN");
                    }
                });
            });

            if self.installing {
                ctx.request_repaint();
            }
        });
    }
}

impl InstallerApp {
    fn start_install(&mut self) {
        self.busy_uninstall = false;
        self.start_task(run_install);
    }

    fn start_uninstall(&mut self) {
        self.busy_uninstall = true;
        self.start_task(run_uninstall);
    }

    fn start_task(&mut self, task: Task) {
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.installing = true;
        self.logs.clear();
        self.started = Some(Instant::now());
        self.finished = false;
        self.error = None;
        self.log("starting");

        thread::spawn(move || {
            let progress_sender = sender.clone();

            // Guarantee a Done message is always sent, even if the task panics,
            // otherwise the UI would spin on "Installing…" forever.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task(&|status: &str| {
                    let _ = progress_sender.send(WorkerMessage::Status(status.to_string()));
                })
            }));

            let result = match outcome {
                Ok(result) => result.map_err(|err| format!("{err:#}")),
                Err(_) => Err("The operation crashed unexpectedly (internal error).".to_string()),
            };

            let _ = sender.send(WorkerMessage::Done(result));
        });
    }

    fn log(&mut self, message: &str) {
        let secs = self.started.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
        self.logs.push(format!("[{secs:>4.1}s] {message}"));
    }

    fn receive_worker_messages(&mut self) {
        if let Some(receiver) = self.receiver.take() {
            let mut keep_receiver = true;

            for message in receiver.try_iter() {
                match message {
                    WorkerMessage::Status(status) => self.log(&status),
                    WorkerMessage::Done(result) => {
                        self.installing = false;
                        keep_receiver = false;
                        match result {
                            Ok(()) => {
                                self.log("done");
                                self.finished = true;
                            }
                            Err(error) => {
                                self.log(&format!("error: {error}"));
                                self.error = Some(error);
                            }
                        }
                    }
                }
            }

            if keep_receiver {
                self.receiver = Some(receiver);
            }
        }
    }
}

fn run_install(progress: &dyn Fn(&str)) -> Result<()> {
    let mut handled = false;

    // --- Chrome: uBlock Origin Lite, force-installed by policy (silent, no admin) ---
    if let Some(chrome) = find_chrome() {
        progress("Chrome found — writing extension policy (uBlock Origin Lite)");
        let entry = format!("{UBOL_ID};{WEBSTORE_UPDATE_URL}");
        ensure_forcelist_entry(&entry)?;
        progress("policy written — HKCU, no admin needed");
        if chrome_is_running() {
            progress("restarting Chrome so it installs now");
            let _ = close_chrome();
            thread::sleep(Duration::from_millis(600));
            let _ = Command::new(&chrome).creation_flags(CREATE_NO_WINDOW).spawn();
        } else {
            progress("Chrome closed — it installs on next launch");
        }
        handled = true;
    } else {
        progress("Chrome not found — skipped");
    }

    // --- Firefox: full uBlock Origin via the AMO "Add to Firefox" page (2 clicks) ---
    if let Some(firefox) = find_firefox() {
        progress("Firefox found — opening the uBlock Origin add-on page");
        Command::new(&firefox)
            .arg(AMO_UBO_PAGE)
            .spawn()
            .context("Failed to launch Firefox")?;
        progress("in Firefox: click \u{201C}Add to Firefox\u{201D}, then \u{201C}Add\u{201D}");
        handled = true;
    } else {
        progress("Firefox not found — skipped");
    }

    if !handled {
        bail!("Neither Chrome nor Firefox was found on this PC.");
    }

    Ok(())
}

fn run_uninstall(progress: &dyn Fn(&str)) -> Result<()> {
    let mut handled = false;

    // --- Chrome: drop our force-install policy entry; Chrome then removes uBOL ---
    if let Some(chrome) = find_chrome() {
        let entry = format!("{UBOL_ID};{WEBSTORE_UPDATE_URL}");
        progress("removing the Chrome extension policy");
        if remove_forcelist_entry(&entry)? {
            progress("policy removed");
            if chrome_is_running() {
                progress("restarting Chrome to remove uBlock");
                let _ = close_chrome();
                thread::sleep(Duration::from_millis(600));
                let _ = Command::new(&chrome).creation_flags(CREATE_NO_WINDOW).spawn();
            } else {
                progress("Chrome closed — uBlock is removed on next launch");
            }
        } else {
            progress("no uBlock policy set for Chrome — nothing to remove");
        }
        handled = true;
    } else {
        progress("Chrome not found — skipped");
    }

    // --- Firefox: uBO was installed by hand, so open about:addons to remove it there ---
    if let Some(firefox) = find_firefox() {
        progress("opening Firefox add-ons — click Remove on uBlock Origin");
        Command::new(&firefox)
            .arg("about:addons")
            .spawn()
            .context("Failed to launch Firefox")?;
        handled = true;
    } else {
        progress("Firefox not found — skipped");
    }

    if !handled {
        bail!("Neither Chrome nor Firefox was found on this PC.");
    }

    Ok(())
}

/// Add our uBlock Origin Lite entry to Chrome's ExtensionInstallForcelist policy without
/// clobbering any existing entries. Chrome reads the values as a list named "1", "2", …,
/// so we append at the first free integer slot (and skip if it's already present).
fn ensure_forcelist_entry(entry: &str) -> Result<()> {
    let output = Command::new("reg")
        .args(["query", FORCELIST_KEY])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("Failed to read Chrome's extension policy")?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut used = BTreeSet::new();
    for line in text.lines() {
        if let Some((name, data)) = line.trim().split_once("REG_SZ") {
            if data.trim() == entry {
                return Ok(()); // already present; nothing to do
            }
            if let Ok(n) = name.trim().parse::<u32>() {
                used.insert(n);
            }
        }
    }

    let mut idx = 1u32;
    while used.contains(&idx) {
        idx += 1;
    }

    let status = Command::new("reg")
        .args([
            "add",
            FORCELIST_KEY,
            "/v",
            &idx.to_string(),
            "/t",
            "REG_SZ",
            "/d",
            entry,
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .context("Failed to write the Chrome extension policy")?;

    if !status.success() {
        bail!("Writing the Chrome policy failed. Please try running the installer again.");
    }

    Ok(())
}

/// Remove our uBlock Origin Lite entry from Chrome's ExtensionInstallForcelist policy.
/// Returns true if it was present. If ours was the only entry, the now-empty key that the
/// installer created is removed too (a clean revert); if other force-installed extensions
/// are listed, only our specific value is deleted so those survive.
fn remove_forcelist_entry(entry: &str) -> Result<bool> {
    let output = Command::new("reg")
        .args(["query", FORCELIST_KEY])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("Failed to read Chrome's extension policy")?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut our_value = None;
    let mut total = 0u32;
    for line in text.lines() {
        if let Some((name, data)) = line.trim().split_once("REG_SZ") {
            total += 1;
            if data.trim() == entry {
                our_value = Some(name.trim().to_string());
            }
        }
    }

    let Some(name) = our_value else {
        return Ok(false);
    };

    let delete_args: Vec<&str> = if total <= 1 {
        // Ours is the only entry — remove the whole key the installer created.
        vec!["delete", FORCELIST_KEY, "/f"]
    } else {
        // Other extensions are force-installed too — only remove our value.
        vec!["delete", FORCELIST_KEY, "/v", name.as_str(), "/f"]
    };

    let status = Command::new("reg")
        .args(delete_args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .context("Failed to remove the Chrome extension policy")?;

    if !status.success() {
        bail!("Removing the Chrome policy failed.");
    }

    Ok(true)
}

fn find_chrome() -> Option<PathBuf> {
    browser_path(r"Google\Chrome\Application\chrome.exe")
}

fn find_firefox() -> Option<PathBuf> {
    browser_path(r"Mozilla Firefox\firefox.exe")
}

fn browser_path(relative: &str) -> Option<PathBuf> {
    [
        std::env::var_os("PROGRAMFILES"),
        std::env::var_os("PROGRAMFILES(X86)"),
        std::env::var_os("LOCALAPPDATA"),
    ]
    .into_iter()
    .flatten()
    .map(|base| PathBuf::from(base).join(relative))
    .find(|path| path.is_file())
}

fn chrome_is_running() -> bool {
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq chrome.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("chrome.exe")
        })
        .unwrap_or(false)
}

fn close_chrome() -> Result<()> {
    Command::new("taskkill")
        .args(["/IM", "chrome.exe", "/F", "/T"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .context("Failed to close Chrome")?;

    Ok(())
}

fn open_url(url: &str) {
    // explorer.exe opens a URL in the default browser (GUI process, no console flash).
    let _ = Command::new("explorer").arg(url).spawn();
}
