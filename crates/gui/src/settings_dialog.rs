//! The Settings dialog, of the GUI's own settings: ffmpeg, where
//! results go, and what new files start with. Changes apply at once, and
//! are kept in `config.toml` at once too: as typing or dragging pauses,
//! and only what is typed that makes sense.

use std::path::{Path, PathBuf};

use crate::config::{self, Accent, Config};
use eframe::egui;
use tgradish_core::CancelToken;
use tgradish_core::backend::Backend;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::presets::{Format, Presets};

use crate::app::App;
use crate::file_settings;
use crate::l10n::{self, fl};
use crate::media::Task;
use crate::theme;
use crate::widgets::{self, note, row, segments};

/// Where something was drawn, nowhere until it is.
#[derive(Clone, Copy, Debug)]
struct Drawn(egui::Rect);

impl Default for Drawn {
    fn default() -> Drawn {
        Drawn(egui::Rect::NOTHING)
    }
}

#[derive(Default)]
pub(crate) struct SettingsDialog {
    open: bool,
    /// What checking ffmpeg found: whether it is ready, and what to say.
    ffmpeg_status: Option<(bool, String)>,
    /// Checking ffmpeg, which runs it, on a thread of its own: stopped if
    /// the dialog is opened again or the GUI closes.
    checking: Option<Task<(bool, String)>>,
    /// What writing the settings did last: where they went, or why they
    /// didn't.
    saved: Option<Result<String, String>>,
    /// The settings as they were last written, or as the dialog opened.
    written: Option<Config>,
    /// The settings as the last frame left them, and when they last
    /// changed: a change waits to be written while it is typed or dragged.
    seen: Option<(Config, f64)>,
    /// The paths typed in the fields of the ffmpeg program and of the
    /// folder for results.
    ffmpeg_typed: Typed,
    folder_typed: Typed,
    /// Reset to defaults was clicked, which `App` takes up, as it may
    /// be undone from its message.
    pub(crate) reset: bool,
    /// What the fields show while empty, found once the dialog opens: the
    /// ffmpeg on PATH, and a folder results could go to.
    placeholders: (String, String),
    /// A file dialog asked for, which `App` opens.
    pub(crate) pick: Option<crate::files::Pick>,
    /// Results are to go in a folder not chosen yet: kept here, as the
    /// settings never hold a folder without a name.
    folder_unchosen: bool,
    /// A default preset picked for a format, which `App` takes up, as
    /// the items that follow it may then fit another way.
    pub(crate) default_preset: Option<(Format, String)>,
    /// The settings file was written, which `App` takes up, as what
    /// it said of the file is past.
    pub(crate) wrote: bool,
    /// The accent colour of one's own as typed, `#rrggbb`, while it is.
    accent_typed: String,
    /// The scale's button clicked this frame, where it is, or nowhere for
    /// the one back to 100%.
    stepped: Option<egui::Rect>,
    /// Where the scale's button last clicked is on the screen, in pixels,
    /// and how far it is from the dialog's corner, in points, with the
    /// settings scrolled to their top: the dialog is kept so that the
    /// button stays there as everything grows or shrinks around it, under
    /// the pointer, rather than in the window's middle, and where it can't
    /// move far enough within the window, its settings are scrolled.
    pinned: Option<(egui::Pos2, egui::Vec2)>,
    /// When the scale's button was last clicked.
    stepped_at: f64,
    /// Where the scale's row is, as last drawn, which the pointer keeps the
    /// dialog pinned near.
    scale_row: Option<egui::Rect>,
    /// Where the dialog's corner was as it went back to the window's middle
    /// from where it was pinned, and when.
    returning: Option<(egui::Pos2, f64)>,
    /// Where the dialog was drawn last.
    drawn: Drawn,
    /// How the settings were scrolled as last drawn.
    scrolled: Scrolled,
}

/// How the dialog's settings were scrolled, in points: how far, how tall
/// they all are, how much of them shows, and how far under the dialog's
/// top that starts.
#[derive(Clone, Copy, Debug, Default)]
struct Scrolled {
    offset: f32,
    content: f32,
    shown: f32,
    top: f32,
}

impl SettingsDialog {
    pub(crate) fn open(&mut self) {
        let ffmpeg = match tgradish_core::ffmpeg::Ffmpeg::on_path() {
            Ok(found) => fl!("ffmpeg-found", path = found.ffmpeg.display().to_string()),
            Err(_) if cfg!(windows) => fl!("for-example", example = r"C:\ffmpeg\bin"),
            Err(_) => fl!("for-example", example = "/usr/local/bin/ffmpeg"),
        };
        let folder = tgradish_core::paths::pictures_dir()
            .map(|dir| fl!("for-example", example = dir.join("Stickers").display().to_string()))
            .unwrap_or_default();
        // `pinned` goes: the dialog opens in the window's middle again
        *self = SettingsDialog {
            open: true,
            placeholders: (ffmpeg, folder),
            ..SettingsDialog::default()
        };
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Forgets what is typed and not taken, and a folder to be chosen, as
    /// the settings are put back to their defaults, or back from them.
    pub(crate) fn forget_typed(&mut self) {
        (self.ffmpeg_typed, self.folder_typed) = Default::default();
        self.folder_unchosen = false;
    }
}

/// A path typed in a field, and the setting it shows: what is typed is
/// kept while it isn't a path the setting can take, and follows the
/// setting once that changes otherwise, as by Choose….
#[derive(Default)]
struct Typed {
    text: String,
    of: Option<Option<PathBuf>>,
}

/// A field of a path, for `setting`, which takes what is typed where
/// `problem` finds nothing wrong with it: an empty field leaves it unset.
/// Gives what is wrong with what is typed, which keeps the setting as it
/// was, and whether the setting changed.
fn path_field(
    ui: &mut egui::Ui,
    typed: &mut Typed,
    setting: &mut Option<PathBuf>,
    placeholder: &str,
    problem: impl Fn(&Path) -> Option<String>,
) -> (Option<String>, bool) {
    if typed.of.as_ref() != Some(setting) {
        typed.text = setting.as_ref().map(|path| path.display().to_string()).unwrap_or_default();
        typed.of = Some(setting.clone());
    }
    let edit = widgets::text_field(&mut typed.text).hint_text(placeholder).desired_width(280.0);
    let field = ui.add(edit);
    let trimmed = typed.text.trim();
    let path = (!trimmed.is_empty()).then(|| PathBuf::from(trimmed));
    let wrong = path.as_deref().and_then(problem);
    let mut changed = false;
    if field.changed() && wrong.is_none() && path != *setting {
        *setting = path.clone();
        typed.of = Some(path);
        changed = true;
    }
    (wrong, changed)
}

/// What is wrong with `path` as the ffmpeg program: it must be there,
/// unless it is a name alone, which is looked for on PATH.
fn ffmpeg_problem(path: &Path) -> Option<String> {
    let named = path.components().count() == 1 && !path.has_root();
    (!named && !path.exists()).then(|| fl!("ffmpeg-program", "missing"))
}

/// What is wrong with `path` as the folder for results: it must be whole,
/// and not a file.
fn folder_problem(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        Some(fl!("in-a-folder", "relative"))
    } else if path.is_file() {
        Some(fl!("in-a-folder", "file"))
    } else {
        None
    }
}

/// What is wrong with a field's content, said under it.
fn problem(ui: &mut egui::Ui, wrong: Option<String>) {
    if let Some(wrong) = wrong {
        let text = egui::RichText::new(wrong).color(theme::bad(ui.visuals())).size(12.5);
        ui.add(egui::Label::new(text).wrap());
    }
}

/// How long a change waits to be written while it is typed or dragged.
const SETTLE: f64 = 0.6;

/// Writes `config` to `config_path` as it changes, once typing or dragging
/// pauses, or at once as the dialog `closing` does.
fn keep(
    ctx: &egui::Context,
    dialog: &mut SettingsDialog,
    config: &mut Config,
    config_path: Option<&Path>,
    closing: bool,
) {
    let now = ctx.input(|input| input.time);
    let written = dialog.written.get_or_insert_with(|| config.clone());
    let (seen, changed_at) = dialog.seen.get_or_insert_with(|| (config.clone(), now));
    if *seen != *config {
        (*seen, *changed_at) = (config.clone(), now);
    }
    if *written == *config {
        return;
    }
    let busy = ctx.text_edit_focused() || ctx.input(|input| input.pointer.any_down());
    let waited = now - *changed_at;
    if busy && waited < SETTLE && !closing {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE - waited));
        return;
    }
    dialog.saved = Some(match config_path {
        Some(path) => save(config, path),
        None => Err(fl!("no-home")),
    });
    dialog.wrote = matches!(dialog.saved, Some(Ok(_)));
    // tried once, whether it worked or not, until it changes again
    dialog.written = Some(config.clone());
}

/// Saves `config` to `path`, or says why it couldn't. A file there that
/// can't be read is kept beside it first, as what it held is lost
/// otherwise: the settings shown are the defaults where it was unreadable
/// as the GUI started.
fn save(config: &mut Config, path: &Path) -> Result<String, String> {
    let shown = path.display().to_string();
    match write(config, path) {
        Ok(None) => Ok(fl!("saved-to", path = shown)),
        Ok(Some(kept)) => {
            Ok(fl!("saved-to", "kept", path = shown, kept = kept.display().to_string()))
        }
        Err(error) => Err(fl!("not-saved", error = error)),
    }
}

/// Writes `config` to `path`, a file there that can't be read kept beside
/// it first, and nothing written where it can't be. Gives where that file
/// went, or why nothing was written.
pub(crate) fn write(config: &mut Config, path: &Path) -> Result<Option<PathBuf>, String> {
    let kept = match Config::load(path) {
        Err(tgradish_core::Error::Config { .. }) => {
            Some(crate::app::keep_aside(path).map_err(|err| err.to_string())?)
        }
        _ => None,
    };
    config.save(path).map_err(|err| err.to_string())?;
    Ok(kept)
}

/// How near the scale's row the pointer keeps the dialog where it is, as
/// its buttons change the scale, and for how long after the last click.
const NEAR: f32 = 24.0;
const HELD: f64 = 1.5;
/// How long the dialog takes back to the window's middle.
const RETURNING: f64 = 0.15;

/// How tall the settings may show, scrolling within the rest of the
/// dialog, so that it fits the window.
fn settings_room(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - 210.0).max(200.0)
}

/// How far within the settings that show the scale's button is kept from
/// their top and bottom, where the last of them fade out: all of it shows.
const KEPT_IN: f32 = FADE + 16.0;

/// Where the dialog goes, and how its settings show.
struct Placed {
    /// Where its corner goes, or the window's middle.
    corner: Option<egui::Pos2>,
    /// How far its settings are scrolled, where that changes.
    scroll: Option<f32>,
    /// How tall its settings show at the most.
    room: f32,
}

/// Where the dialog's corner goes: where it keeps the scale's button
/// clicked under the pointer, or on its way back to the window's middle
/// from there, within the window either way; `None` in the middle. Where
/// it can't move far enough to keep the button there, its settings are
/// scrolled, and shown shorter, so that it ends at the window's edge.
fn placed(ctx: &egui::Context, dialog: &mut SettingsDialog) -> Placed {
    let window = ctx.content_rect();
    let room = settings_room(ctx);
    let scrolled = dialog.scrolled;
    let drawn = dialog.drawn.0.size().max(egui::Vec2::ZERO);
    // how tall it is with `shown` of its settings showing at the most
    let rest = (drawn.y - scrolled.shown).max(0.0);
    let height = |shown: f32| rest + shown.min(scrolled.content);
    // all of it in the window, its corner at the least where it is larger
    let within = |corner: egui::Pos2, shown: f32| {
        let most = (window.max - egui::vec2(drawn.x, height(shown))).max(window.min);
        corner.clamp(window.min, most)
    };
    if let Some((at, offset)) = dialog.pinned {
        let at = at / ctx.pixels_per_point();
        // where its top is with the settings scrolled `scroll`
        let top = |scroll: f32| at.y - offset.y + scroll;
        // not over the window's top: scrolled further instead
        let mut scroll = scrolled.offset.max(window.top() - top(0.0)).max(0.0);
        // nor past its bottom: scrolled back as far as that keeps it under
        // the window's top, then its settings shown shorter
        let fits = window.bottom() - height(room) - top(0.0);
        if scroll > fits {
            scroll = fits.max(window.top() - top(0.0)).max(0.0);
        }
        let mut shown = room.min(scrolled.content);
        let over = top(scroll) + height(shown) - window.bottom();
        if over > 0.0 {
            let least = at.y + KEPT_IN - top(scroll) - scrolled.top;
            shown = (shown - over).max(least);
        }
        // as far as the settings scroll, the button in what shows of them
        let most = (scrolled.content - shown).min(offset.y - scrolled.top - KEPT_IN);
        let scroll = scroll.min(most).max(0.0);
        let corner = within(egui::pos2(at.x - offset.x, top(scroll)), shown);
        return Placed { corner: Some(corner), scroll: Some(scroll), room: shown };
    }
    let unplaced = Placed { corner: None, scroll: None, room };
    let Some((from, since)) = dialog.returning else { return unplaced };
    let share = (ctx.input(|input| input.time) - since) / RETURNING;
    if share >= 1.0 {
        dialog.returning = None;
        return unplaced;
    }
    ctx.request_repaint();
    let middle = window.center() - egui::vec2(drawn.x, height(room)) / 2.0;
    let corner = within(from.lerp(middle, crate::inset::eased(share)), room);
    Placed { corner: Some(corner), ..unplaced }
}

/// Whether the wheel turned this frame, as to scroll.
fn wheeled(ctx: &egui::Context) -> bool {
    let wheel = |event: &egui::Event| matches!(event, egui::Event::MouseWheel { .. });
    ctx.input(|input| input.raw.events.iter().any(wheel))
}

/// Lets the dialog go back to the window's middle, from where it is drawn.
fn release(dialog: &mut SettingsDialog, now: f64) {
    if dialog.pinned.take().is_some() {
        dialog.returning = Some((dialog.drawn.0.min, now));
    }
}

/// How much of the last of the settings that fit fades out, where there
/// are more below.
const FADE: f32 = 24.0;

/// Shows the dialog if it is open. `ffmpeg_given` says an ffmpeg was given
/// as tgradish was started, which it uses over the settings'.
pub(crate) fn show(
    ctx: &egui::Context,
    dialog: &mut SettingsDialog,
    config: &mut Config,
    ffmpeg_given: bool,
    config_path: Option<&Path>,
    presets: &Presets,
) {
    if !dialog.open {
        return;
    }
    if let Some(found) = dialog.checking.as_ref().and_then(Task::poll) {
        dialog.ffmpeg_status = Some(found.unwrap_or_else(|message| (false, message)));
        dialog.checking = None;
    }
    let mut close = false;
    let now = ctx.input(|input| input.time);
    // its settings scrolled by hand while it is kept: let go, rather than
    // moved to keep the button where it was
    if dialog.pinned.is_some() && wheeled(ctx) {
        release(dialog, now);
    }
    let mut modal = widgets::dialog(ctx, "settings");
    let Placed { corner, scroll, room } = placed(ctx, dialog);
    if let Some(corner) = corner {
        let area = egui::Modal::default_area(egui::Id::new("settings"));
        modal = modal.area(area.anchor(egui::Align2::LEFT_TOP, corner.to_vec2()));
    }
    let modal = modal.show(ctx, |ui| {
        ui.set_width(620.0);
        close = widgets::dialog_title(ui, widgets::title_text(ui, fl!("settings")));
        // the dialog fits the window, its settings scrolling within, and
        // shown shorter where it is kept low in it. That there is more
        // below shows: the bar is always drawn where
        // they don't all fit, rather than as the pointer comes near, and
        // the last of what fits fades out until scrolled to the end
        let solid = egui::style::ScrollStyle::solid();
        let scrolled = ui
            .scope(|ui| {
                ui.spacing_mut().scroll = solid;
                let mut area = egui::ScrollArea::vertical().max_height(room);
                if let Some(scroll) = scroll {
                    area = area.vertical_scroll_offset(scroll);
                }
                area.show(ui, |ui| {
                    widgets::settings_spacing(ui);
                    sections(ui, dialog, config, ffmpeg_given, presets);
                })
            })
            .inner;
        dialog.scrolled = Scrolled {
            offset: scrolled.state.offset.y,
            content: scrolled.content_size.y,
            shown: scrolled.inner_rect.height(),
            // from the window's top until it is drawn
            top: scrolled.inner_rect.top(),
        };
        let hidden =
            scrolled.content_size.y - scrolled.state.offset.y - scrolled.inner_rect.height();
        if hidden > 1.0 {
            let rect = scrolled.inner_rect;
            let bar = solid.bar_width + solid.bar_inner_margin + solid.bar_outer_margin;
            let fade = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - FADE.min(hidden)),
                egui::pos2(rect.right() - bar, rect.bottom()),
            );
            widgets::fade_out(ui.painter(), fade, ui.visuals().window_fill);
        }
        // what writing them did, under the settings, where a path has
        // the dialog's width
        ui.add_space(4.0);
        match &dialog.saved {
            Some(Ok(saved)) => widgets::note(ui, saved),
            Some(Err(error)) => {
                ui.colored_label(theme::bad(ui.visuals()), error);
            }
            None => widgets::note(ui, fl!("settings-note")),
        }
        widgets::dialog_buttons(ui, |ui| {
            close |= ui.add(widgets::dialog_button(fl!("close"))).clicked();
            // at the left, as it isn't the way out
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let reset = ui.add(widgets::dialog_button(fl!("reset-settings")));
                if reset.on_hover_text(fl!("reset-settings", "hint")).clicked() {
                    dialog.reset = true;
                }
            });
        });
    });
    dialog.drawn = Drawn(modal.response.rect);
    dialog.scrolled.top -= modal.response.rect.top();
    match dialog.stepped.take() {
        Some(button) if button.is_positive() => {
            // the button kept there already clicked again: kept there as
            // it was, so that what egui rounds to pixels doesn't add up
            let ppp = ctx.pixels_per_point();
            let again = dialog.pinned.is_some_and(|(at, _)| button.contains(at / ppp));
            if !again {
                let offset = button.center() - modal.response.rect.min
                    + egui::vec2(0.0, dialog.scrolled.offset);
                dialog.pinned = Some((button.center() * ppp, offset));
            }
            dialog.stepped_at = now;
            dialog.returning = None;
        }
        // back to 100%, from a button that stays where it is
        Some(_) => release(dialog, now),
        None => {}
    }
    // held while the pointer is on the scale's row, for the next click,
    // and back to the middle once it leaves, or a while after a click
    if dialog.pinned.is_some() {
        let pointer = ctx.input(|input| input.pointer.hover_pos());
        // without a pointer, as clicked by the keyboard, for a while
        let near = match (dialog.scale_row, pointer) {
            (Some(row), Some(at)) => row.expand(NEAR).contains(at),
            (_, None) => true,
            (None, Some(_)) => false,
        };
        let waited = now - dialog.stepped_at;
        if !near || waited >= HELD {
            release(dialog, now);
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(HELD - waited));
        }
    }
    let closing = close || modal.should_close();
    keep(ctx, dialog, config, config_path, closing);
    if closing {
        dialog.open = false;
    }
}

fn sections(
    ui: &mut egui::Ui,
    dialog: &mut SettingsDialog,
    config: &mut Config,
    ffmpeg_given: bool,
    presets: &Presets,
) {
    widgets::section(ui, "ffmpeg");
    if ffmpeg_given {
        note(ui, fl!("ffmpeg-given"));
    }
    let builtin = Backend::select(FfmpegChoice::Builtin, None).is_ok();
    // without one built in, Automatic is the system's: nothing to choose,
    // unless the built-in one is chosen, as by another build's settings
    let choosing = builtin || config.ffmpeg.choice == FfmpegChoice::Builtin;
    if choosing {
        ffmpeg_use(ui, config, builtin);
    }
    row(ui, &fl!("ffmpeg-program"), &fl!("ffmpeg-program", "hint"), Ok(()), |ui| {
        let ffmpeg = &mut config.ffmpeg;
        let wrong = ui
            .horizontal(|ui| {
                let placeholder = dialog.placeholders.0.as_str();
                let typed = &mut dialog.ffmpeg_typed;
                let (wrong, _) =
                    path_field(ui, typed, &mut ffmpeg.path, placeholder, ffmpeg_problem);
                if ui.button(fl!("choose")).clicked() {
                    dialog.pick = Some(crate::files::Pick::Ffmpeg);
                }
                wrong
            })
            .inner;
        problem(ui, wrong);
        ui.horizontal(|ui| {
            let check = egui::Button::new(fl!("check"));
            let clicked = ui
                .add_enabled(dialog.checking.is_none(), check)
                .on_hover_text(fl!("check", "hint"))
                .clicked();
            if clicked {
                // the settings being edited, even where another ffmpeg
                // was given for this run: Check sits beside them, and
                // Save keeps them
                let ffmpeg = config.ffmpeg.clone();
                dialog.checking = Some(Task::spawn(ui.ctx(), move |cancel| {
                    Ok(check_ffmpeg(ffmpeg.choice, ffmpeg.path.as_deref(), cancel))
                }));
                dialog.ffmpeg_status = None;
            }
            if dialog.checking.is_some() {
                ui.spinner();
                note(ui, fl!("checking-ffmpeg"));
            } else if let Some((ok, status)) = &dialog.ffmpeg_status {
                let colour = if *ok { theme::good(ui.visuals()) } else { theme::bad(ui.visuals()) };
                ui.add(egui::Label::new(egui::RichText::new(status).color(colour)).truncate());
            }
        });
    });

    widgets::section(ui, &fl!("results"));
    let gui = &mut config.gui;
    let placeholder = &dialog.placeholders.1;
    row(ui, &fl!("save-results"), "", Ok(()), |ui| {
        let mut elsewhere = gui.output_dir.is_some() || dialog.folder_unchosen;
        let choices = [
            (false, fl!("next-to-input"), String::new()),
            (true, fl!("in-a-folder"), String::new()),
        ];
        if segments(ui, &mut elsewhere, &choices, |_| Ok(())) {
            gui.output_dir = None;
            dialog.folder_unchosen = elsewhere;
        }
        let wrong = ui
            .add_enabled_ui(elsewhere, |ui| {
                ui.horizontal(|ui| {
                    let typed = &mut dialog.folder_typed;
                    let output_dir = &mut gui.output_dir;
                    let (wrong, changed) =
                        path_field(ui, typed, output_dir, placeholder, folder_problem);
                    if changed {
                        dialog.folder_unchosen = output_dir.is_none();
                    }
                    if ui.button(fl!("choose")).clicked() {
                        dialog.pick = Some(crate::files::Pick::OutputDir);
                    }
                    wrong
                })
                .inner
            })
            .inner;
        if elsewhere {
            problem(ui, wrong);
        }
        if elsewhere && gui.output_dir.is_none() {
            widgets::hint(ui, fl!("in-a-folder", "unchosen"));
        }
    });
    row(ui, &fl!("existing-files"), &fl!("existing-files", "hint"), Ok(()), |ui| {
        ui.checkbox(&mut gui.overwrite, fl!("replace-in-the-way"));
    });

    widgets::section(ui, &fl!("interface"));
    // only once there is more than English to pick
    let languages = l10n::languages();
    if languages.len() > 1 {
        row(ui, &fl!("language"), &fl!("language", "hint"), Ok(()), |ui| {
            // the system's, then each by its own name
            let tags: Vec<Option<String>> = std::iter::once(None)
                .chain(languages.iter().map(|(language, _)| Some(language.to_string())))
                .collect();
            let names = std::iter::once(fl!("as-the-system"))
                .chain(languages.iter().map(|(_, name)| name.clone()));
            let choices: Vec<widgets::Choice<usize>> =
                names.enumerate().map(|(index, name)| (index, name, String::new())).collect();
            let mut index = tags.iter().position(|tag| *tag == gui.language).unwrap_or(0);
            if segments(ui, &mut index, &choices, |_| Ok(())) {
                gui.language = tags[index].clone();
                l10n::choose(gui.language.as_deref());
            }
        });
    }
    row(ui, &fl!("theme"), &fl!("theme", "hint"), Ok(()), |ui| {
        use crate::config::Theme;
        let themes = [
            (Theme::System, fl!("as-the-system"), String::new()),
            (Theme::Light, fl!("theme-light"), String::new()),
            (Theme::Dark, fl!("theme-dark"), String::new()),
        ];
        segments(ui, &mut gui.theme, &themes, |_| Ok(()));
    });
    row(ui, &fl!("accent"), &fl!("accent", "hint"), Ok(()), |ui| {
        accents(ui, &mut gui.accent, &mut dialog.accent_typed);
    });
    let scale_row = ui.scope(|ui| {
        row(ui, &fl!("scale"), &fl!("scale", "hint"), Ok(()), |ui| {
            if let Some(button) = scale(ui, &mut gui.scale) {
                dialog.stepped = Some(button);
            }
        });
    });
    dialog.scale_row = Some(scale_row.response.rect);
    row(ui, &fl!("files"), "", Ok(()), |ui| {
        ui.checkbox(&mut gui.reopen_files, fl!("reopen-files"));
    });
    row(ui, &fl!("scrolling"), "", Ok(()), |ui| {
        ui.checkbox(&mut gui.smooth_scrolling, fl!("smooth-scrolling"));
    });

    widgets::section(ui, &fl!("new-files"));
    row(ui, &fl!("make"), "", Ok(()), |ui| {
        let mut target = gui.target.unwrap_or_default();
        if segments(ui, &mut target, &file_settings::targets(), |_| Ok(())) {
            gui.target = Some(target);
        }
    });
    for (format, text) in
        [(Format::Webm, fl!("preset-for-webm")), (Format::Tgs, fl!("preset-for-tgs"))]
    {
        let names = file_settings::preset_names(presets);
        // what built-in presets do differs by format
        let choices: Vec<widgets::Choice<&str>> = names
            .iter()
            .map(|(name, description)| {
                let builtin = presets.get(name).is_ok_and(|preset| preset.path.is_none());
                let hint = file_settings::builtin_description(name, format).filter(|_| builtin);
                let description = description.as_deref().unwrap_or("").to_owned();
                (name.as_str(), file_settings::preset_label(name), hint.unwrap_or(description))
            })
            .collect();
        let current = config.preset_for(format).to_owned();
        let mut chosen = current.as_str();
        let broken = |name: &str| names.iter().any(|(n, d)| n == name && d.is_err());
        row(ui, &text, "", Ok(()), |ui| {
            segments(ui, &mut chosen, &choices, |name| {
                if broken(name) { Err(fl!("preset", "broken")) } else { Ok(()) }
            });
            widgets::chosen_hint(ui, chosen, &choices);
        });
        let picked = chosen.to_owned();
        if picked != current {
            dialog.default_preset = Some((format, picked));
        }
    }
}

/// Which ffmpeg to use: the one built in, where `builtin` says there is
/// one, or the system's, or either.
fn ffmpeg_use(ui: &mut egui::Ui, config: &mut Config, builtin: bool) {
    row(ui, &fl!("ffmpeg-use"), &fl!("ffmpeg-use", "hint"), Ok(()), |ui| {
        let choices = [
            (FfmpegChoice::Auto, fl!("ffmpeg-auto"), fl!("ffmpeg-auto", "hint")),
            (FfmpegChoice::Builtin, fl!("ffmpeg-builtin"), fl!("ffmpeg-builtin", "hint")),
            (FfmpegChoice::System, fl!("ffmpeg-system"), fl!("ffmpeg-system", "hint")),
        ];
        segments(ui, &mut config.ffmpeg.choice, &choices, |choice| match choice {
            FfmpegChoice::Builtin if !builtin => Err(fl!("ffmpeg-builtin", "missing")),
            _ => Ok(()),
        });
    });
}

/// The scale, `scale`, smaller and larger by the steps egui's keys
/// take, and back to 100%, which `App` applies at once, with those keys
/// under it, as the other hints write keys. Gives the button that changed it,
/// where one did: nowhere for the one back to 100%.
fn scale(ui: &mut egui::Ui, scale: &mut f64) -> Option<egui::Rect> {
    use crate::keys::{Command, key_text, shortcut_of};
    let mut stepped = None;
    // the setting changes, which `App` then applies, as it does a
    // theme: kept once saved, like the rest of Settings; egui would only
    // apply a scale asked of it on the next frame, which `App` would
    // take for its keys', which it keeps at once
    let mut changed = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let side = ui.spacing().interact_size.y;
        let button = |text| egui::Button::new(text).min_size(egui::vec2(side, side));
        let least = *scale <= *config::SCALES.start() + 1e-6;
        let smaller = widgets::named(ui.add_enabled(!least, button("−")), &fl!("scale-smaller"))
            .on_hover_text(fl!("scale-smaller"));
        if smaller.clicked() {
            changed = Some(config::scale_step(*scale, -1));
            stepped = Some(smaller.rect);
        }
        // as wide as the widest, so + stays put
        let percent = fl!("scale-percent", percent = (*scale * 100.0).round());
        let widest = fl!("scale-percent", percent = 500);
        let width = widgets::text_width(ui, &widgets::zeros(&widest), 4.0);
        ui.add_sized(egui::vec2(width, side), egui::Label::new(percent));
        let most = *scale >= *config::SCALES.end() - 1e-6;
        let larger = widgets::named(ui.add_enabled(!most, button("+")), &fl!("scale-larger"))
            .on_hover_text(fl!("scale-larger"));
        if larger.clicked() {
            changed = Some(config::scale_step(*scale, 1));
            stepped = Some(larger.rect);
        }
        if widgets::reset_to(ui, (*scale - 1.0).abs() > 1e-6, &fl!("scale-reset")) {
            changed = Some(1.0);
            stepped = Some(egui::Rect::NOTHING);
        }
    });
    if let Some(changed) = changed {
        *scale = changed;
    }
    // as the other hints write keys, Ctrl+V: so Ctrl++, rather
    // than egui's Ctrl+Plus
    // the first of each, as the dialog of shortcuts lists them, though
    // Ctrl+= does it too
    let keys = |command| key_text(&shortcut_of(command).keys[0]);
    widgets::hint(
        ui,
        fl!(
            "scale",
            "keys",
            larger = keys(Command::ScaleUp),
            smaller = keys(Command::ScaleDown),
            reset = keys(Command::ScaleReset)
        ),
    );
    stepped
}

/// The accent colours to choose from: the system's, Plasma's, and one's
/// own, which a colour picker under it sets, or `typed` as `#rrggbb`.
fn accents(ui: &mut egui::Ui, accent: &mut Accent, typed: &mut String) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let system = *accent == Accent::System;
        let button = widgets::choice(ui, system, &fl!("accent-system"), true);
        if button.on_hover_text(fl!("accent-system", "hint")).clicked() {
            *accent = Accent::System;
        }
        ui.add_space(6.0);
        for (name, colour) in theme::ACCENTS {
            let chosen = *accent == Accent::Named(name);
            if widgets::swatch(ui, Some(colour), chosen, &accent_name(name)).clicked() {
                *accent = Accent::Named(name);
            }
        }
        let own = match *accent {
            Accent::Own(colour) => Some(colour),
            _ => None,
        };
        let name = match own {
            Some(colour) => fl!("accent-own", "chosen", colour = config::hex(colour)),
            None => fl!("accent-own"),
        };
        let swatch = widgets::swatch(ui, own, own.is_some(), &name);
        let picker = egui::Popup::from_toggle_button_response(&swatch)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        picker.show(|ui| {
            // from the accent shown, until one is picked
            let mut colour = own.unwrap_or_else(|| theme::accent(ui.visuals()));
            if widgets::colour_picker(ui, &mut colour) {
                *accent = Accent::Own(colour);
            }
            ui.horizontal(|ui| {
                ui.label(fl!("accent-hex"));
                let field = ui.add(widgets::text_field(typed).desired_width(90.0));
                if field.changed() {
                    if let Some(colour) = Accent::hex(typed.trim()) {
                        *accent = Accent::Own(colour);
                    }
                } else if !field.has_focus() {
                    *typed = config::hex(colour);
                }
            });
            if Accent::hex(typed.trim()).is_none() {
                problem(ui, Some(fl!("accent-hex", "wrong")));
            }
        });
    });
}

/// The name of one of [`theme::ACCENTS`], in the GUI's language.
fn accent_name(name: &str) -> String {
    match name {
        "pink" => fl!("accent-pink"),
        "red" => fl!("accent-red"),
        "orange" => fl!("accent-orange"),
        "yellow" => fl!("accent-yellow"),
        "green" => fl!("accent-green"),
        "teal" => fl!("accent-teal"),
        "blue" => fl!("accent-blue"),
        "purple" => fl!("accent-purple"),
        "violet" => fl!("accent-violet"),
        "grey" => fl!("accent-grey"),
        _ => name.to_owned(),
    }
}

impl App {
    /// Makes `name` the default preset for `format`. The items without a
    /// preset of their own follow it, and may then fit another way, as
    /// when they pick another preset: their own range to search goes
    /// where it counts in another unit.
    pub(crate) fn set_default_preset(&mut self, format: Format, name: String) {
        let fit = |choices: &_, config: &_| file_settings::fit_of(choices, &self.presets, config);
        let before: Vec<_> =
            self.items.iter().map(|item| fit(&item.choices, &self.config)).collect();
        match format {
            Format::Webm | Format::Webp => self.config.preset = Some(name),
            Format::Tgs => self.config.tgs_preset = Some(name),
        }
        for (item, before) in self.items.iter_mut().zip(before) {
            let after = fit(&item.choices, &self.config);
            file_settings::follow_fit(&mut item.choices.webm, before, after);
        }
    }
}

/// Whether the ffmpeg chosen, or at `path`, can make stickers, and what to
/// say of it. Runs it, so it may take a while.
fn check_ffmpeg(choice: FfmpegChoice, path: Option<&Path>, cancel: &CancelToken) -> (bool, String) {
    let backend = match Backend::select(choice, path) {
        Ok(backend) => backend,
        Err(err) => return (false, crate::jobs::failure(&err)),
    };
    match backend.capabilities(cancel) {
        Ok(caps) => {
            // "ffmpeg version 8.0 Copyright (c) …", or "… (built in)", where
            // builds from ffmpeg's git tags say "n8.0"
            let version = caps.version.trim_start_matches("ffmpeg version ");
            let version = version.split(" Copyright").next().unwrap_or(version).trim();
            let version = match version.strip_prefix('n') {
                Some(number) if number.starts_with(|c: char| c.is_ascii_digit()) => number,
                _ => version,
            };
            match caps.libvpx_vp9 {
                true => (true, fl!("ffmpeg-ready", version = version)),
                false => (false, fl!("ffmpeg-no-vp9", version = version)),
            }
        }
        Err(err) => (false, crate::jobs::failure(&err)),
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Color32;
    use eframe::egui::ecolor::HsvaGamma;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;

    /// The GUI with its settings kept in `path`, with frames a moment
    /// apart, as by hand.
    fn kept_in(path: &Path) -> Harness<'static, App> {
        let at = path.to_path_buf();
        let builder = Harness::builder().with_size([1280.0, 820.0]).with_step_dt(1.0 / 30.0);
        builder.build_eframe(move |_| {
            App::with(Config::default(), Some(at.clone()), Presets::builtin())
        })
    }

    #[test]
    fn keeps_each_change_as_it_is_made() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut harness = kept_in(&path);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        // opening it writes nothing
        assert!(!path.exists());
        harness.get_by_label(&fl!("settings-note"));
        // a click applies, and is kept, at once
        harness.get_by_label(&fl!("replace-in-the-way")).click_accesskit();
        harness.run_steps(2);
        assert!(harness.state().config.gui.overwrite);
        assert!(Config::load(&path).unwrap().gui.overwrite);
        harness.get_by_label_contains("Saved to");
        // and so is a change made as it closes
        harness.state_mut().config.gui.reopen_files = false;
        harness.get_all_by_label(&fl!("close")).last().unwrap().click_accesskit();
        harness.run_steps(2);
        assert!(!harness.state().settings_dialog.is_open());
        assert!(!Config::load(&path).unwrap().gui.reopen_files);
        // there is no Save to forget
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        assert!(harness.query_by_label(&fl!("save")).is_none());
    }

    #[test]
    fn keeps_what_is_typed_once_typing_pauses_and_only_what_makes_sense() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut harness = kept_in(&path);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.get_by_label(&fl!("in-a-folder")).click_accesskit();
        harness.run_steps(2);
        // the fields of the ffmpeg program and of the folder, in turn
        let role = egui::accesskit::Role::TextInput;
        let field = || {
            egui_kittest::kittest::by().predicate(move |node| {
                node.role() == role
                    && node.placeholder().is_some_and(|text| text.contains("Stickers"))
            })
        };
        let on_disk = || Config::load(&path).ok().and_then(|config| config.gui.output_dir);
        // half a path: said to be wrong, and neither used nor kept
        harness.get(field()).focus();
        harness.run_steps(1);
        harness.get(field()).type_text("results");
        harness.run_steps(2);
        harness.get_by_label(&fl!("in-a-folder", "relative"));
        assert_eq!((harness.state().config.gui.output_dir.clone(), on_disk()), (None, None));
        // made whole: used at once, and kept once typing pauses
        let whole = dir.path().join("results");
        harness.key_press(egui::Key::Home);
        harness.run_steps(1);
        let parent = format!("{}{}", dir.path().display(), std::path::MAIN_SEPARATOR);
        harness.get(field()).type_text(&parent);
        harness.run_steps(1);
        assert!(harness.query_by_label(&fl!("in-a-folder", "relative")).is_none());
        assert_eq!(harness.state().config.gui.output_dir, Some(whole.clone()));
        assert_eq!(on_disk(), None);
        harness.run_steps(30);
        assert_eq!(on_disk(), Some(whole.clone()));
        // a file where the folder would be is wrong too; a program's name
        // alone is looked for on PATH
        let file = dir.path().join("a file");
        std::fs::write(&file, "").unwrap();
        assert_eq!(folder_problem(&file), Some(fl!("in-a-folder", "file")));
        assert_eq!(folder_problem(dir.path()), None);
        assert_eq!(ffmpeg_problem(Path::new("ffmpeg")), None);
        // an ffmpeg that isn't there likewise
        let ffmpeg = || {
            egui_kittest::kittest::by().predicate(move |node| {
                node.role() == role
                    && node.placeholder().is_some_and(|text| text.contains("ffmpeg"))
            })
        };
        harness.get(ffmpeg()).focus();
        harness.run_steps(1);
        harness.get(ffmpeg()).type_text(&dir.path().join("no ffmpeg").display().to_string());
        harness.run_steps(30);
        harness.get_by_label(&fl!("ffmpeg-program", "missing"));
        assert_eq!(harness.state().config.ffmpeg.path, None);
    }

    #[test]
    fn resets_to_the_defaults_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut harness = kept_in(&path);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.get_by_label(&fl!("replace-in-the-way")).click_accesskit();
        harness.get_by_label(&fl!("theme-dark")).click_accesskit();
        harness.run_steps(2);
        let kept = || Config::load(&path).unwrap().gui;
        assert!(kept().overwrite && kept().theme == crate::config::Theme::Dark);
        harness.get_by_label(&fl!("reset-settings")).click_accesskit();
        harness.run_steps(3);
        assert!(!harness.state().config.gui.overwrite);
        assert!(!kept().overwrite && kept().theme == crate::config::Theme::System);
        // the message's Undo, over Settings, puts them back, and keeps them
        harness.get_by_label(&fl!("settings-reset"));
        harness.get_by_label(&fl!("undo-reset")).click();
        harness.run_steps(3);
        assert!(harness.state().config.gui.overwrite);
        assert!(kept().overwrite && kept().theme == crate::config::Theme::Dark);
        assert!(harness.query_by_label(&fl!("settings-reset")).is_none());
        // and from the message once Settings has closed
        harness.get_by_label(&fl!("reset-settings")).click_accesskit();
        harness.run_steps(2);
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!harness.state().settings_dialog.is_open() && !kept().overwrite);
        harness.get_by_label(&fl!("undo-reset")).click();
        harness.run_steps(2);
        assert!(kept().overwrite && kept().theme == crate::config::Theme::Dark);
    }

    #[test]
    fn scales_keeping_its_buttons_under_the_pointer() {
        use eframe::egui::{Key, Modifiers, vec2};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let at = path.clone();
        // the window's size in pixels, which egui sees as fewer points as
        // it grows, as eframe tells it
        let pixels = vec2(1280.0, 900.0);
        // clicked a moment apart, as by hand, rather than seconds
        let builder = Harness::builder().with_size(pixels).with_step_dt(1.0 / 60.0);
        let mut harness = builder.build_eframe(move |_| {
            App::with(Config::default(), Some(at.clone()), Presets::builtin())
        });
        let settle = |harness: &mut Harness<App>| {
            for _ in 0..8 {
                let zoom = harness.ctx.zoom_factor();
                harness.set_size(pixels / zoom);
                harness.step();
            }
        };
        harness.state_mut().settings_dialog.open();
        settle(&mut harness);
        harness.get_by_label(&fl!("scale-percent", percent = 100));
        // its keys said as other shortcuts are, Ctrl++ rather than Ctrl+=
        harness.get_by_label_contains("Ctrl++ and Ctrl+- change it too; Ctrl+0");
        // + clicked again and again where it is on the screen
        let pixel = |harness: &Harness<App>, label: &str| {
            harness.get_by_label(label).rect().center() * harness.ctx.zoom_factor()
        };
        let under = pixel(&harness, &fl!("scale-larger"));
        for percent in [110, 120, 130] {
            harness.get_by_label(&fl!("scale-larger")).click_accesskit();
            settle(&mut harness);
            harness.get_by_label(&fl!("scale-percent", percent = percent));
            let now = pixel(&harness, &fl!("scale-larger"));
            assert!((now - under).length() < 1.5, "at {percent}%: {now:?}, not {under:?}");
        }
        assert_eq!((harness.state().config.gui.scale, harness.ctx.zoom_factor()), (1.3, 1.3));
        // applied and kept at once, as the rest of Settings
        let on_disk = || Config::load(&path).map_or(1.0, |config| config.gui.scale);
        assert_eq!(on_disk(), 1.3);
        // back to 100%, and closed
        harness.get_by_label(&fl!("scale-reset")).click_accesskit();
        settle(&mut harness);
        assert_eq!((harness.state().config.gui.scale, harness.ctx.zoom_factor()), (1.0, 1.0));
        harness.get_all_by_label(&fl!("close")).last().unwrap().click_accesskit();
        settle(&mut harness);
        assert_eq!((harness.ctx.zoom_factor(), on_disk()), (1.0, 1.0));
        // by the keys, with Settings closed: kept at once
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
        settle(&mut harness);
        assert_eq!(harness.ctx.zoom_factor(), 1.1);
        assert_eq!(Config::load(&path).unwrap().gui.scale, 1.1);
        // as by the key the hint names, which + is
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Plus);
        settle(&mut harness);
        assert_eq!(Config::load(&path).unwrap().gui.scale, 1.2);
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
        settle(&mut harness);
        assert_eq!(harness.ctx.zoom_factor(), 1.1);
        // and read as the GUI starts again
        let config = Config::load(&path).unwrap();
        let mut again = Harness::builder()
            .with_size(pixels)
            .build_eframe(move |_| App::with(config.clone(), None, Presets::builtin()));
        again.run_steps(2);
        assert_eq!(again.ctx.zoom_factor(), 1.1);
    }

    #[test]
    fn offers_a_choice_of_ffmpeg_only_where_one_is_built_in() {
        let mut harness = crate::testing::harness(Config::default());
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        let builtin = Backend::select(FfmpegChoice::Builtin, None).is_ok();
        let offered = |harness: &Harness<App>| harness.query_by_label(&fl!("ffmpeg-use")).is_some();
        assert_eq!(offered(&harness), builtin);
        harness.get_by_label(&fl!("ffmpeg-program"));
        // chosen though there is none, as by another build: there to change
        harness.state_mut().config.ffmpeg.choice = FfmpegChoice::Builtin;
        harness.run_steps(2);
        assert!(offered(&harness));
    }

    #[test]
    fn goes_back_to_the_middle_once_the_pointer_leaves_the_scale() {
        use eframe::egui::{Event, PointerButton, Pos2, vec2};
        let pixels = vec2(1280.0, 900.0);
        let mut harness = Harness::builder()
            .with_size(pixels)
            .with_step_dt(1.0 / 60.0)
            .build_eframe(move |_| App::with(Config::default(), None, Presets::builtin()));
        let settle = |harness: &mut Harness<App>, steps: usize| {
            for _ in 0..steps {
                let zoom = harness.ctx.zoom_factor();
                harness.set_size(pixels / zoom);
                harness.step();
            }
        };
        let centred = |harness: &Harness<App>| {
            let drawn = harness.state().settings_dialog.drawn.0;
            (drawn.center() - harness.ctx.content_rect().center()).length() < 1.0
        };
        harness.state_mut().settings_dialog.open();
        // which takes a few frames as its parts find their sizes
        settle(&mut harness, 16);
        assert!(centred(&harness));
        // clicked with the pointer, which stays on the row a moment
        let click = |harness: &mut Harness<App>, at: Pos2| {
            harness.event(Event::PointerMoved(at));
            for pressed in [true, false] {
                let button = PointerButton::Primary;
                let modifiers = egui::Modifiers::NONE;
                harness.event(Event::PointerButton { pos: at, button, pressed, modifiers });
                harness.step();
            }
        };
        for _ in 0..2 {
            let plus = harness.get_by_label(&fl!("scale-larger")).rect().center();
            click(&mut harness, plus);
            settle(&mut harness, 4);
        }
        assert_eq!(harness.state().config.gui.scale, 1.2);
        assert!(harness.state().settings_dialog.pinned.is_some() && !centred(&harness));
        // the pointer away: back in the middle, a moment later
        harness.event(Event::PointerMoved(egui::pos2(5.0, 5.0)));
        settle(&mut harness, 2);
        assert!(harness.state().settings_dialog.pinned.is_none());
        settle(&mut harness, 30);
        assert!(centred(&harness) && harness.state().settings_dialog.returning.is_none());
    }

    /// How far `label`'s button is out of what shows of the settings, in
    /// points: below it, or above it where less than nothing, and nothing
    /// where it shows, to a pixel. Scrolled out of sight, a click there
    /// goes to whatever is drawn there instead.
    fn out_of_sight(screen: &crate::testing::Screen, label: &str) -> f32 {
        let dialog = &screen.harness.state().settings_dialog;
        let top = dialog.drawn.0.top() + dialog.scrolled.top;
        let button = screen.harness.get_by_label(label).rect();
        let below = button.bottom() - (top + dialog.scrolled.shown);
        let above = button.top() - top;
        let pixel = 1.0 / screen.harness.ctx.pixels_per_point();
        if below > pixel {
            below
        } else if above < -pixel {
            above
        } else {
            0.0
        }
    }

    /// Settings open at `scale` on a screen `size` pixels large, the
    /// pointer moved onto `label`'s button and kept there. Where the button
    /// is out of sight at first, as below the settings that show in a
    /// window they hardly fit, they are scrolled with the wheel, as by
    /// hand, just as far as it shows.
    fn on_the_scale(scale: f64, size: [f32; 2], label: &str) -> crate::testing::Screen {
        let mut config = Config::default();
        config.gui.scale = scale;
        let mut screen = crate::testing::Screen::new(config, size, true);
        for _ in 0..10 {
            let out = out_of_sight(&screen, label);
            if out == 0.0 {
                break;
            }
            // the wheel turned over the middle of the settings
            let dialog = &screen.harness.state().settings_dialog;
            let middle = dialog.drawn.0.center_top()
                + egui::vec2(0.0, dialog.scrolled.top + dialog.scrolled.shown / 2.0);
            screen.move_to(middle * screen.harness.ctx.zoom_factor());
            let (unit, phase) = (egui::MouseWheelUnit::Point, egui::TouchPhase::Move);
            let (delta, modifiers) = (egui::vec2(0.0, -out), egui::Modifiers::NONE);
            screen.frame(vec![egui::Event::MouseWheel { unit, delta, phase, modifiers }]);
            screen.frames(16);
        }
        assert_eq!(out_of_sight(&screen, label), 0.0, "{label} at {scale}");
        let pixels = screen.pixels(label);
        screen.move_to(pixels);
        assert!(screen.on(label));
        screen
    }

    #[test]
    fn takes_clicks_again_and_again_with_the_pointer_kept_on_the_scale() {
        // as by hand: on − or +, then clicked again and again a moment
        // apart, without the pointer moving at all, as the button is kept
        // under it, in a window tall enough for the dialog to stay within
        // it as it is kept
        let steps = [
            (fl!("scale-larger"), [1.1, 1.2, 1.3, 1.4, 1.5]),
            (fl!("scale-smaller"), [0.9, 0.8, 0.7, 0.6, 0.5]),
        ];
        for (label, scales) in steps {
            let mut screen = on_the_scale(1.0, [1600.0, 1400.0], &label);
            let under = screen.pixels(&label);
            for scale in scales {
                screen.click();
                screen.frames(4);
                let app = screen.harness.state();
                assert_eq!(app.config.gui.scale, scale, "{label}");
                assert!(app.settings_dialog.pinned.is_some(), "{label} at {scale}");
                // to a pixel or two, as egui rounds what it lays out to them
                let now = screen.pixels(&label);
                assert!((now - under).length() < 2.0, "{label} at {scale}: {now:?}, not {under:?}");
                // the pointer where it is on the screen, and so on it
                let pointer = screen.pointer().map(|at| at * screen.harness.ctx.zoom_factor());
                assert!(pointer.is_some_and(|at| (at - under).length() < 0.01), "{pointer:?}");
                assert!(screen.on(&label), "{label} at {scale}: the pointer at {pointer:?}");
            }
        }
    }

    #[test]
    fn takes_clicks_again_and_again_in_a_window_it_hardly_fits() {
        use crate::testing::Screen;
        // `label`'s button clicked again and again, the pointer kept still,
        // each click setting the scale to the next of `scales`
        let clicks = |screen: &mut Screen, label: &str, scales: &[f64]| {
            let under = screen.pixels(label);
            for &scale in scales {
                screen.click();
                screen.frames(4);
                let app = screen.harness.state();
                assert_eq!(app.config.gui.scale, scale, "{label}");
                assert!(app.settings_dialog.pinned.is_some(), "{label} at {scale}");
                let now = screen.pixels(label);
                assert!((now - under).length() < 2.0, "{label} at {scale}: {now:?}, not {under:?}");
                assert!(screen.on(label), "{label} at {scale}");
            }
        };
        let dialog = |screen: &Screen| {
            let dialog = &screen.harness.state().settings_dialog;
            (dialog.drawn.0, dialog.scrolled)
        };
        // a usual window, which the dialog is nearly as tall as, to where
        // the window, 1280 pixels wide, is too narrow for it
        let (larger, smaller) = (fl!("scale-larger"), fl!("scale-smaller"));
        let mut screen = on_the_scale(1.0, [1280.0, 900.0], &larger);
        clicks(&mut screen, &larger, &[1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7]);
        // which it did by scrolling its settings: it is at the window's top
        let (drawn, scrolled) = dialog(&screen);
        assert!(drawn.top() < 1.0 && scrolled.offset > 50.0, "{drawn:?}, {scrolled:?}");
        // and back by − from there, the pointer moved along the row:
        // scrolled back, then lower
        screen.move_to(screen.pixels(&smaller));
        clicks(&mut screen, &smaller, &[1.6, 1.5, 1.4, 1.3, 1.2, 1.1, 1.0]);
        // by − from the middle, further down the window: lower, then
        // ending at the window's bottom, its settings shown shorter
        let mut screen = on_the_scale(1.7, [1280.0, 900.0], &smaller);
        clicks(&mut screen, &smaller, &[1.6, 1.5, 1.4, 1.3, 1.2, 1.1, 1.0, 0.9, 0.8]);
        let (drawn, scrolled) = dialog(&screen);
        let bottom = screen.harness.ctx.content_rect().bottom();
        assert!((drawn.bottom() - bottom).abs() < 1.0, "{drawn:?} in {bottom}");
        assert!(scrolled.offset < 1.0 && scrolled.shown < scrolled.content - 10.0, "{scrolled:?}");
        // and scrolled by hand, it is let go
        let (unit, phase) = (egui::MouseWheelUnit::Point, egui::TouchPhase::Move);
        let modifiers = egui::Modifiers::NONE;
        let delta = egui::vec2(0.0, -40.0);
        screen.frame(vec![egui::Event::MouseWheel { unit, delta, phase, modifiers }]);
        assert!(screen.harness.state().settings_dialog.pinned.is_none());
    }

    #[test]
    fn stays_where_it_is_at_the_ends_of_the_scale() {
        // to the least, then − can't be clicked, and nothing else is: the
        // pointer on it still
        let smaller = fl!("scale-smaller");
        let mut screen = on_the_scale(0.4, [1280.0, 900.0], &smaller);
        for scale in [0.3, 0.2, 0.2, 0.2] {
            screen.click();
            screen.frames(4);
            assert_eq!(screen.harness.state().config.gui.scale, scale);
            assert!(screen.on(&smaller), "at {scale}");
        }
        // and to the most, where + can't be either, on a screen large
        // enough for the dialog then
        let larger = fl!("scale-larger");
        let mut screen = on_the_scale(4.8, [5200.0, 4400.0], &larger);
        for scale in [4.9, 5.0, 5.0, 5.0] {
            screen.click();
            screen.frames(4);
            assert_eq!(screen.harness.state().config.gui.scale, scale);
            assert!(screen.on(&larger), "at {scale}");
        }
    }

    #[test]
    fn takes_a_click_where_the_pointer_is_once_back_in_the_middle() {
        let larger = fl!("scale-larger");
        let mut screen = on_the_scale(1.0, [1280.0, 900.0], &larger);
        let under = screen.pixels(&larger);
        for _ in 0..3 {
            screen.click();
            screen.frames(4);
        }
        assert_eq!(screen.harness.state().config.gui.scale, 1.3);
        // a while later, the pointer kept still, the dialog is back in the
        // middle, and + is no longer under the pointer
        screen.frames(120);
        let dialog = &screen.harness.state().settings_dialog;
        assert!(dialog.pinned.is_none() && dialog.returning.is_none());
        let middle = screen.harness.ctx.content_rect().center();
        assert!((dialog.drawn.0.center() - middle).length() < 1.0);
        assert!((screen.pixels(&larger) - under).length() > 20.0);
        assert!(!screen.on(&larger));
        // so clicking there again doesn't change the scale
        screen.click();
        screen.frames(4);
        assert_eq!(screen.harness.state().config.gui.scale, 1.3);
        // where + now is, it does
        let now = screen.pixels(&larger);
        screen.move_to(now);
        screen.click();
        screen.frames(4);
        assert_eq!(screen.harness.state().config.gui.scale, 1.4);
        assert!(screen.on(&larger));
    }

    #[test]
    fn keeps_a_file_it_cannot_read_beside_the_one_it_saves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[ffmpeg]\nuse = \"auto\"\n").unwrap();
        let mut config = Config::default();
        config.gui.overwrite = true;
        let said = save(&mut config, &path).unwrap();
        let aside = dir.path().join("config.toml.unreadable");
        assert!(said.contains("config.toml.unreadable"), "{said}");
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "[ffmpeg]\nuse = \"auto\"\n");
        assert!(Config::load(&path).unwrap().gui.overwrite);
        // a file that reads is written over, and nothing more is kept
        std::fs::remove_file(&aside).unwrap();
        assert!(!save(&mut config, &path).unwrap().contains("unreadable"));
        assert!(!aside.exists());
    }

    #[test]
    fn drops_a_range_a_new_default_preset_fits_by_another_unit() {
        use tgradish_core::options::{Fit, Range};
        use tgradish_core::tgs;
        use tgradish_core::webm::Options;

        let dir = tempfile::tempdir().unwrap();
        let webm = Options { fit: Some(Fit::Crf), ..Options::default() };
        let (tgs, presets_dir) = (tgs::Options::default(), dir.path());
        tgradish_core::presets::save(presets_dir, "by quality", "", (&webm, &tgs), false).unwrap();
        let mut harness = crate::testing::harness(Config::default());
        let bitrates = Some(Range { min: 100.0, max: 400.0 });
        {
            let app = harness.state_mut();
            app.presets = Presets::load(Some(presets_dir)).unwrap();
            for (id, preset) in [(1, None), (2, Some("balanced"))] {
                let mut item = crate::item::Item::new(
                    id,
                    vec!["pig.mp4".into()],
                    false,
                    tgradish_core::telegram::Target::Sticker,
                );
                item.choices.preset = preset.map(str::to_owned);
                item.choices.webm.fit_range = bitrates;
                app.items.push(item);
            }
            app.settings_dialog.open();
        }
        harness.run_steps(2);
        // the WebM row's, the first
        harness.get_all_by_label("by quality").next().unwrap().click_accesskit();
        harness.run_steps(2);
        let app = harness.state();
        assert_eq!(app.config.preset.as_deref(), Some("by quality"));
        // the first follows the default, by quality now; the second has a
        // preset of its own, by bitrate still
        let ranges: Vec<_> = app.items.iter().map(|item| item.choices.webm.fit_range).collect();
        assert_eq!(ranges, [None, bitrates]);
    }

    #[test]
    fn takes_up_an_accent_as_it_is_chosen() {
        let mut harness = crate::testing::harness(Config::default());
        harness.state().style(&harness.ctx);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        let shown = |harness: &Harness<App>| theme::accent(&harness.ctx.global_style().visuals);
        assert_eq!(shown(&harness), theme::BREEZE_ACCENT);
        harness.get_by_label(&fl!("accent-teal")).click_accesskit();
        harness.run_steps(2);
        assert_eq!(harness.state().config.gui.accent, Accent::Named("teal"));
        assert_eq!(shown(&harness), Color32::from_rgb(0, 211, 184));
        // one's own, typed in the picker
        harness.get_by_label(&fl!("accent-own")).click_accesskit();
        harness.run_steps(2);
        harness.get_by_label(&fl!("accent-hex"));
        let field = || {
            let role = egui::accesskit::Role::TextInput;
            egui_kittest::kittest::by().value("#00d3b8").role(role)
        };
        harness.get(field()).focus();
        harness.run_steps(1);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.get(field()).type_text("#FF8800");
        harness.run_steps(2);
        let own = Color32::from_rgb(255, 136, 0);
        assert_eq!(harness.state().config.gui.accent, Accent::Own(own));
        assert_eq!(shown(&harness), own);
        harness.get_by_label(&fl!("accent-own", "chosen", colour = "#ff8800"));
        // the square and the strip, with the keyboard: Shift leaps
        let (shade, hue_name) = (fl!("accent-shade"), fl!("accent-hue"));
        let square = || egui_kittest::kittest::by().label(&shade);
        harness.get(square()).focus();
        harness.run_steps(1);
        harness.run_steps(1);
        harness.key_press(egui::Key::ArrowLeft);
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        let less = shown(&harness);
        let (before, after) = (HsvaGamma::from(own), HsvaGamma::from(less));
        assert!(after.s < before.s && after.v < before.v, "{before:?} to {after:?}");
        assert_eq!(harness.state().config.gui.accent, Accent::Own(less));
        assert!(harness.get(square()).value().is_some_and(|value| value.contains("% bright")));
        harness.get(egui_kittest::kittest::by().label(&hue_name)).focus();
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowRight);
        harness.run_steps(2);
        let hue = |colour| HsvaGamma::from(colour).h;
        assert!((hue(shown(&harness)) - hue(less) - 0.1).abs() < 0.02, "{:?}", shown(&harness));
        // the focus kept, and a screen reader's step taken too
        let strip = harness.get(egui_kittest::kittest::by().label(&hue_name));
        assert!(strip.is_focused());
        let (target_node, target_tree) = strip.accesskit_node().locate();
        let stepped = shown(&harness);
        let action = egui::accesskit::Action::Decrement;
        let request =
            egui::accesskit::ActionRequest { action, target_node, target_tree, data: None };
        harness.event(egui::Event::AccessKitActionRequest(request));
        harness.run_steps(2);
        assert!(hue(shown(&harness)) < hue(stepped));
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        // and back to the system's
        // the theme's is the first
        let system = fl!("accent-system");
        harness.get_all_by_label(&system).last().unwrap().click_accesskit();
        harness.run_steps(2);
        assert_eq!(harness.state().config.gui.accent, Accent::System);
        assert_eq!(shown(&harness), theme::BREEZE_ACCENT);
    }

    #[test]
    fn keeps_no_folder_without_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let at = path.clone();
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).build_eframe(move |_| {
            App::with(Config::default(), Some(at.clone()), Presets::builtin())
        });
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.get_by_label(&fl!("in-a-folder")).click_accesskit();
        harness.run_steps(2);
        // in a folder, said to be unchosen, yet none is kept
        harness.get_by_label(&fl!("in-a-folder", "unchosen"));
        assert_eq!(harness.state().config.gui.output_dir, None);
        harness.get_by_label(&fl!("replace-in-the-way")).click_accesskit();
        harness.run_steps(2);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("output-dir"));
    }

    #[test]
    fn checks_ffmpeg_while_frames_are_drawn() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.ffmpeg.path = Some(dir.path().join("no-ffmpeg-here"));
        let mut harness = crate::testing::harness(config);
        // the command line's choice is used, and said to be, but the
        // setting is what is checked
        harness.state_mut().ffmpeg_override = (Some(FfmpegChoice::System), None);
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.get_by_label_contains("ffmpeg given as tgradish was started");
        harness.get_by_label("Check").click_accesskit();
        harness.run_steps(2);
        // what it finds is taken in as frames are drawn
        for _ in 0..500 {
            if harness.state().settings_dialog.ffmpeg_status.is_some() {
                break;
            }
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // in tgradish's own words, as reading a file without it would say
        let status = harness.state().settings_dialog.ffmpeg_status.clone();
        assert!(
            matches!(&status, Some((false, said)) if said.starts_with("ffmpeg wasn't found")),
            "{status:?}"
        );
    }
}
