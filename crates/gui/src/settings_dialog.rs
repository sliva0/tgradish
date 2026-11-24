//! Application Settings dialog: `ffmpeg` selection, worker thread limit, output
//! directory, interface appearance, and defaults for new files.
//!
//! Changes apply immediately and are saved to `config.toml` automatically
//! (debounced while typing or dragging, and written only when typed values are
//! valid).

use std::path::{Path, PathBuf};

use crate::config::{self, Accent, Config};
use eframe::egui;
use tgradish_core::CancelToken;
use tgradish_core::backend::Backend;
use tgradish_core::ffmpeg::FfmpegChoice;
use tgradish_core::presets::{Format, Presets};

use crate::app::App;
use crate::file_settings;
use crate::l10n::{self, Text, fl, later};
use crate::media::Task;
use crate::theme;
use crate::widgets::{self, note, row, segments};

/// Last rendered bounding rectangle (`Rect::NOTHING` until drawn).
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
    /// Result of the last `ffmpeg` check.
    ffmpeg_status: Option<Checked>,
    /// Background task running `ffmpeg` to check its version and encoders
    /// (cancelled if the dialog is reopened or the GUI exits).
    checking: Option<Task<Checked>>,
    /// Result of the last `config.toml` write (`Ok(message)` or `Err(error)`).
    saved: Option<Result<Text, Text>>,
    /// Snapshot of `Config` as last written to disk (or when the dialog opened).
    written: Option<Config>,
    /// Snapshot of `Config` on the previous frame and the timestamp when it
    /// last changed, used to debounce saves while typing or dragging.
    seen: Option<(Config, f64)>,
    /// Current text in the `ffmpeg` path and output directory text fields.
    ffmpeg_typed: Typed,
    folder_typed: Typed,
    /// Set when "Reset to defaults" is clicked; handled by `App` so the reset
    /// can be undone from a banner action.
    pub(crate) reset: bool,
    /// What the empty path fields show, resolved on open: the `ffmpeg`
    /// found on `PATH` and an example output directory.
    placeholders: (Placeholder, Placeholder),
    /// Pending native file-picker request for `App` to open.
    pub(crate) pick: Option<crate::files::Pick>,
    /// True when "In a folder" is selected but no directory path has been
    /// entered yet (`Config` never stores an empty `output_dir`).
    folder_unchosen: bool,
    /// Newly chosen default preset for a format; handled by `App` so items
    /// inheriting the default can update their fitting ranges.
    pub(crate) default_preset: Option<(Format, String)>,
    /// Set when `config.toml` was saved, allowing `App` to clear any stale
    /// config error banner.
    pub(crate) wrote: bool,
    /// Current `#rrggbb` string in the custom accent hex input.
    accent_typed: String,
    /// Bounding box of the scale stepper button clicked this frame (or
    /// `Rect::NOTHING` when the 100% reset button was clicked).
    stepped: Option<egui::Rect>,
    /// Screen-pixel centre of the last clicked scale button and its point
    /// offset from the dialog's top-left corner when unscrolled. While set,
    /// the dialog is anchored (and scrolled if necessary) so the button stays
    /// stationary under the cursor as UI scale changes.
    pinned: Option<(egui::Pos2, egui::Vec2)>,
    /// Timestamp of the last scale button click.
    stepped_at: f64,
    /// Last rendered bounding rectangle of the scale setting row; moving the
    /// pointer away from this row releases `pinned`.
    scale_row: Option<egui::Rect>,
    /// Starting top-left position and timestamp when easing the dialog back to
    /// the window centre after releasing `pinned`.
    returning: Option<(egui::Pos2, f64)>,
    /// Bounding rectangle of the dialog on the previous frame.
    drawn: Drawn,
    /// Scroll state of the settings area on the previous frame.
    scrolled: Scrolled,
}

/// Scroll metrics of the dialog's settings area in points: scroll offset,
/// total content height, visible viewport height, and vertical offset from the
/// dialog top.
#[derive(Clone, Copy, Debug, Default)]
struct Scrolled {
    offset: f32,
    content: f32,
    shown: f32,
    top: f32,
}

impl SettingsDialog {
    pub(crate) fn open(&mut self) {
        // Reset `pinned` so the dialog opens centred in the window.
        *self = SettingsDialog {
            open: true,
            placeholders: placeholders(),
            ..SettingsDialog::default()
        };
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// Shows the `ffmpeg` found on `PATH` as at `ffmpeg`, and `folder` as
    /// the example folder for results, whatever this computer has, as a
    /// long path is on another.
    #[cfg(test)]
    pub(crate) fn as_if_found(&mut self, ffmpeg: &Path, folder: &Path) {
        self.placeholders = (Placeholder::found(ffmpeg), Placeholder::example(folder));
    }

    /// Shows what checking the `ffmpeg` at `ffmpeg` finds where there is
    /// none, and the settings as saved to `saved`.
    #[cfg(test)]
    pub(crate) fn as_if_missing(&mut self, ffmpeg: &Path, saved: &Path) {
        let missing = tgradish_core::FfmpegMissing::NoProgram(ffmpeg.to_path_buf());
        let err = tgradish_core::Error::FfmpegNotFound(missing);
        self.ffmpeg_status = Some(checked(&err));
        self.saved = Some(Ok(later!("saved-to", path = saved.display().to_string())));
    }

    /// Clears uncommitted path edits and `folder_unchosen` when resetting or
    /// restoring settings.
    pub(crate) fn forget_typed(&mut self) {
        (self.ffmpeg_typed, self.folder_typed) = Default::default();
        self.folder_unchosen = false;
    }
}

/// What an empty path field shows: `say` with `path` in it, which
/// [`path_field`] cuts short in its middle where the field is too narrow
/// for it whole.
struct Placeholder {
    path: String,
    say: fn(&str) -> String,
}

impl Default for Placeholder {
    /// Nothing shown.
    fn default() -> Placeholder {
        Placeholder { path: String::new(), say: |_| String::new() }
    }
}

impl Placeholder {
    /// The `ffmpeg` at `path`, found on `PATH`.
    fn found(path: &Path) -> Placeholder {
        let say = |path: &str| fl!("ffmpeg-found", path = path);
        Placeholder { path: path.display().to_string(), say }
    }

    /// `path` as an example.
    fn example(path: &Path) -> Placeholder {
        let say = |path: &str| fl!("for-example", example = path);
        Placeholder { path: path.display().to_string(), say }
    }
}

/// What the empty fields of the ffmpeg program and of the folder for
/// results show: the `ffmpeg` found on `PATH`, and an example folder.
fn placeholders() -> (Placeholder, Placeholder) {
    let ffmpeg = match tgradish_core::ffmpeg::Ffmpeg::on_path() {
        Ok(found) => Placeholder::found(&found.ffmpeg),
        Err(_) if cfg!(windows) => Placeholder::example(Path::new(r"C:\ffmpeg\bin")),
        Err(_) => Placeholder::example(Path::new("/usr/local/bin/ffmpeg")),
    };
    let folder = tgradish_core::paths::pictures_dir()
        .map(|dir| Placeholder::example(&dir.join("Stickers")))
        .unwrap_or_default();
    (ffmpeg, folder)
}

/// Buffers text typed into a path field alongside the last `setting` value it
/// was synced from (preserving invalid intermediate text while syncing external
/// changes such as "Choose…").
#[derive(Default)]
struct Typed {
    text: String,
    of: Option<Option<PathBuf>>,
}

/// Draws a path text field bound to `setting`, leaving room beside it for
/// [`choose`]. Updates `setting` whenever `problem` returns `None` (an empty
/// field clears `setting` to `None`), and returns `(validation_error,
/// setting_changed)`.
fn path_field(
    ui: &mut egui::Ui,
    typed: &mut Typed,
    setting: &mut Option<PathBuf>,
    placeholder: &Placeholder,
    problem: impl Fn(&Path) -> Option<String>,
) -> (Option<String>, bool) {
    if typed.of.as_ref() != Some(setting) {
        typed.text = setting.as_ref().map(|path| path.display().to_string()).unwrap_or_default();
        typed.of = Some(setting.clone());
    }
    let padding = 2.0 * ui.spacing().button_padding.x;
    let beside = widgets::text_width(ui, &fl!("choose"), padding) + ui.spacing().item_spacing.x;
    let width = (ui.available_width() - beside).min(280.0);
    // the placeholder as wide as the field within its margins, else its
    // path cut short in the middle, whole on hover, rather than cut short
    // at its end, losing the file's name, or pushing the field wider
    let room = width.min(ui.available_width()) - FIELD_MARGINS;
    let font = egui::FontSelection::Default.resolve(ui.style());
    let fits = |text: &str| {
        let galley =
            ui.painter().layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE);
        galley.size().x <= room
    };
    let whole = (placeholder.say)(&placeholder.path);
    let shown = widgets::path_cut(&placeholder.path, placeholder.say, fits);
    let edit = widgets::text_field(&mut typed.text).hint_text(shown.as_str()).desired_width(width);
    let mut field = ui.add(edit);
    if typed.text.is_empty() && shown != whole {
        field = field.on_hover_text(whole);
    }
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

/// How much narrower than a [`path_field`] its text is: egui's margins of
/// a text field, 4 points each side, and a point to spare for rounding.
const FIELD_MARGINS: f32 = 9.0;

/// Draws the button beside a [`path_field`] that picks the path from the
/// system's file dialog, returning `true` when clicked.
fn choose(ui: &mut egui::Ui) -> bool {
    ui.button(fl!("choose")).clicked()
}

/// Validates an `ffmpeg` path: must exist on disk unless it is a bare command
/// name (which is resolved via `PATH`).
fn ffmpeg_problem(path: &Path) -> Option<String> {
    let named = path.components().count() == 1 && !path.has_root();
    (!named && !path.exists()).then(|| fl!("ffmpeg-program", "missing"))
}

/// Validates an output directory path: must be absolute and not an existing file.
fn folder_problem(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        Some(fl!("in-a-folder", "relative"))
    } else if path.is_file() {
        Some(fl!("in-a-folder", "file"))
    } else {
        None
    }
}

/// What checking `ffmpeg` found: whether it is ready to convert, what is
/// said of it, and the path that names, if it names one.
#[derive(Clone, Debug, Default)]
struct Checked {
    ready: bool,
    said: Text,
    path: Option<String>,
}

/// What checking `ffmpeg` found where it failed with `err`.
fn checked(err: &tgradish_core::Error) -> Checked {
    use tgradish_core::FfmpegMissing::{NoFfprobe, NoProgram};
    let path = match err {
        tgradish_core::Error::FfmpegNotFound(NoProgram(path) | NoFfprobe(path)) => {
            Some(path.display().to_string())
        }
        _ => None,
    };
    Checked { ready: false, said: crate::jobs::failure(err), path }
}

/// Draws what checking `ffmpeg` found in `colour`, wrapping up to two
/// lines: the path it names cut short in its middle where it is too long
/// for them, else the end cut short, and all of it on hover where either
/// is.
fn ffmpeg_status(ui: &mut egui::Ui, checked: &Checked, colour: egui::Color32) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = ui.available_width();
    let lay_out = |text: &str| {
        let format = egui::TextFormat::simple(font.clone(), colour);
        let mut job = egui::text::LayoutJob::single_section(text.to_owned(), format);
        job.wrap = egui::text::TextWrapping {
            max_rows: 2,
            ..egui::text::TextWrapping::wrap_at_width(width)
        };
        ui.painter().layout_job(job)
    };
    let said = checked.said.say();
    let said = said.as_str();
    let shown = match &checked.path {
        Some(path) if said.contains(path.as_str()) => {
            let say = |cut: &str| said.replacen(path.as_str(), cut, 1);
            widgets::path_cut(path, say, |text| !lay_out(text).elided)
        }
        _ => said.to_owned(),
    };
    let galley = lay_out(&shown);
    let (rect, response) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, said));
    let cut = galley.elided || shown != said;
    ui.painter().galley(rect.min, galley, colour);
    if cut {
        response.on_hover_text(said);
    }
}

/// Draws a field validation error below the control when `wrong` is `Some`.
fn problem(ui: &mut egui::Ui, wrong: Option<String>) {
    if let Some(wrong) = wrong {
        let text = egui::RichText::new(wrong).color(theme::bad(ui.visuals())).size(12.5);
        ui.add(egui::Label::new(text).wrap());
    }
}

/// Debounce delay in seconds before writing `config.toml` while a text field
/// has focus or a mouse button is held.
const SETTLE: f64 = 0.6;

/// Persists `config` to `config_path` when modified (debounced by [`SETTLE`]
/// while typing or dragging, or written immediately when `closing`).
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
        None => Err(later!("no-home")),
    });
    dialog.wrote = matches!(dialog.saved, Some(Ok(_)));
    // Record the attempt so a failing save is not retried every frame until
    // `config` changes again.
    dialog.written = Some(config.clone());
}

/// Saves `config` to `path` and returns a localised status or error string. If
/// `path` already exists but cannot be parsed, backs it up beside `path` first
/// so an unreadable config file from startup is not overwritten.
fn save(config: &mut Config, path: &Path) -> Result<Text, Text> {
    let shown = path.display().to_string();
    match write(config, path) {
        Ok(None) => Ok(later!("saved-to", path = shown)),
        Ok(Some(kept)) => {
            Ok(later!("saved-to", "kept", path = shown, kept = kept.display().to_string()))
        }
        Err(error) => Err(later!("not-saved", error = error.to_string())),
    }
}

/// Writes `config` to `path`, backing up any existing unparseable file at
/// `path` first and returning the backup path if one was created.
pub(crate) fn write(
    config: &mut Config,
    path: &Path,
) -> Result<Option<PathBuf>, tgradish_core::Error> {
    let kept = match Config::load(path) {
        Err(tgradish_core::Error::Config { .. }) => Some(crate::app::keep_aside(path)?),
        _ => None,
    };
    config.save(path)?;
    Ok(kept)
}

/// Pointer distance around the scale row (in points) and timeout (in seconds)
/// for keeping the dialog pinned after clicking a scale button.
const NEAR: f32 = 24.0;
const HELD: f64 = 1.5;
/// Duration in seconds of the ease-back animation to the window centre.
const RETURNING: f64 = 0.15;

/// Maximum height of the scrollable settings area inside the dialog: room
/// to spare around it in a tall window, and as much as the dialog fits in
/// a short one.
fn settings_room(ctx: &egui::Context) -> f32 {
    let spared = (ctx.content_rect().height() - 210.0).max(200.0);
    spared.min(widgets::dialog_room(ctx, egui::Id::new("settings")))
}

/// Minimum margin from the top and bottom edges of the scroll viewport to keep
/// the pinned scale button clear of the scroll fade region.
const KEPT_IN: f32 = widgets::SCROLL_FADE + 16.0;

/// Computed position and scroll state for the Settings dialog.
struct Placed {
    /// Top-left dialog position (`None` to centre in the window).
    corner: Option<egui::Pos2>,
    /// Override scroll offset for the settings area when pinned near a window edge.
    scroll: Option<f32>,
    /// Maximum height of the settings scroll viewport.
    room: f32,
}

/// Computes the dialog placement for this frame: anchored to keep a clicked
/// scale button under the pointer (scrolling or shrinking the viewport if the
/// dialog hits a window edge), easing back to the centre, or `None` when centred.
fn placed(ctx: &egui::Context, dialog: &mut SettingsDialog) -> Placed {
    let window = ctx.content_rect();
    let room = settings_room(ctx);
    let scrolled = dialog.scrolled;
    let drawn = dialog.drawn.0.size().max(egui::Vec2::ZERO);
    // Total dialog height when the scroll viewport is at most `shown` points tall.
    let rest = (drawn.y - scrolled.shown).max(0.0);
    let height = |shown: f32| rest + shown.min(scrolled.content);
    // Clamp the top-left corner so the dialog stays inside the window.
    let within = |corner: egui::Pos2, shown: f32| {
        let most = (window.max - egui::vec2(drawn.x, height(shown))).max(window.min);
        corner.clamp(window.min, most)
    };
    if let Some((at, offset)) = dialog.pinned {
        let at = at / ctx.pixels_per_point();
        // Dialog top Y when the settings area is scrolled by `scroll`.
        let top = |scroll: f32| at.y - offset.y + scroll;
        // If the dialog top would extend above the window, scroll down instead.
        let mut scroll = scrolled.offset.max(window.top() - top(0.0)).max(0.0);
        // If the dialog bottom would extend below the window, scroll back up as
        // far as the top allows, then shrink the scroll viewport height.
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
        // Clamp scroll to valid bounds while keeping the button visible inside `shown`.
        let most = (scrolled.content - shown).min(offset.y - scrolled.top - KEPT_IN);
        let scroll = scroll.min(most).max(0.0);
        let corner = within(egui::pos2(at.x - offset.x, top(scroll)), shown);
        return Placed { corner: Some(corner), scroll: Some(scroll), room: shown };
    }
    let unplaced = Placed { corner: None, scroll: None, room };
    let Some((from, since)) = dialog.returning else { return unplaced };
    let share = crate::motion::along(ctx, since, RETURNING);
    if share >= 1.0 {
        dialog.returning = None;
        return unplaced;
    }
    let middle = window.center() - egui::vec2(drawn.x, height(room)) / 2.0;
    let corner = within(from.lerp(middle, crate::inset::eased(f64::from(share))), room);
    Placed { corner: Some(corner), ..unplaced }
}

/// Returns true if a mouse wheel event occurred this frame.
fn wheeled(ctx: &egui::Context) -> bool {
    let wheel = |event: &egui::Event| matches!(event, egui::Event::MouseWheel { .. });
    ctx.input(|input| input.raw.events.iter().any(wheel))
}

/// Unpins the dialog and starts easing it back to the window centre.
fn release(dialog: &mut SettingsDialog, now: f64) {
    if dialog.pinned.take().is_some() {
        dialog.returning = Some((dialog.drawn.0.min, now));
    }
}

/// Draws the Settings dialog if open. `ffmpeg_given` indicates that an
/// `ffmpeg` override was passed on the command line for this session.
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
        dialog.ffmpeg_status =
            Some(found.unwrap_or_else(|said| Checked { said, ..Checked::default() }));
        dialog.checking = None;
    }
    let mut close = false;
    let now = ctx.input(|input| input.time);
    // Manual wheel scrolling unpins the dialog instead of fighting the scroll offset.
    if dialog.pinned.is_some() && wheeled(ctx) {
        release(dialog, now);
    }
    let mut modal = widgets::dialog(ctx, "settings").width(620.0);
    let Placed { corner, scroll, room } = placed(ctx, dialog);
    if let Some(corner) = corner {
        let area = egui::Modal::default_area(egui::Id::new("settings"));
        modal = modal.area(area.anchor(egui::Align2::LEFT_TOP, corner.to_vec2()));
    }
    let modal = modal.show(ctx, |ui| {
        close = widgets::dialog_title(ui, widgets::title_text(ui, fl!("settings")));
        // Shown shorter while the dialog is pinned low.
        let mut area = egui::ScrollArea::vertical();
        if let Some(scroll) = scroll {
            area = area.vertical_scroll_offset(scroll);
        }
        // In a short window, the save status scrolls at the end of the
        // settings, where it leaves them more room.
        let short = ctx.content_rect().height() < SHORT;
        let scrolled = widgets::dialog_body_within(ui, area, room, |ui| {
            widgets::settings_spacing(ui);
            sections(ui, dialog, config, ffmpeg_given, presets);
            if short {
                footer(ui, &dialog.saved);
            }
        });
        dialog.scrolled = Scrolled {
            offset: scrolled.state.offset.y,
            content: scrolled.content_size.y,
            shown: scrolled.inner_rect.height(),
            // Adjusted to be relative to `modal.response.rect.top()` below.
            top: scrolled.inner_rect.top(),
        };
        if !short {
            footer(ui, &dialog.saved);
        }
        widgets::dialog_buttons(ui, |ui| {
            close |= ui.add(widgets::dialog_button(fl!("close"))).clicked();
            // Put Reset to defaults at the left, apart from the buttons that
            // close.
            let reset = widgets::dialog_button_apart(ui, &fl!("reset-settings"));
            if reset.on_hover_text(fl!("reset-settings", "hint")).clicked() {
                dialog.reset = true;
            }
        });
    });
    dialog.drawn = Drawn(modal.response.rect);
    dialog.scrolled.top -= modal.response.rect.top();
    match dialog.stepped.take() {
        Some(button) if button.is_positive() => {
            // If the same pinned button was clicked again, keep its original
            // pixel anchor so sub-pixel rounding errors do not accumulate.
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
        // Clicking the 100% reset button unpins the dialog (it does not move
        // as the scale changes).
        Some(_) => release(dialog, now),
        None => {}
    }
    // Keep the dialog pinned while the pointer remains near the scale row and
    // within `HELD` seconds of the last click.
    if dialog.pinned.is_some() {
        let pointer = ctx.input(|input| input.pointer.hover_pos());
        // Keep pinned until `HELD` expires when activated via keyboard (no pointer).
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

/// Window height under which [`footer`] scrolls with the settings, as the
/// least window height is from 1.4 times the scale.
const SHORT: f32 = 400.0;

/// Draws the save status across the full width of the dialog, from the
/// last write of `config.toml` (`saved`), or what saving is like.
fn footer(ui: &mut egui::Ui, saved: &Option<Result<Text, Text>>) {
    ui.add_space(4.0);
    match saved {
        Some(Ok(saved)) => widgets::note(ui, saved.say()),
        Some(Err(error)) => {
            ui.colored_label(theme::bad(ui.visuals()), error.say());
        }
        None => widgets::note(ui, fl!("settings-note")),
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
    // Hide the backend selector in builds without built-in ffmpeg (where Auto
    // always uses the system binary), unless `Builtin` is currently selected
    // in `config.toml` from another build.
    let choosing = builtin || config.ffmpeg.choice == FfmpegChoice::Builtin;
    if choosing {
        ffmpeg_use(ui, config, builtin);
    }
    row(ui, &fl!("ffmpeg-program"), &fl!("ffmpeg-program", "hint"), Ok(()), |ui| {
        let ffmpeg = &mut config.ffmpeg;
        let wrong = ui
            .horizontal(|ui| {
                let placeholder = &dialog.placeholders.0;
                let typed = &mut dialog.ffmpeg_typed;
                let (wrong, _) =
                    path_field(ui, typed, &mut ffmpeg.path, placeholder, ffmpeg_problem);
                if choose(ui) {
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
                // Check the configured `ffmpeg` setting even if a CLI override
                // is active for this run.
                let ffmpeg = config.ffmpeg.clone();
                dialog.checking = Some(Task::spawn(ui.ctx(), move |cancel| {
                    Ok(check_ffmpeg(ffmpeg.choice, ffmpeg.path.as_deref(), cancel))
                }));
                dialog.ffmpeg_status = None;
            }
            // Reserve two lines of height so checking `ffmpeg` does not shift
            // the controls below.
            let lines = 2.0 * ui.text_style_height(&egui::TextStyle::Body);
            let room = egui::vec2(ui.available_width(), lines.max(theme::CONTROL));
            let layout = egui::Layout::left_to_right(egui::Align::Center);
            ui.allocate_ui_with_layout(room, layout, |ui| {
                ui.set_min_size(room);
                if dialog.checking.is_some() {
                    ui.spinner();
                    note(ui, fl!("checking-ffmpeg"));
                } else if let Some(checked) = &dialog.ffmpeg_status {
                    let visuals = ui.visuals();
                    let colour = match checked.ready {
                        true => theme::good(visuals),
                        false => theme::bad(visuals),
                    };
                    ffmpeg_status(ui, checked, colour);
                }
            });
        });
    });

    widgets::section(ui, &fl!("converting-settings"));
    row(ui, &fl!("threads"), &fl!("threads", "hint"), Ok(()), |ui| threads(ui, config));

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
                    if choose(ui) {
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
        // Always reserve space for the unchosen-folder hint so toggling modes
        // does not shift controls below.
        if elsewhere && gui.output_dir.is_none() {
            widgets::hint(ui, fl!("in-a-folder", "unchosen"));
        } else {
            widgets::hint_room(ui, fl!("in-a-folder", "unchosen"));
        }
    });
    row(ui, &fl!("existing-files"), &fl!("existing-files", "hint"), Ok(()), |ui| {
        ui.checkbox(&mut gui.overwrite, fl!("replace-in-the-way"));
    });

    widgets::section(ui, &fl!("interface"));
    if language(ui, &mut gui.language, l10n::variable().as_deref()) {
        // said in the language before
        dialog.placeholders = placeholders();
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
    row(ui, &fl!("motion"), &fl!("motion", "hint"), Ok(()), |ui| {
        ui.checkbox(&mut gui.motion, fl!("motion-on")).on_hover_text(fl!("motion", "hint"));
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
        // Use format-specific descriptions for built-in presets.
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

/// The globe that marks the choice of language beside its name, for it to
/// be found in a language one can't read.
const GLOBE: &str = "🌐";

/// Draws the choice of language, if there are translations: the system's
/// first, saying which that is, then each by its own name. A change shows
/// at once, as it is kept; returns whether there was one. While
/// `variable`, what `TGRADISH_LANG` asks for, chooses the language, the
/// choice shows that, and can't be changed.
fn language(ui: &mut egui::Ui, chosen: &mut Option<String>, variable: Option<&str>) -> bool {
    use i18n_embed::unic_langid::LanguageIdentifier;
    let languages = l10n::languages();
    if languages.len() < 2 {
        return false;
    }
    let name = |language: &LanguageIdentifier| {
        l10n::name_of(language).map_or_else(|| language.to_string(), str::to_owned)
    };
    let system = l10n::system();
    let system = fl!("as-the-system-language", language = name(&system));
    let picked: Option<LanguageIdentifier> = chosen.as_deref().and_then(|tag| tag.parse().ok());
    let label = format!("{GLOBE} {}", fl!("language"));
    let mut changed = false;
    row(ui, &label, &fl!("language", "hint"), Ok(()), |ui| {
        let shown = match (variable, &picked) {
            // English made longer, as it is asked for
            (Some(tag @ "pseudo"), _) => tag.to_owned(),
            (Some(_), _) => name(&l10n::current()),
            (None, Some(language)) => name(language),
            (None, None) => system.clone(),
        };
        // Within the window at any scale; it scrolls for the rest.
        let height = (ui.ctx().content_rect().height() * 0.4).clamp(80.0, 320.0);
        let mut asked = None;
        let combo = ui.add_enabled_ui(variable.is_none(), |ui| {
            let combo = egui::ComboBox::from_id_salt("language").selected_text(shown.clone());
            combo.height(height).show_ui(ui, |ui| {
                if ui.selectable_label(picked.is_none(), &system).clicked() {
                    asked = Some(None);
                }
                ui.separator();
                for (language, own) in languages {
                    if ui.selectable_label(picked.as_ref() == Some(language), own).clicked() {
                        asked = Some(Some(language.to_string()));
                    }
                }
            })
        });
        let combo = combo.inner.response;
        let enabled = combo.enabled();
        combo.widget_info(|| {
            let language = fl!("language");
            let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, language);
            info.current_text_value = Some(shown.clone());
            info
        });
        if variable.is_some() {
            widgets::hint(ui, fl!("language", "variable", variable = l10n::VARIABLE));
        }
        if let Some(asked) = asked.filter(|asked| asked != chosen) {
            *chosen = asked;
            l10n::choose(chosen.as_deref());
            changed = true;
        }
    });
    changed
}

/// Draws the ffmpeg backend selector (`Auto`, `Builtin`, `System`).
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

/// Draws the worker thread limit control (`Auto` for hardware concurrency, or
/// a custom limit).
fn threads(ui: &mut egui::Ui, config: &mut Config) {
    use std::num::NonZeroU32;
    use tgradish_core::config::Threads;
    let machine = std::thread::available_parallelism().map_or(1, |threads| threads.get() as u32);
    let mut limit = config.threads.limit().map(NonZeroU32::get);
    let automatic = (fl!("threads-auto"), fl!("threads-auto", "hint"));
    let automatic = (automatic.0.as_str(), automatic.1.as_str());
    let changed =
        widgets::auto_number(ui, &mut limit, automatic, Some(machine), machine, |field| {
            field.range(1..=MOST_THREADS)
        });
    if changed {
        config.threads = limit.and_then(NonZeroU32::new).map_or(Threads::Auto, Threads::Limit);
    }
}

/// Upper bound for the custom thread count field (far more than any machine it
/// runs on makes use of).
const MOST_THREADS: u32 = 256;

/// Draws the UI scale stepper (− and + step by the steps egui's keys take;
/// holding either repeats while staying under the pointer) and 100% reset
/// button. Returns the bounding box of the clicked stepper button (or
/// `Rect::NOTHING` when reset to 100% was clicked).
fn scale(ui: &mut egui::Ui, scale: &mut f64) -> Option<egui::Rect> {
    use crate::keys::{Command, key_text, shortcut_of};
    let mut stepped = None;
    // Change the setting and let `App` apply it, as for the theme, so it is
    // saved with the rest of Settings. Zooming egui directly would only take
    // effect next frame, where `App` would mistake it for a keyboard zoom and
    // save it at once.
    let mut changed = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let side = ui.spacing().interact_size.y;
        let least = *scale <= *config::SCALES.start() + 1e-6;
        let (smaller, step) = widgets::stepper(ui, "−", !least, &fl!("scale-smaller"));
        if step {
            changed = Some(config::scale_step(*scale, -1));
            stepped = Some(smaller.rect);
        }
        // Fix the percentage label width to its widest value ("500%") so `+` never shifts.
        let percent = fl!("scale-percent", percent = (*scale * 100.0).round());
        let widest = fl!("scale-percent", percent = 500);
        let width = widgets::text_width(ui, &widgets::zeros(&widest), 4.0);
        ui.add_sized(egui::vec2(width, side), egui::Label::new(percent));
        let most = *scale >= *config::SCALES.end() - 1e-6;
        let (larger, step) = widgets::stepper(ui, "+", !most, &fl!("scale-larger"));
        if step {
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
    // Format primary shortcuts with `key_text` (e.g. `Ctrl++` rather than `Ctrl+Plus`).
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

/// Draws the accent colour selector: system accent, Plasma palette swatches,
/// and a custom colour swatch with a popup HSV/hex picker.
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
            // Initialise from the currently active accent until a custom colour is picked.
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

/// Returns the localised display name for a built-in [`theme::ACCENTS`] entry.
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
    /// Sets `name` as the default preset for `format` without recording an item
    /// undo step, clearing any per-item `fit_range` whose fitting unit no
    /// longer matches the new default preset.
    pub(crate) fn set_default_preset(&mut self, format: Format, name: String) {
        self.settle_edits();
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
        self.take_in_edits();
    }
}

/// Probes the selected `ffmpeg` backend for version and `libvpx-vp9` support.
/// Runs it, so it may take a while.
fn check_ffmpeg(choice: FfmpegChoice, path: Option<&Path>, cancel: &CancelToken) -> Checked {
    let backend = match Backend::select(choice, path) {
        Ok(backend) => backend,
        Err(err) => return checked(&err),
    };
    match backend.capabilities(cancel) {
        Ok(caps) => {
            // Version strings look like "ffmpeg version 8.0 Copyright (c) …"
            // or "… (built in)"; builds from git tags say "n8.0". Strip the
            // prefix, the copyright and that leading 'n'.
            let version = caps.version.trim_start_matches("ffmpeg version ");
            let version = version.split(" Copyright").next().unwrap_or(version).trim();
            let version = match version.strip_prefix('n') {
                Some(number) if number.starts_with(|c: char| c.is_ascii_digit()) => number,
                _ => version,
            };
            let (ready, said) = match caps.libvpx_vp9 {
                true => (true, later!("ffmpeg-ready", version = version)),
                false => (false, later!("ffmpeg-no-vp9", version = version)),
            };
            Checked { ready, said, path: None }
        }
        Err(err) => checked(&err),
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
    fn leaves_the_first_escape_to_a_field_typed_in() {
        let mut harness = crate::testing::harness(Config::default());
        harness.run_steps(2);
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(3);
        // the threads, of one's own number, typed in
        harness.get_all_by_label(&fl!("custom")).last().unwrap().click_accesskit();
        harness.run_steps(3);
        let threads = |harness: &Harness<crate::app::App>| harness.state().config.threads;
        let set = threads(&harness);
        assert_ne!(set, tgradish_core::config::Threads::Auto);
        let field = harness.query_all_by_role(egui::accesskit::Role::SpinButton).last().unwrap();
        field.focus();
        harness.run_steps(2);
        harness.event(egui::Event::Text("3".into()));
        harness.run_steps(2);
        // put back as it was, Settings still open
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(harness.state().dialogs.settings.is_open());
        assert!(!harness.ctx.text_edit_focused());
        assert_eq!(threads(&harness), set);
        // and closed by the next
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(!harness.state().dialogs.settings.is_open());
        assert_eq!(threads(&harness), set);
    }

    #[test]
    fn closes_on_the_first_escape_from_a_field_left_as_it_was() {
        let mut harness = crate::testing::harness(Config::default());
        crate::widgets::test_fonts(&harness);
        harness.run_steps(2);
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(3);
        harness.get_all_by_label(&fl!("custom")).last().unwrap().click_accesskit();
        harness.run_steps(3);
        let threads = |harness: &Harness<crate::app::App>| harness.state().config.threads;
        let set = threads(&harness);
        let focus = |harness: &mut Harness<crate::app::App>| {
            let field = harness.query_all_by_role(egui::accesskit::Role::SpinButton).last();
            field.unwrap().focus();
            harness.run_steps(2);
        };
        // stepped and back, then typed and back: nothing to put back
        focus(&mut harness);
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(2);
        harness.key_press(egui::Key::ArrowDown);
        harness.run_steps(2);
        harness.event(egui::Event::Text("3".into()));
        harness.run_steps(2);
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        harness.run_steps(2);
        assert!(harness.ctx.text_edit_focused());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(!harness.state().dialogs.settings.is_open());
        assert!(!harness.ctx.text_edit_focused());
        assert_eq!(threads(&harness), set);
        // only stepped: put back, Settings still open
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(3);
        focus(&mut harness);
        harness.key_press(egui::Key::ArrowUp);
        harness.run_steps(2);
        assert_ne!(threads(&harness), set);
        harness.key_press(egui::Key::Escape);
        harness.run_steps(3);
        assert!(harness.state().dialogs.settings.is_open());
        assert_eq!(threads(&harness), set);
    }

    #[test]
    fn keeps_each_change_as_it_is_made() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut harness = kept_in(&path);
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(2);
        // opening it writes nothing
        assert!(!path.exists());
        harness.get_by_label(&fl!("settings-note"));
        // a click applies, and is kept, at once
        harness.get_by_label(&fl!("replace-in-the-way")).click_accesskit();
        harness.run_steps(2);
        assert!(harness.state().config.gui.overwrite);
        assert!(Config::load(&path).unwrap().gui.overwrite);
        harness.get_by_label_contains(&fl!("saved-to", path = path.display().to_string()));
        // and so is a change made as it closes
        harness.state_mut().config.gui.reopen_files = false;
        harness.get_all_by_label(&fl!("close")).last().unwrap().click_accesskit();
        harness.run_steps(2);
        assert!(!harness.state().dialogs.settings.is_open());
        assert!(!Config::load(&path).unwrap().gui.reopen_files);
        // there is no Save to forget
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(2);
        assert!(harness.query_by_label(&fl!("save")).is_none());
    }

    /// The choice of language lists the system's first, saying which that
    /// is, then each language by its own name, within a short window; and
    /// keeps the one picked.
    #[test]
    fn picks_a_language_from_each_by_its_own_name() {
        // `TGRADISH_LANG` chooses instead, as the next test has it
        if crate::l10n::variable().is_some() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let at = path.clone();
        // as short as a larger scale makes the window
        let builder = Harness::builder().with_size([crate::MIN_WIDTH, crate::MIN_HEIGHT / 1.5]);
        let mut harness = builder.build_eframe(move |cc| {
            let app = App::with(Config::default(), Some(at.clone()), Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(3);
        let system = crate::l10n::system();
        let system = crate::l10n::name_of(&system).unwrap();
        let system = fl!("as-the-system-language", language = system);
        let language = fl!("language");
        let combo = harness.get_by_label(&language);
        assert_eq!(combo.accesskit_node().value().as_deref(), Some(system.as_str()));
        combo.scroll_to_me();
        harness.run_steps(3);
        harness.get_by_label(&language).click();
        harness.run_steps(3);
        // each named, and the list within the window
        harness.get_by_label(&system);
        for (_, name) in crate::l10n::languages() {
            harness.get_by_label(name);
        }
        let screen = harness.ctx.content_rect();
        let areas: Vec<egui::Rect> = harness.ctx.memory(|memory| {
            let layers = memory.areas().visible_layer_ids();
            layers.iter().filter_map(|layer| memory.area_rect(layer.id)).collect()
        });
        assert!(areas.len() > 1, "{areas:?}");
        for area in areas {
            assert!(screen.expand(0.5).contains_rect(area), "{area:?} out of {screen:?}");
        }
        // picked, shown and kept
        harness.get_by_label("English (UK)").click_accesskit();
        harness.run_steps(3);
        assert_eq!(harness.state().config.gui.language.as_deref(), Some("en-GB"));
        assert_eq!(Config::load(&path).unwrap().gui.language.as_deref(), Some("en-GB"));
        let combo = harness.get_by_label(&language);
        assert_eq!(combo.accesskit_node().value().as_deref(), Some("English (UK)"));
    }

    /// While `TGRADISH_LANG` chooses the language, the choice shows that
    /// and says why it can't be changed.
    #[test]
    fn shows_the_language_tgradish_lang_chooses() {
        let mut harness = Harness::new_ui_state(
            |ui, chosen: &mut Option<String>| {
                language(ui, chosen, Some("pseudo"));
            },
            Some("ru".to_owned()),
        );
        harness.run();
        let language = fl!("language");
        let combo = harness.get_by_label(&language);
        assert_eq!(combo.accesskit_node().value().as_deref(), Some("pseudo"));
        assert!(combo.accesskit_node().is_disabled());
        let why = fl!("language", "variable", variable = "TGRADISH_LANG");
        harness.get_by_label(&why);
        assert_eq!(harness.state().as_deref(), Some("ru"));
        // the globe beside it is drawn by the GUI's own fonts
        assert!(GLOBE.chars().all(|c| crate::fonts::drawn().contains(&c)));
    }

    #[test]
    fn keeps_what_is_typed_once_typing_pauses_and_only_what_makes_sense() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut harness = kept_in(&path);
        harness.state_mut().dialogs.settings.open();
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
    fn limits_the_threads_conversions_use() {
        use egui::accesskit::Role;
        use std::num::NonZeroU32;
        use tgradish_core::config::Threads;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut harness = kept_in(&path);
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(2);
        // automatic, beside how many this computer runs at once, which a
        // number of one's own starts from; the ffmpeg's Automatic, where
        // there is one to choose, comes first
        let machine = std::thread::available_parallelism().unwrap().get().to_string();
        let automatic = |harness: &Harness<App>| {
            harness.get_all_by_label(&fl!("threads-auto")).last().unwrap().click_accesskit()
        };
        harness.get_by_label(&fl!("custom")).click_accesskit();
        harness.run_steps(2);
        let field = egui_kittest::kittest::by().role(Role::SpinButton).value(&machine);
        harness.get(field).focus();
        harness.run_steps(2);
        assert!(harness.ctx.text_edit_focused());
        // what it shows, after the cursor
        for _ in 0..machine.len() {
            harness.key_press(egui::Key::Delete);
            harness.run_steps(1);
        }
        harness.event(egui::Event::Text("3".into()));
        harness.key_press(egui::Key::Enter);
        harness.run_steps(30);
        let three = Threads::Limit(NonZeroU32::new(3).unwrap());
        assert_eq!(harness.state().config.threads, three);
        assert_eq!(Config::load(&path).unwrap().threads, three);
        // what is converted next runs in them
        let recipe = crate::item::Recipe::Tgs(Default::default());
        let setup = harness.state().setup(recipe, &Err(Text::default())).unwrap();
        assert_eq!(setup.threads, three);
        // back to automatic, and to the defaults
        automatic(&harness);
        harness.run_steps(2);
        assert_eq!(Config::load(&path).unwrap().threads, Threads::Auto);
        harness.state_mut().config.threads = three;
        harness.run_steps(30);
        harness.get_by_label(&fl!("reset-settings")).click_accesskit();
        harness.run_steps(3);
        assert_eq!(Config::load(&path).unwrap().threads, Threads::Auto);
    }

    #[test]
    fn resets_to_the_defaults_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut harness = kept_in(&path);
        harness.state_mut().dialogs.settings.open();
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
        // the message gone, once it has faded out
        harness.run_steps(3);
        assert!(harness.query_by_label(&fl!("settings-reset")).is_none());
        // and from the message once Settings has closed
        harness.get_by_label(&fl!("reset-settings")).click_accesskit();
        harness.run_steps(2);
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!harness.state().dialogs.settings.is_open() && !kept().overwrite);
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
        harness.state_mut().dialogs.settings.open();
        settle(&mut harness);
        harness.get_by_label(&fl!("scale-percent", percent = 100));
        // its keys said as other shortcuts are, Ctrl++ rather than Ctrl+=
        let (larger, smaller, reset) = ("Ctrl++", "Ctrl+-", "Ctrl+0");
        let keys = fl!("scale", "keys", larger = larger, smaller = smaller, reset = reset);
        harness.get_by_label(&keys);
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
        harness.state_mut().dialogs.settings.open();
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
            let drawn = harness.state().dialogs.settings.drawn.0;
            (drawn.center() - harness.ctx.content_rect().center()).length() < 1.0
        };
        harness.state_mut().dialogs.settings.open();
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
        assert!(harness.state().dialogs.settings.pinned.is_some() && !centred(&harness));
        // the pointer away: back in the middle, a moment later
        harness.event(Event::PointerMoved(egui::pos2(5.0, 5.0)));
        settle(&mut harness, 2);
        assert!(harness.state().dialogs.settings.pinned.is_none());
        settle(&mut harness, 30);
        assert!(centred(&harness) && harness.state().dialogs.settings.returning.is_none());
    }

    #[test]
    fn goes_back_to_the_middle_at_once_without_motion() {
        let mut config = Config::default();
        config.gui.motion = false;
        let mut harness = Harness::builder()
            .with_size([1280.0, 900.0])
            .with_step_dt(1.0 / 60.0)
            .build_eframe(move |_| App::with(config.clone(), None, Presets::builtin()));
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(16);
        let centre = |harness: &Harness<App>| harness.state().dialogs.settings.drawn.0.center();
        let middle = centre(&harness);
        // let go of in the corner, as the scale's buttons leave it
        let leave = |harness: &mut Harness<App>| {
            let now = harness.ctx.input(|input| input.time);
            harness.state_mut().dialogs.settings.returning = Some((egui::Pos2::ZERO, now));
            harness.step();
        };
        leave(&mut harness);
        assert!((centre(&harness) - middle).length() < 1.0);
        assert!(harness.state().dialogs.settings.returning.is_none());
        // with motion: on its way a frame later, there once its time is up
        harness.state_mut().config.gui.motion = true;
        harness.step();
        leave(&mut harness);
        let away = (centre(&harness) - middle).length();
        assert!(away > 50.0, "{away}");
        harness.run_steps(4);
        let nearer = (centre(&harness) - middle).length();
        assert!(nearer > 1.0 && nearer < away, "{nearer}");
        harness.run_steps(8);
        assert!((centre(&harness) - middle).length() < 1.0);
        assert!(harness.state().dialogs.settings.returning.is_none());
    }

    /// How far `label`'s button is out of what shows of the settings, in
    /// points: below it, or above it where less than nothing, and nothing
    /// where it shows, to a pixel. Scrolled out of sight, a click there
    /// goes to whatever is drawn there instead.
    fn out_of_sight(screen: &crate::testing::Screen, label: &str) -> f32 {
        let dialog = &screen.harness.state().dialogs.settings;
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
            let dialog = &screen.harness.state().dialogs.settings;
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
                assert!(app.dialogs.settings.pinned.is_some(), "{label} at {scale}");
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
                assert!(app.dialogs.settings.pinned.is_some(), "{label} at {scale}");
                let now = screen.pixels(label);
                assert!((now - under).length() < 2.0, "{label} at {scale}: {now:?}, not {under:?}");
                assert!(screen.on(label), "{label} at {scale}");
            }
        };
        let dialog = |screen: &Screen| {
            let dialog = &screen.harness.state().dialogs.settings;
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
        assert!(screen.harness.state().dialogs.settings.pinned.is_none());
    }

    #[test]
    fn stays_where_it_is_at_the_ends_of_the_scale() {
        // to the least, then − can't be clicked, and nothing else is: the
        // pointer on it still
        let smaller = fl!("scale-smaller");
        let mut screen = on_the_scale(0.7, [1280.0, 900.0], &smaller);
        for scale in [0.6, 0.5, 0.5, 0.5] {
            screen.click();
            screen.frames(4);
            assert_eq!(screen.harness.state().config.gui.scale, scale);
            assert!(screen.on(&smaller), "at {scale}");
        }
        let disabled = |screen: &crate::testing::Screen, label: &str| {
            screen.harness.get_by_label(label).accesskit_node().is_disabled()
        };
        assert_eq!(screen.harness.ctx.zoom_factor(), 0.5);
        assert!(disabled(&screen, &smaller) && !disabled(&screen, &fl!("scale-larger")));
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
        let dialog = &screen.harness.state().dialogs.settings;
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
    fn steps_on_while_held_with_the_button_kept_under_the_pointer() {
        let larger = fl!("scale-larger");
        let mut screen = on_the_scale(1.0, [1600.0, 1400.0], &larger);
        let under = screen.pixels(&larger);
        let scale = |screen: &crate::testing::Screen| screen.harness.state().config.gui.scale;
        let kept = |screen: &crate::testing::Screen| {
            let now = screen.pixels(&larger);
            assert!((now - under).length() < 2.0, "{now:?}, not {under:?}");
            assert!(screen.on(&larger));
        };
        // a step as it is pressed, then none for a while, frames a 60th of
        // a second apart
        screen.press(true);
        assert_eq!(scale(&screen), 1.1);
        screen.frames(20);
        assert_eq!(scale(&screen), 1.1);
        kept(&screen);
        // then from 0.4 s on, 8 a second, the button where it was: at
        // 0.4, 0.525 and 0.65 s, and on to 1.275 s
        screen.frames(20);
        assert_eq!(scale(&screen), 1.4);
        screen.frames(40);
        assert_eq!(scale(&screen), 1.9);
        kept(&screen);
        // none once let go of
        screen.press(false);
        let last = scale(&screen);
        screen.frames(60);
        assert_eq!(scale(&screen), last);
        kept(&screen);
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
        assert!(said.say().contains("config.toml.unreadable"), "{said}");
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "[ffmpeg]\nuse = \"auto\"\n");
        assert!(Config::load(&path).unwrap().gui.overwrite);
        // a file that reads is written over, and nothing more is kept
        std::fs::remove_file(&aside).unwrap();
        assert!(!save(&mut config, &path).unwrap().say().contains("unreadable"));
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
            app.dialogs.settings.open();
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
        harness.state_mut().dialogs.settings.open();
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

    /// What checking ffmpeg found is said whole in the smallest window, in
    /// room kept for it, so that nothing under it moves.
    #[test]
    fn says_what_checking_ffmpeg_found_whole_in_room_kept_for_it() {
        crate::l10n::english();
        let mut harness =
            Harness::builder().with_size([crate::MIN_WIDTH, 820.0]).build_eframe(|cc| {
                let app = App::with(Config::default(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(16);
        let under = |harness: &Harness<App>| harness.get_by_label(&fl!("threads")).rect();
        let before = under(&harness);
        let version = "8.0.1-full_build-www.gyan.dev Copyright (c) 2000-2025";
        let status = fl!("ffmpeg-no-vp9", version = version);
        let checked = Checked { said: Text::said(status.clone()), ..Checked::default() };
        harness.state_mut().dialogs.settings.ffmpeg_status = Some(checked);
        harness.run_steps(2);
        harness.get_by_label(&status);
        assert_eq!(under(&harness), before);
        let cut = harness.output().shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) => text.galley.elided && text.galley.text().contains("VP9"),
            _ => false,
        });
        assert!(!cut, "{status:?} cut short");
    }

    /// A path too long for its field is cut short in its middle, its
    /// file's name and what is said of it kept, and is said whole on hover.
    #[test]
    fn cuts_a_long_path_short_in_its_middle_whole_on_hover() {
        crate::l10n::english();
        let mut harness =
            Harness::builder().with_size([crate::MIN_WIDTH, 820.0]).build_eframe(|cc| {
                let app = App::with(Config::default(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
        let found = Path::new(r"C:\ProgramData\Chocolatey\bin\long-folder-name\ffmpeg.exe");
        let settings = &mut harness.state_mut().dialogs.settings;
        settings.open();
        settings.as_if_found(found, Path::new("/home/someone/Pictures/Stickers"));
        harness.run_steps(4);
        let whole = fl!("ffmpeg-found", path = found.display().to_string());
        let field = harness.get_by(|node| {
            node.role() == egui::accesskit::Role::TextInput
                && node
                    .placeholder()
                    .is_some_and(|text| text.ends_with("ffmpeg.exe, found on PATH"))
        });
        let shown = field.accesskit_node().placeholder().unwrap_or_default().to_owned();
        assert!(shown.starts_with(r"C:\Program") && shown.contains('…'), "{shown:?}");
        let cut = harness.output().shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::Text(text) => text.galley.elided,
            _ => false,
        });
        assert!(!cut, "cut short at its end");
        let at = field.rect().center();
        harness.event(egui::Event::PointerMoved(at));
        harness.run_steps(4);
        harness.get_by_label(&whole);
    }

    #[test]
    fn keeps_no_folder_without_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let at = path.clone();
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).build_eframe(move |_| {
            App::with(Config::default(), Some(at.clone()), Presets::builtin())
        });
        harness.state_mut().dialogs.settings.open();
        // which takes a few frames as its parts find their sizes
        harness.run_steps(16);
        let under =
            |harness: &Harness<App>| harness.get_by_label(&fl!("replace-in-the-way")).rect();
        let before = under(&harness);
        assert!(harness.query_by_label(&fl!("in-a-folder", "unchosen")).is_none());
        harness.get_by_label(&fl!("in-a-folder")).click_accesskit();
        harness.run_steps(2);
        // in a folder, said to be unchosen, yet none is kept; said in room
        // kept for it, so nothing under it moved
        harness.get_by_label(&fl!("in-a-folder", "unchosen"));
        assert_eq!(under(&harness), before);
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
        harness.state_mut().ffmpeg.given = (Some(FfmpegChoice::System), None);
        harness.state_mut().dialogs.settings.open();
        harness.run_steps(2);
        harness.get_by_label(&fl!("ffmpeg-given"));
        harness.get_by_label(&fl!("check")).click_accesskit();
        harness.run_steps(2);
        // what it finds is taken in as frames are drawn
        for _ in 0..500 {
            if harness.state().dialogs.settings.ffmpeg_status.is_some() {
                break;
            }
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // in tgradish's own words, as reading a file without it would say
        let status = harness.state().dialogs.settings.ffmpeg_status.clone();
        assert!(
            matches!(&status, Some(Checked { ready: false, said, path: Some(_) })
                if said.say().starts_with("ffmpeg wasn't found")),
            "{status:?}"
        );
    }
}
