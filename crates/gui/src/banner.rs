//! Unsolicited notices and warnings. Transient info notices (such as removed
//! files with an Undo action) appear as cards in the bottom-left corner over
//! the file list (mostly empty there). Actionable warnings (such as unreadable
//! or unsaved files) stay in the bottom banner to the right of the file list
//! until dismissed. Each row shows a headline, optional file path, reason,
//! note, action buttons, and raw program or OS output (collapsed when long).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, Sense, TextStyle, vec2};

use crate::l10n::fl;
use crate::theme;

/// How a message reads: a warning says something didn't work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Info,
    Warning,
}

/// What a message is about, where saying it again with other words or
/// details is to take the place of what was said before rather than add
/// to it, as the settings file reads differently once it is changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Subject {
    SettingsFile,
    Presets,
    /// Files removed a moment ago.
    Removed,
    /// Settings put back to their defaults a moment ago.
    SettingsReset,
}

/// A button in a message's row, and what clicking it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Writes the settings in use in place of the settings file, which
    /// can't be read, keeping that beside it.
    FreshSettings,
    /// Reads the settings file again, as after it was put right by hand.
    ReadSettingsAgain,
    /// Reads the presets again.
    ReadPresetsAgain,
    /// Copies the message's file's whole path.
    CopyPath,
    /// Opens the folder of the message's file, the file selected.
    ShowInFolder,
    /// Asks for the ffmpeg program, and keeps it in the settings, as a
    /// file's settings ask while it is missing.
    ChooseFfmpeg,
    /// Puts the files removed a moment ago back.
    Undo,
    /// Puts back the settings as they were before they were reset.
    UndoReset,
}

/// Where an action asked for waits, an `Option<Action>`: written by
/// [`ask`] from controls drawn where `App` isn't at hand, and taken by
/// `App` with [`asked`] once the banner is drawn, in the same frame.
fn asked_id() -> egui::Id {
    egui::Id::new("asked action")
}

/// Asks for `action` to be done, as a message's button would, from
/// where `App` itself isn't at hand.
pub(crate) fn ask(ctx: &egui::Context, action: Action) {
    ctx.data_mut(|data| data.insert_temp(asked_id(), Some(action)));
}

/// What [`ask`] asked for, once.
pub(crate) fn asked(ctx: &egui::Context) -> Option<Action> {
    ctx.data_mut(|data| data.remove_temp::<Option<Action>>(asked_id())).flatten()
}

impl Action {
    fn label(self) -> String {
        match self {
            Action::FreshSettings => fl!("fresh-settings"),
            Action::ReadSettingsAgain | Action::ReadPresetsAgain => fl!("read-again"),
            Action::CopyPath => fl!("copy-path"),
            Action::ShowInFolder => fl!("folder-of"),
            Action::ChooseFfmpeg => fl!("choose-ffmpeg"),
            Action::Undo => fl!("undo-removing"),
            Action::UndoReset => fl!("undo-reset"),
        }
    }

    fn hint(self) -> String {
        match self {
            Action::FreshSettings => fl!("fresh-settings", "hint"),
            Action::ReadSettingsAgain | Action::ReadPresetsAgain => fl!("read-again", "hint"),
            Action::CopyPath => fl!("copy-path", "hint"),
            Action::ShowInFolder => fl!("folder-of", "hint"),
            Action::ChooseFfmpeg => fl!("choose-ffmpeg", "hint"),
            Action::Undo => fl!("undo-removing", "hint"),
            Action::UndoReset => fl!("undo-reset", "hint"),
        }
    }

    /// The way out most people want, a button among links.
    fn primary(self) -> bool {
        matches!(self, Action::FreshSettings | Action::ChooseFfmpeg)
    }
}

/// Something tgradish says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) severity: Severity,
    /// What happened, in a short sentence of tgradish's own.
    pub(crate) headline: String,
    /// The file it is about.
    pub(crate) file: Option<PathBuf>,
    /// Why, in tgradish's own words, in a line of its own.
    pub(crate) reason: Option<String>,
    /// What it means or what can be done, in a line of its own.
    pub(crate) note: Option<String>,
    /// What a program or the system said, as it said it, in monospace
    /// and in its own lines and columns: never tgradish's own words,
    /// which are the reason's.
    pub(crate) literal: Option<String>,
    /// Buttons that do something about it, the way out most want first.
    pub(crate) actions: Vec<Action>,
    pub(crate) subject: Option<Subject>,
}

impl Message {
    pub(crate) fn info(headline: impl Into<String>) -> Message {
        Message::new(Severity::Info, headline.into())
    }

    pub(crate) fn warning(headline: impl Into<String>) -> Message {
        Message::new(Severity::Warning, headline.into())
    }

    fn new(severity: Severity, headline: String) -> Message {
        Message {
            severity,
            headline,
            file: None,
            reason: None,
            note: None,
            literal: None,
            actions: Vec::new(),
            subject: None,
        }
    }

    pub(crate) fn file(self, file: impl Into<PathBuf>) -> Message {
        Message { file: Some(file.into()), ..self }
    }

    pub(crate) fn reason(self, reason: impl Into<String>) -> Message {
        Message { reason: Some(reason.into()), ..self }
    }

    pub(crate) fn note(self, note: impl Into<String>) -> Message {
        Message { note: Some(note.into()), ..self }
    }

    /// With `literal`, what a program or the system said, the blank lines
    /// and spaces at its end left out.
    pub(crate) fn literal(self, literal: impl AsRef<str>) -> Message {
        let literal = literal.as_ref().trim_end();
        Message { literal: (!literal.is_empty()).then(|| literal.to_owned()), ..self }
    }

    /// With the buttons `actions`; those about a file only if it has one.
    pub(crate) fn actions(self, actions: &[Action]) -> Message {
        let filed = |action: &&Action| {
            self.file.is_some() || !matches!(action, Action::CopyPath | Action::ShowInFolder)
        };
        let actions = actions.iter().filter(filed).copied().collect();
        Message { actions, ..self }
    }

    pub(crate) fn about(self, subject: Subject) -> Message {
        Message { subject: Some(subject), ..self }
    }

    /// Everything it says, for finding fonts for it.
    pub(crate) fn texts(&self) -> impl Iterator<Item = String> + '_ {
        let file = self.file.as_ref().map(|file| file.display().to_string());
        let parts = [&self.reason, &self.note, &self.literal].into_iter().flatten().cloned();
        std::iter::once(self.headline.clone()).chain(file).chain(parts)
    }
}

/// A message shown, with how it is shown.
pub(crate) struct Said {
    /// Tells its row's state apart in egui's memory.
    pub(crate) id: u64,
    pub(crate) message: Message,
    /// How many times it was said while it showed.
    pub(crate) count: u32,
    /// When it was first drawn, or last hovered: information goes after a
    /// while.
    since: Option<f64>,
    pub(crate) details_open: bool,
    /// What an action said of itself, as "Copied", and until when.
    pub(crate) flash: Option<(String, f64)>,
    /// When it was first drawn, from which it fades in.
    came: Option<f64>,
    /// How tall it was last drawn, with the room after it, which closes
    /// up as it goes.
    height: f32,
}

impl Said {
    /// Whether it passes, shown in the corner over the list.
    fn passes(&self) -> bool {
        self.message.severity == Severity::Info
    }
}

/// The messages shown, oldest first.
#[derive(Default)]
pub(crate) struct Banner {
    pub(crate) rows: Vec<Said>,
    next_id: u64,
    /// Where the rows of the list of files are seen, under whose corner
    /// what passes is shown, as the list last drew it; without files, the
    /// part of the drop zone the list would take.
    pub(crate) corner: Option<egui::Rect>,
    /// The top of the bar of actions under the preview as drawn this
    /// frame, whose height changes as it goes on two lines; `None` where
    /// there is none.
    pub(crate) bar: Option<f32>,
    /// When the messages were last drawn: hovering what passes holds it.
    drawn: Option<f64>,
    /// Whether what passes was held as they were: the time since, until
    /// they are drawn again, is its as it was then, however long, as
    /// nothing may be drawn while the pointer rests.
    held: bool,
    /// What was said and is no longer, fading out where it was while its
    /// room closes up, from when.
    leaving: Vec<(Said, Option<f64>)>,
}

/// How long information shows unless hovered; warnings stay until
/// dismissed, as they ask for something to be done.
const SHOWN: f64 = 12.0;
/// How long what an action says of itself shows.
const FLASHED: f64 = 2.5;
/// Lines of details shown without asking.
const SHORT_DETAIL: usize = 3;
/// The widest the banner grows, its frame left out.
const WIDEST: f32 = 640.0;
/// Room left between the banner and the window's sides.
const SIDE_ROOM: f32 = 24.0;
/// How far up from the window's bottom the banner stands where there is
/// no bar of actions, as over one on a single line; and how far down past
/// a bar's top it comes, into the room over its buttons.
const OVER_NO_BAR: f32 = 56.0;
const INTO_BAR: f32 = 6.0;
/// The icon's side, and the room between it, the text and the button.
const ICON: f32 = 16.0;
const GAP: f32 = 10.0;
/// The dismiss button's width.
const DISMISS: f32 = 22.0;
/// The most notices shown at once in the corner: the oldest goes first.
const MOST_PASSING: usize = 3;
/// Room around the notices in the corner, and between them.
const CORNER_ROOM: f32 = 8.0;
/// Room inside a notice, around what it says.
const NOTICE_MARGIN: egui::Margin = egui::Margin { left: 10, right: 8, top: 8, bottom: 9 };
/// How thick the line is that shortens as a notice's time runs out.
const COUNTDOWN: f32 = 2.0;
/// The longest the line goes undrawn as it shortens, a pixel or two.
const COUNTDOWN_STEP: f64 = 1.0 / 15.0;
/// The tallest the details get before they scroll.
const DETAIL_HEIGHT: f32 = 160.0;
/// How long what is said takes to fade in, and what goes to fade out as
/// its room closes up, in seconds.
const FADE: f64 = 0.15;

impl Banner {
    /// Shows `message` under what is already said. What is showing
    /// already is said once, counted; a message about a subject says it
    /// once however often, and takes the place of another about it.
    pub(crate) fn say(&mut self, message: Message) {
        if let Some(said) = self.rows.iter_mut().find(|said| said.message == message) {
            if message.subject.is_none() {
                said.count += 1;
                said.since = None;
            }
            return;
        }
        if let Some(subject) = message.subject
            && let Some(said) =
                self.rows.iter_mut().find(|said| said.message.subject == Some(subject))
        {
            said.message = message;
            said.since = None;
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.rows.push(Said {
            id,
            message,
            count: 1,
            since: None,
            details_open: false,
            flash: None,
            came: None,
            height: 0.0,
        });
        // the oldest passing notice goes for the newest
        if self.rows.iter().filter(|said| said.passes()).count() > MOST_PASSING
            && let Some(oldest) = self.rows.iter().position(Said::passes)
        {
            let oldest = self.rows[oldest].id;
            self.take_out(|said| said.id == oldest, None);
        }
    }

    /// Stops showing what was said about `subject`.
    pub(crate) fn forget(&mut self, subject: Subject) {
        self.take_out(|said| said.message.subject == Some(subject), None);
    }

    /// Takes what `gone` picks out of what is said, to fade out and close
    /// up from `now`, or from when it is next drawn.
    fn take_out(&mut self, gone: impl Fn(&Said) -> bool, now: Option<f64>) {
        let (out, kept) = std::mem::take(&mut self.rows).into_iter().partition(|said| gone(said));
        self.rows = kept;
        let out: Vec<Said> = out;
        self.leaving.extend(out.into_iter().map(|said| (said, now)));
    }

    /// Shows a moment's `text` beside the buttons of `message`, or of
    /// what took its place, from `now`.
    pub(crate) fn flash(&mut self, message: &Message, text: String, now: f64) {
        let of = |said: &&mut Said| {
            said.message == *message
                || message.subject.is_some() && said.message.subject == message.subject
        };
        if let Some(said) = self.rows.iter_mut().find(of) {
            said.flash = Some((text, now + FLASHED));
        }
    }

    /// Draws the banner, if there is anything to say, and the notices in
    /// the corner over the list, and takes in their dismissals. Gives the
    /// actions clicked, with the messages they are of, to be done.
    pub(crate) fn show(&mut self, ctx: &egui::Context) -> Vec<(Message, Action)> {
        let mut clicked = Vec::new();
        let mut dismissed = Vec::new();
        let now = ctx.input(|input| input.time);
        for (_, since) in &mut self.leaving {
            since.get_or_insert(now);
        }
        let gone = |since: &Option<f64>| crate::motion::along(ctx, since.unwrap_or(now), FADE);
        self.leaving.retain(|(_, since)| gone(since) < 1.0);
        let all = self.banner(ctx, now, &mut clicked, &mut dismissed);
        // the next frame's bar says where it is again, if there is one
        self.bar = None;
        let held = self.passing(ctx, now, &mut clicked, &mut dismissed);
        // hovering what passes holds it where it is, for as long: the time
        // since it was last drawn by whether it was held then, as it may
        // be let go of only now, after a long while undrawn
        let passed = self.drawn.map_or(0.0, |drawn| (now - drawn).max(0.0));
        let was_held = std::mem::replace(&mut self.held, held);
        self.drawn = Some(now);
        for said in &mut self.rows {
            let passes = said.passes();
            match &mut said.since {
                None => said.since = Some(now),
                Some(since) if passes => {
                    if was_held {
                        *since += passed;
                    }
                    if !held && now - *since > SHOWN {
                        dismissed.push(said.id);
                    }
                }
                Some(_) => {}
            }
            if said.flash.as_ref().is_some_and(|(_, until)| now > *until) {
                said.flash = None;
            }
        }
        if all {
            self.take_out(|said| !said.passes(), Some(now));
        }
        self.take_out(|said| dismissed.contains(&said.id), Some(now));
        // drawn again as the first goes or stops flashing, and as the line
        // of what passes shortens, and not more often
        let passing = self.rows.iter().any(Said::passes);
        let shortens = (passing && !held).then_some(now + COUNTDOWN_STEP);
        let next = self
            .rows
            .iter()
            .flat_map(|said| {
                let goes = said.passes().then(|| said.since.map(|since| since + SHOWN)).flatten();
                goes.into_iter().chain(said.flash.as_ref().map(|(_, until)| *until))
            })
            .chain(shortens)
            .fold(f64::INFINITY, f64::min);
        if next.is_finite() {
            // a little after, so it is past by then
            let wait = (next - now).max(0.0) + 0.05;
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        clicked
    }

    /// Draws the banner of what asks for something to be done, along the
    /// bottom, right of the list, if there is any. Gives
    /// whether all of it was dismissed at once.
    fn banner(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        clicked: &mut Vec<(Message, Action)>,
        dismissed: &mut Vec<u64>,
    ) -> bool {
        let staying = self.rows.iter().filter(|said| !said.passes()).count();
        if staying == 0 && self.leaving.iter().all(|(said, _)| said.passes()) {
            return false;
        }
        // in the middle of what is right of the list, or of where it would
        // be, clear of what passes in its corner, and where it stays as
        // files are added
        let screen = ctx.content_rect();
        let left = self.corner.map_or(screen.left(), |corner| corner.right()).min(screen.right());
        let middle = (left + screen.right()) / 2.0;
        let widest = WIDEST.min(screen.right() - left - 2.0 * SIDE_ROOM).max(200.0);
        // over the bar, however many lines it takes
        let above = self.bar.map_or(OVER_NO_BAR, |top| screen.bottom() - top - INTO_BAR);
        let mut all = false;
        egui::Area::new(egui::Id::new("banner"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, vec2(middle - screen.center().x, -above))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(12)).show(
                    ui,
                    |ui| {
                        let room = widest - 24.0 - ICON - 2.0 * GAP - DISMISS;
                        let entries = entries(&mut self.rows, &mut self.leaving, false);
                        let column = entries
                            .iter()
                            .map(|(said, _)| needs(ui, said, room))
                            .fold(0.0, f32::max);
                        let column = column.clamp(160.0_f32.min(room), room);
                        ui.set_width(ICON + 2.0 * GAP + column + DISMISS);
                        for (index, (said, leaving)) in entries.into_iter().enumerate() {
                            let row = shown(ui, said, now, leaving, |ui, said| {
                                if index > 0 {
                                    hairline(ui);
                                }
                                draw(ui, said, column, now, false)
                            });
                            let Some(row) = row else { continue };
                            clicked
                                .extend(row.actions.into_iter().map(|a| (said.message.clone(), a)));
                            if row.dismissed {
                                dismissed.push(said.id);
                            }
                        }
                        if staying > 2 {
                            hairline(ui);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    all = link_button(ui, fl!("dismiss", "all")).clicked();
                                },
                            );
                        }
                    },
                );
            });
        all
    }

    /// Draws what passes in the corner over the list, each notice on a
    /// card of its own, the newest at the bottom. Where they are said is
    /// there with none, for screen readers to say them as they come. Gives
    /// whether they are held: hovered, or with the keyboard's focus.
    fn passing(
        &mut self,
        ctx: &egui::Context,
        now: f64,
        clicked: &mut Vec<(Message, Action)>,
        dismissed: &mut Vec<u64>,
    ) -> bool {
        let corner = self.corner.unwrap_or_else(|| ctx.content_rect());
        let width = (corner.width() - 2.0 * CORNER_ROOM).max(160.0);
        let id = egui::Id::new("notices");
        let before = egui::AreaState::load(ctx, id).and_then(|state| state.size);
        let mut focused = false;
        // under menus, but over a dialog while one is open, as one said
        // there can be acted on there, as Settings put back to their
        // defaults is undone
        let over_dialog = ctx.memory(|memory| memory.top_modal_layer().is_some());
        let order = if over_dialog { egui::Order::Foreground } else { egui::Order::Middle };
        let area = egui::Area::new(id)
            .order(order)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(egui::pos2(corner.left() + CORNER_ROOM, corner.bottom() - CORNER_ROOM));
        if over_dialog {
            ctx.move_to_top(area.layer());
        }
        let area = area.show(ctx, |ui| {
            ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                node.set_live(egui::accesskit::Live::Polite);
            });
            ui.spacing_mut().item_spacing.y = CORNER_ROOM;
            let room = width - NOTICE_MARGIN.sum().x - ICON - 2.0 * GAP - DISMISS;
            for (said, leaving) in entries(&mut self.rows, &mut self.leaving, true) {
                let row = shown(ui, said, now, leaving, |ui, said| {
                    let card = egui::Frame::new()
                        .fill(ui.visuals().panel_fill)
                        .stroke(theme::floating_outline(ui.visuals()))
                        .corner_radius(ui.visuals().window_corner_radius)
                        .shadow(theme::floating_shadow(ui.visuals()))
                        .inner_margin(NOTICE_MARGIN)
                        .show(ui, |ui| {
                            ui.ctx().accesskit_node_builder(ui.unique_id(), |node| {
                                node.set_role(egui::accesskit::Role::Status);
                            });
                            ui.set_width(width - NOTICE_MARGIN.sum().x);
                            draw(ui, said, room, now, true)
                        });
                    // the time left, held while hovered
                    let left = said.since.map_or(1.0, |since| (1.0 - (now - since) / SHOWN) as f32);
                    countdown(ui, card.response.rect, left.clamp(0.0, 1.0));
                    card.inner
                });
                let Some(row) = row else { continue };
                focused |= row.focused;
                clicked.extend(row.actions.into_iter().map(|a| (said.message.clone(), a)));
                if row.dismissed {
                    dismissed.push(said.id);
                }
            }
        });
        // drawn again at once where the notices take more room or less
        // than last time, as they are laid out from where they end
        if before.is_some_and(|before| before != area.response.rect.size()) {
            ctx.request_discard("notices resized");
        }
        // over the list's bar where they are taller than the list: the
        // oldest goes, until they fit or one is left
        let tall = area.response.rect.height() > corner.height() - 2.0 * CORNER_ROOM;
        if tall && self.rows.iter().filter(|said| said.passes()).count() > 1 {
            dismissed.extend(self.rows.iter().find(|said| said.passes()).map(|said| said.id));
        }
        area.response.contains_pointer() || focused
    }
}

/// What is said in the corner where `passing`, else in the banner, with
/// what is going there, in the order it was said: whether each is going,
/// and since when.
fn entries<'a>(
    rows: &'a mut [Said],
    leaving: &'a mut [(Said, Option<f64>)],
    passing: bool,
) -> Vec<(&'a mut Said, Option<f64>)> {
    let staying = rows.iter_mut().map(|said| (said, None));
    let going = leaving.iter_mut().map(|(said, since)| (said, *since));
    let mut entries: Vec<_> = staying.chain(going).filter(|(s, _)| s.passes() == passing).collect();
    entries.sort_by_key(|(said, _)| said.id);
    entries
}

/// Draws `said` with `add`, fading in as it is first drawn, and where it is
/// `leaving`, since then, fading out as its room closes up from below,
/// what it does then left undone. Gives what `add` gave, unless leaving.
fn shown<R>(
    ui: &mut egui::Ui,
    said: &mut Said,
    now: f64,
    leaving: Option<f64>,
    add: impl FnOnce(&mut egui::Ui, &mut Said) -> R,
) -> Option<R> {
    let ctx = ui.ctx().clone();
    let Some(since) = leaving else {
        let came = crate::motion::along(&ctx, *said.came.get_or_insert(now), FADE);
        let (top, opacity) = (ui.cursor().top(), ui.opacity());
        ui.multiply_opacity(came);
        let inner = add(ui, said);
        ui.set_opacity(opacity);
        said.height = ui.cursor().top() - top;
        return Some(inner);
    };
    let left = 1.0 - crate::inset::eased(f64::from(crate::motion::along(&ctx, since, FADE)));
    let at = ui.cursor().min;
    let width = ui.available_width();
    ui.add_space(said.height * left);
    let whole = egui::Rect::from_min_size(at, vec2(width, said.height));
    let shown = egui::Rect::from_min_size(at, vec2(width, said.height * left));
    let layout = *ui.layout();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(whole).layout(layout));
    child.set_clip_rect(shown.intersect(ui.clip_rect()));
    child.multiply_opacity(left);
    add(&mut child, said);
    None
}

/// A line along the bottom of `card`, as long as the `left` of its time.
fn countdown(ui: &egui::Ui, card: egui::Rect, left: f32) {
    let inset = f32::from(ui.visuals().window_corner_radius.sw).max(4.0);
    let full = card.width() - 2.0 * inset;
    let at = egui::pos2(card.left() + inset, card.bottom() - 1.0 - COUNTDOWN);
    let line = egui::Rect::from_min_size(at, vec2(full * left, COUNTDOWN));
    ui.painter().rect_filled(line, COUNTDOWN / 2.0, theme::mark(ui.visuals()));
}

impl crate::app::App {
    /// Does what the button `action` of `message` is for.
    pub(crate) fn act(&mut self, ctx: &egui::Context, message: &Message, action: Action) {
        let now = ctx.input(|input| input.time);
        match action {
            Action::CopyPath => {
                let Some(file) = &message.file else { return };
                ctx.copy_text(copied_path(file));
                self.banner.flash(message, fl!("copy-path", "done"), now);
            }
            Action::ShowInFolder => {
                if let Some(file) = &message.file {
                    crate::system::reveal(file);
                }
            }
            Action::FreshSettings => self.fresh_settings(),
            Action::ReadSettingsAgain => {
                let Some(path) = self.config_path.clone() else { return };
                match crate::config::Config::load(&path) {
                    // taken up as when the GUI starts
                    Ok(config) => {
                        if config.gui.language != self.config.gui.language {
                            crate::l10n::choose(config.gui.language.as_deref());
                        }
                        self.config = config;
                        self.banner.forget(Subject::SettingsFile);
                        self.say(Message::info(fl!("settings-read")).file(path));
                    }
                    Err(err) => {
                        let headline = message.headline.clone();
                        self.say(crate::app::settings_unreadable(headline, &err));
                        self.banner.flash(message, fl!("read-again", "failed"), now);
                    }
                }
            }
            Action::ChooseFfmpeg => self.picker.pick(ctx, crate::files::Pick::Ffmpeg),
            Action::Undo => self.undo_removing(),
            Action::UndoReset => self.undo_reset(),
            Action::ReadPresetsAgain => {
                if !self.reload_presets() {
                    self.banner.flash(message, fl!("read-again", "failed"), now);
                }
            }
        }
    }

    /// Writes the settings in use in place of the settings file, which
    /// can't be read, keeping that beside it, as Settings saves.
    /// The changes that couldn't be saved are among them.
    fn fresh_settings(&mut self) {
        let Some(path) = self.config_path.clone() else { return };
        match crate::settings_dialog::write(&mut self.config, &path) {
            Ok(kept) => {
                self.banner.forget(Subject::SettingsFile);
                let message = Message::info(fl!("fresh-settings", "saved"));
                self.say(match kept {
                    Some(kept) => {
                        let name = kept.file_name().unwrap_or_default().to_string_lossy();
                        let note = fl!("fresh-settings", "kept", name = name.into_owned());
                        message.file(kept).note(note).actions(&[Action::ShowInFolder])
                    }
                    None => message.file(path),
                });
            }
            Err(err) => {
                // about the file that couldn't be read or written, where
                // that is the copy of the old one rather than the new
                let message = Message::warning(fl!("fresh-settings", "failed")).file(path);
                self.say(crate::app::told(message, &err));
            }
        }
    }
}

/// The whole path of `file`, as Copy path copies it.
pub(crate) fn copied_path(file: &Path) -> String {
    std::path::absolute(file).unwrap_or_else(|_| file.to_path_buf()).display().to_string()
}

/// What a row's buttons did.
struct Row {
    actions: Vec<Action>,
    dismissed: bool,
    /// Whether one of them has the keyboard's focus.
    focused: bool,
}

/// A line across the banner between rows.
fn hairline(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        line.x_range(),
        line.center().y,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    ui.add_space(4.0);
}

/// Whether the details show straight away: when they are short.
fn short(detail: &str) -> bool {
    detail.lines().count() <= SHORT_DETAIL
}

/// The headline with how many times it was said, wrapped at `width`.
fn headline(ui: &egui::Ui, said: &Said, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let body = TextStyle::Body.resolve(ui.style());
    job.append(
        &said.message.headline,
        0.0,
        egui::TextFormat::simple(body, ui.visuals().text_color()),
    );
    if said.count > 1 {
        let small = egui::FontId::proportional(12.0);
        let times = fl!("said-times", count = said.count);
        let format = egui::TextFormat {
            valign: egui::Align::Center,
            ..egui::TextFormat::simple(small, ui.visuals().weak_text_color())
        };
        job.append(&times, 8.0, format);
    }
    job.wrap.max_width = width;
    job
}

/// The detail in monospace, wrapped at `width` only where a line is wider.
fn detail_job(ui: &egui::Ui, detail: &str, width: f32) -> egui::text::LayoutJob {
    let mono = TextStyle::Monospace.resolve(ui.style());
    let mut job =
        egui::text::LayoutJob::simple(detail.to_owned(), mono, ui.visuals().text_color(), width);
    job.wrap.break_anywhere = true;
    job
}

/// Room around the details in their block.
const DETAIL_MARGIN: egui::Margin = egui::Margin::symmetric(8, 6);

/// How wide a row's text needs to be, up to `room`.
fn needs(ui: &egui::Ui, said: &Said, room: f32) -> f32 {
    let painter = ui.painter();
    let body = TextStyle::Body.resolve(ui.style());
    let message = &said.message;
    let mut widest = painter.layout_job(headline(ui, said, room)).size().x;
    for line in [&message.reason, &message.note].into_iter().flatten() {
        let job = egui::text::LayoutJob::simple(line.clone(), body.clone(), Color32::WHITE, room);
        widest = widest.max(painter.layout_job(job).size().x);
    }
    if let Some(file) = &message.file {
        let shown = home_shortened(file);
        widest = widest.max(painter.layout_no_wrap(shown, body.clone(), Color32::WHITE).size().x);
    }
    // the buttons, as if on one line
    let padding = ui.spacing().button_padding.x * 2.0 + ui.spacing().item_spacing.x;
    let mut buttons: f32 = message
        .actions
        .iter()
        .map(|action| {
            painter.layout_no_wrap(action.label(), body.clone(), Color32::WHITE).size().x + padding
        })
        .sum();
    if message.literal.as_deref().is_some_and(|detail| !short(detail)) {
        buttons += painter.layout_no_wrap(fl!("details"), body, Color32::WHITE).size().x + 20.0;
    }
    widest = widest.max(buttons);
    let open = said.details_open || message.literal.as_deref().is_some_and(short);
    if let Some(detail) = message.literal.as_deref().filter(|_| open) {
        let block = painter.layout_job(detail_job(ui, detail, f32::INFINITY)).size().x;
        widest = widest.max(block + DETAIL_MARGIN.sum().x + ui.spacing().scroll.bar_width);
    }
    widest.min(room)
}

/// Draws a row, its text `column` wide; with `inline`, on one line with
/// its buttons where they fit beside its headline, as a notice's Undo.
fn draw(ui: &mut egui::Ui, said: &mut Said, column: f32, now: f64, inline: bool) -> Row {
    let mut row = Row { actions: Vec::new(), dismissed: false, focused: false };
    let inline = inline && fits_inline(ui, said, column);
    // the icon and the dismiss button level with the headline's first
    // line, or with the middle of the buttons beside it
    let line =
        if inline { ui.spacing().interact_size.y } else { ui.text_style_height(&TextStyle::Body) };
    let top_left = ui.cursor().min;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (icon, _) = ui.allocate_exact_size(vec2(ICON, line), Sense::hover());
        paint_icon(ui, said.message.severity, icon);
        ui.add_space(GAP);
        ui.vertical(|ui| {
            ui.set_width(column);
            ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
            if inline {
                headline_and_buttons(ui, said, column, &mut row);
            } else {
                text(ui, said, column, now, &mut row);
            }
        });
        ui.add_space(GAP + DISMISS);
    });
    // at the same place in every row, level with the headline's first line
    let at = egui::pos2(top_left.x + ICON + GAP + column + GAP, top_left.y);
    let rect = egui::Rect::from_min_size(at, vec2(DISMISS, line));
    let response = ui.interact(rect, ui.id().with(("dismiss", said.id)), Sense::click());
    let dismiss = fl!("dismiss");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &dismiss));
    let colour = if response.hovered() {
        let fill = ui.visuals().widgets.hovered.weak_bg_fill;
        ui.painter().rect_filled(rect.expand(2.0), 4.0, fill);
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let cross = egui::FontId::proportional(11.0);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "✖", cross, colour);
    row.focused |= response.has_focus();
    row.dismissed = response.on_hover_text(dismiss).clicked();
    row
}

/// The room between buttons on their line.
const BUTTON_GAP: f32 = 12.0;

/// Whether a row says no more than its headline, with buttons that fit on
/// its line `column` wide.
fn fits_inline(ui: &egui::Ui, said: &Said, column: f32) -> bool {
    let message = &said.message;
    if message.actions.is_empty()
        || message.file.is_some()
        || message.reason.is_some()
        || message.note.is_some()
        || message.literal.is_some()
        || said.flash.is_some()
    {
        return false;
    }
    let painter = ui.painter();
    let body = TextStyle::Body.resolve(ui.style());
    let headline = painter.layout_job(headline(ui, said, f32::INFINITY)).size().x;
    let buttons: f32 = message
        .actions
        .iter()
        .map(|action| {
            let label = painter.layout_no_wrap(action.label(), body.clone(), Color32::WHITE);
            let padding = if action.primary() { 2.0 * ui.spacing().button_padding.x } else { 0.0 };
            BUTTON_GAP + label.size().x + padding
        })
        .sum();
    headline + buttons <= column
}

/// A row's headline and its buttons, on one line: the buttons at its end.
fn headline_and_buttons(ui: &mut egui::Ui, said: &Said, column: f32, row: &mut Row) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(headline(ui, said, column)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = BUTTON_GAP;
            for &action in said.message.actions.iter().rev() {
                action_button(ui, action, row);
            }
        });
    });
}

/// The button of `action`, taking in what it did into `row`: on one line,
/// going on to the next whole where there isn't room for it on this one.
fn action_button(ui: &mut egui::Ui, action: Action, row: &mut Row) {
    let label = action.label();
    let response = if action.primary() {
        let button = egui::Button::new(label).stroke(theme::on_outline(ui.visuals()));
        ui.add(button.wrap_mode(egui::TextWrapMode::Extend))
    } else {
        link_button(ui, label)
    };
    row.focused |= response.has_focus();
    if response.on_hover_text(action.hint()).clicked() {
        row.actions.push(action);
    }
}

/// The text column of a row: headline, file, reason, note, buttons and
/// details.
fn text(ui: &mut egui::Ui, said: &mut Said, column: f32, now: f64, row: &mut Row) {
    let message = &said.message;
    ui.add(egui::Label::new(headline(ui, said, column)).wrap());
    if let Some(file) = &message.file {
        let body = TextStyle::Body.resolve(ui.style());
        let shown = elided(ui, &home_shortened(file), &body, column);
        let label = egui::RichText::new(shown).color(ui.visuals().weak_text_color());
        ui.add(egui::Label::new(label).extend()).on_hover_text(file.display().to_string());
    }
    for line in [&message.reason, &message.note].into_iter().flatten() {
        ui.add(egui::Label::new(line.as_str()).wrap());
    }
    let long = message.literal.as_deref().is_some_and(|detail| !short(detail));
    if !message.actions.is_empty() || long {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(BUTTON_GAP, 6.0);
            for &action in &message.actions {
                action_button(ui, action, row);
            }
            if long {
                disclosure(ui, &mut said.details_open, &fl!("details"));
            }
            if let Some((flash, until)) = &said.flash
                && now <= *until
            {
                ui.label(
                    egui::RichText::new(format!("✔ {flash}")).color(theme::good(ui.visuals())),
                );
            }
        });
    }
    let message = &said.message;
    if let Some(detail) = message.literal.as_deref().filter(|_| !long || said.details_open) {
        ui.add_space(2.0);
        detail_block(ui, detail, column, said.id);
    }
}

/// `detail` in a block set into the banner, as wide as its widest line up
/// to `column`, and scrolling once it is tall.
fn detail_block(ui: &mut egui::Ui, detail: &str, column: f32, id: u64) {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let room = column - DETAIL_MARGIN.sum().x - 2.0 * stroke.width;
    let widest = ui.painter().layout_job(detail_job(ui, detail, f32::INFINITY)).size().x;
    let mut galley = ui.painter().layout_job(detail_job(ui, detail, room));
    let tall = galley.size().y > DETAIL_HEIGHT;
    if tall {
        // the scroll bar's room taken from the lines
        let room = room - ui.spacing().scroll.bar_width - ui.spacing().scroll.bar_inner_margin;
        galley = ui.painter().layout_job(detail_job(ui, detail, room));
    }
    let bar = if tall {
        ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin
    } else {
        0.0
    };
    let width = (widest.ceil() + bar).min(room);
    egui::Frame::new()
        .fill(ui.visuals().code_bg_color)
        .stroke(stroke)
        .corner_radius(4)
        .inner_margin(DETAIL_MARGIN)
        .show(ui, |ui| {
            ui.set_width(width);
            // as tall as the lines, which the banner's room doesn't limit
            let height = galley.size().y.min(DETAIL_HEIGHT);
            egui::ScrollArea::vertical()
                .id_salt(("detail", id))
                .min_scrolled_height(height)
                .max_height(DETAIL_HEIGHT)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(galley));
                });
        });
}

/// A button drawn as a link, for what is done beside a message rather than
/// a page opened: a button to a screen reader.
fn link_button(ui: &mut egui::Ui, text: String) -> egui::Response {
    let colour = ui.visuals().hyperlink_color;
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = 0.0;
        let button = egui::Button::new(egui::RichText::new(text).color(colour)).frame(false);
        let response = ui.add(button.wrap_mode(egui::TextWrapMode::Extend));
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        response
    })
    .inner
}

/// A toggle of something folded away: an arrow pointing at `text`, then
/// down once open.
fn disclosure(ui: &mut egui::Ui, open: &mut bool, text: &str) {
    let body = TextStyle::Body.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), body, ui.visuals().text_color());
    let arrow = 12.0;
    // as tall as the buttons beside it, to be as easy to hit
    let size = vec2(arrow + 4.0 + galley.size().x, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (shown, now_open) = (text.to_owned(), *open);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, now_open, shown.clone())
    });
    if response.clicked() {
        *open = !*open;
    }
    let colour = if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().text_color()
    };
    let icon = egui::Rect::from_center_size(
        egui::pos2(rect.left() + arrow / 2.0, rect.center().y),
        vec2(7.0, 7.0),
    );
    let points = if *open {
        vec![icon.left_top(), icon.right_top(), icon.center_bottom()]
    } else {
        vec![icon.left_top(), icon.right_center(), icon.left_bottom()]
    };
    ui.painter().add(egui::Shape::convex_polygon(points, colour, egui::Stroke::NONE));
    let at = egui::pos2(rect.left() + arrow + 4.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, colour);
}

/// The icon of `severity` in `rect`: a triangle with "!" for warnings, a
/// circle with "i" for information, as Breeze draws them.
fn paint_icon(ui: &egui::Ui, severity: Severity, rect: egui::Rect) {
    let painter = ui.painter();
    let centre = rect.center();
    let side = ICON;
    let mark = ui.visuals().window_fill;
    let stroke = egui::Stroke::new(1.8, mark);
    match severity {
        Severity::Warning => {
            let top = egui::pos2(centre.x, centre.y - side * 0.5);
            let left = egui::pos2(centre.x - side * 0.55, centre.y + side * 0.45);
            let right = egui::pos2(centre.x + side * 0.55, centre.y + side * 0.45);
            painter.add(egui::Shape::convex_polygon(
                vec![top, right, left],
                theme::NEUTRAL,
                egui::Stroke::new(1.5, theme::NEUTRAL),
            ));
            painter.line_segment(
                [
                    egui::pos2(centre.x, centre.y - side * 0.17),
                    egui::pos2(centre.x, centre.y + side * 0.13),
                ],
                stroke,
            );
            painter.circle_filled(egui::pos2(centre.x, centre.y + side * 0.29), 1.1, mark);
        }
        Severity::Info => {
            painter.circle_filled(centre, side * 0.5, theme::mark(ui.visuals()));
            painter.line_segment(
                [
                    egui::pos2(centre.x, centre.y - side * 0.07),
                    egui::pos2(centre.x, centre.y + side * 0.27),
                ],
                stroke,
            );
            painter.circle_filled(egui::pos2(centre.x, centre.y - side * 0.25), 1.2, mark);
        }
    }
}

/// `path` as shown, `~` for the home folder.
pub(crate) fn home_shortened(path: &Path) -> String {
    shortened(path, std::env::home_dir().as_deref())
}

fn shortened(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}

/// `text` cut short in its middle, where "…" goes, to fit `width`: the
/// end of a path names the file, its start where it is.
fn elided(ui: &egui::Ui, text: &str, font: &egui::FontId, width: f32) -> String {
    let fits = |text: &str| {
        ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE).size().x <= width
    };
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let cut = |kept: usize| {
        let head = kept / 2;
        let tail = kept - head;
        let start: String = chars[..head].iter().collect();
        let end: String = chars[chars.len() - tail..].iter().collect();
        format!("{start}…{end}")
    };
    // the most characters that still fit
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        if fits(&cut(middle)) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    cut(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The GUI drawn a sixtieth of a second apart, as on a screen.
    fn stepped() -> Harness<'static, App> {
        Harness::builder().with_size([1280.0, 820.0]).with_step_dt(1.0 / 60.0).build_eframe(|_| {
            App::with(Config::default(), None, tgradish_core::presets::Presets::builtin())
        })
    }

    /// The banner stands over the bar of actions, on one line or two, as
    /// what the bar says wraps it in a narrow window.
    #[test]
    fn stands_over_the_bar_of_actions_however_tall() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("holiday at the seaside with the whole family.png");
        crate::testing::square(&art, [200, 80, 0, 255]);
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let size = [crate::MIN_WIDTH, crate::MIN_HEIGHT];
        let mut harness = Harness::builder().with_size(size).build_eframe(move |cc| {
            let app = App::with(config.clone(), None, tgradish_core::presets::Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
        harness.state_mut().add(vec![art]);
        crate::testing::finish(&mut harness);
        harness.state_mut().say(Message::warning("Stays"));
        let placed = |harness: &mut Harness<App>| {
            harness.run_steps(4);
            let banner = harness.ctx.memory(|memory| memory.area_rect(egui::Id::new("banner")));
            let keys = crate::keys::keys_text(crate::keys::Command::Convert);
            let convert = harness.get(egui_kittest::kittest::by().label_contains(&keys)).rect();
            let bar = egui::PanelState::load(&harness.ctx, egui::Id::new("actions"));
            (banner.expect("drawn"), convert, bar.expect("drawn").outer_rect.top())
        };
        let near = |a: f32, b: f32| assert!((a - b).abs() < 0.5, "{a} against {b}");
        // on one line, where it always stood
        let (banner, convert, bar) = placed(&mut harness);
        near(banner.bottom(), crate::MIN_HEIGHT - OVER_NO_BAR);
        assert!(banner.bottom() < convert.top(), "{banner:?} over {convert:?}");
        // on two, as what keeps it from being converted is said beside
        // Convert, the name of the result under them: as far over it
        harness.state_mut().items[0].choices.crop =
            Some(tgradish_core::options::Crop { x: 100, y: 100, width: 50, height: 50 });
        let (taller, higher, top) = placed(&mut harness);
        assert!(top < bar - 20.0, "{top} under {bar}");
        near(higher.top(), convert.top() - (bar - top));
        near(taller.bottom(), banner.bottom() - (bar - top));
        assert!(taller.bottom() < higher.top(), "{taller:?} over {higher:?}");
        // and where there is no bar, as over one on one line
        harness.state_mut().selected = None;
        harness.run_steps(4);
        let banner = harness.ctx.memory(|memory| memory.area_rect(egui::Id::new("banner")));
        near(banner.expect("drawn").bottom(), crate::MIN_HEIGHT - OVER_NO_BAR);
    }

    #[test]
    fn fades_what_goes_out_as_its_room_closes_up() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        crate::testing::square(&art, [200, 80, 0, 255]);
        let mut harness = stepped();
        harness.state_mut().add(vec![art]);
        for headline in ["Stays", "Goes"] {
            harness.state_mut().say(Message::warning(headline));
        }
        for headline in ["Above", "Below"] {
            harness.state_mut().say(Message::info(headline));
        }
        harness.run_steps(20);
        let top = |harness: &Harness<App>, label: &str| harness.get_by_label(label).rect().top();
        let (stays, above) = (top(&harness, "Stays"), top(&harness, "Above"));
        // the banner's lower row, and the lower notice, dismissed: what is
        // above them moves down into their room from where it was, as
        // they fade out, both laid out from below
        let ids: Vec<u64> = harness.state().banner.rows.iter().map(|said| said.id).collect();
        let (goes, below) = (ids[1], ids[3]);
        harness.state_mut().banner.take_out(|said| said.id == goes || said.id == below, None);
        harness.step();
        let shift =
            |harness: &Harness<App>| (top(harness, "Stays") - stays, top(harness, "Above") - above);
        let (banner, corner) = shift(&harness);
        assert!(banner < 8.0 && corner < 8.0, "{banner}, {corner}");
        assert_eq!(harness.state().banner.leaving.len(), 2);
        harness.run_steps(4);
        let (banner, corner) = shift(&harness);
        assert!(banner > 8.0 && corner > 8.0, "{banner}, {corner}");
        harness.run_steps(8);
        let (end_banner, end_corner) = shift(&harness);
        assert!(end_banner > banner && end_corner > corner);
        assert!(harness.state().banner.leaving.is_empty());
        // where it ends, as if they had never been
        let rows: Vec<_> =
            harness.state().banner.rows.iter().map(|said| &said.message.headline).collect();
        assert_eq!(rows, ["Stays", "Above"]);
        harness.run_steps(2);
        assert_eq!(shift(&harness), (end_banner, end_corner));
    }

    #[test]
    fn says_the_same_once() {
        let mut banner = Banner::default();
        let gone = Message::info("1 file is gone");
        for _ in 0..3 {
            banner.say(gone.clone());
        }
        banner.say(Message::warning("Something else"));
        assert_eq!(banner.rows.len(), 2);
        assert_eq!(banner.rows[0].count, 3);
        // about a subject, once however often, and in the place of the old
        let about = |detail: &str| {
            Message::warning("Unreadable").literal(detail).about(Subject::SettingsFile)
        };
        banner.say(about("line 2"));
        banner.say(about("line 2"));
        assert_eq!(banner.rows[2].count, 1);
        banner.say(about("line 3\n"));
        assert_eq!(banner.rows.len(), 3);
        assert_eq!(banner.rows[2].message.literal.as_deref(), Some("line 3"));
        banner.forget(Subject::SettingsFile);
        assert_eq!(banner.rows.len(), 2);
    }

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use crate::app::App;
    use crate::config::Config;

    /// The GUI with its settings kept in `dir`, in a file of an older
    /// version that can't be read; and that file.
    fn unreadable_settings(dir: &Path) -> (Harness<'static, App>, PathBuf) {
        let path = dir.join("config.toml");
        std::fs::write(&path, OLD).unwrap();
        let mut harness = crate::testing::harness(Config::default());
        harness.state_mut().config_path = Some(path.clone());
        (harness, path)
    }

    const OLD: &str = "[ffmpeg]\nuse = \"auto\"\n";

    fn click(harness: &mut Harness<App>, label: &str) {
        harness.get_by_label(label).click();
        harness.run_steps(2);
    }

    #[test]
    fn shows_what_is_said_again_once() {
        let mut harness = crate::testing::harness(Config::default());
        for _ in 0..2 {
            harness.state_mut().say(Message::warning("Nothing to paste"));
        }
        harness.run_steps(2);
        assert_eq!(harness.get_all_by_label_contains("Nothing to paste").count(), 1);
        harness.get_by_label_contains(&fl!("said-times", count = 2));
    }

    #[test]
    fn dismisses_one_message_of_several() {
        let mut harness = crate::testing::harness(Config::default());
        for headline in ["First", "Second", "Third"] {
            harness.state_mut().say(Message::warning(headline));
        }
        harness.run_steps(2);
        harness.get_all_by_label(&fl!("dismiss")).nth(1).unwrap().click();
        harness.run_steps(2);
        let left: Vec<_> =
            harness.state().banner.rows.iter().map(|said| &said.message.headline).collect();
        assert_eq!(left, ["First", "Third"]);
        // two left: no more dismissing them all at once, nor with what
        // passes in the corner, which goes by itself
        assert!(harness.query_by_label(&fl!("dismiss", "all")).is_none());
        harness.state_mut().say(Message::info("Passing"));
        harness.run_steps(2);
        assert!(harness.query_by_label(&fl!("dismiss", "all")).is_none());
        harness.state_mut().say(Message::warning("Fourth"));
        harness.run_steps(2);
        click(&mut harness, &fl!("dismiss", "all"));
        let left: Vec<_> =
            harness.state().banner.rows.iter().map(|said| &said.message.headline).collect();
        assert_eq!(left, ["Passing"]);
    }

    #[test]
    fn says_what_passes_in_the_corner_over_the_list() {
        use egui::accesskit::{Live, Role};
        use egui_kittest::kittest::{NodeT, by};
        let dir = tempfile::tempdir().unwrap();
        let mut harness = crate::testing::harness(Config::default());
        let art = dir.path().join("art.png");
        crate::testing::square(&art, [200, 80, 0, 255]);
        harness.state_mut().add(vec![art]);
        harness.state_mut().say(Message::warning("Stays"));
        for headline in ["First", "Second", "Third", "Fourth"] {
            harness.state_mut().say(Message::info(headline));
        }
        harness.run_steps(2);
        // the oldest gone for the newest
        let rows = &harness.state().banner.rows;
        let left: Vec<_> = rows.iter().map(|said| &said.message.headline).collect();
        assert_eq!(left, ["Stays", "Second", "Third", "Fourth"]);
        // in the corner over the list, over its footer, the newest at the
        // bottom; the banner right of the list
        let corner = harness.state().banner.corner.expect("drawn");
        let place = |label: &str| harness.get_by_label(label).rect();
        for label in ["Second", "Third", "Fourth"] {
            assert!(corner.contains_rect(place(label)), "{label}: {:?}", place(label));
        }
        assert!(place("Second").bottom() < place("Third").top());
        assert!(place("Third").bottom() < place("Fourth").top());
        let remove_all = place(&fl!("remove-all"));
        assert!(place("Fourth").bottom() < remove_all.top());
        assert!(place("Stays").left() > corner.right());
        // each a status, where screen readers say what comes
        let statuses: Vec<_> = harness.query_all(by().role(Role::Status)).collect();
        assert_eq!(statuses.len(), 3);
        assert!(statuses.iter().all(|node| node.accesskit_node().live() == Live::Polite));
        assert_eq!(harness.get_by_label("Fourth").accesskit_node().live(), Live::Polite);
        assert_eq!(harness.get_by_label("Stays").accesskit_node().live(), Live::Off);
    }

    #[test]
    fn holds_what_passes_while_it_is_hovered() {
        let mut harness = crate::testing::harness(Config::default());
        harness.state_mut().say(Message::info("Passing"));
        harness.run_steps(2);
        let at = harness.get_by_label("Passing").rect().center();
        harness.event(egui::Event::PointerMoved(at));
        // four steps a second
        harness.run_steps(4 * (SHOWN as usize + 2));
        assert_eq!(harness.state().banner.rows.len(), 1);
        harness.event(egui::Event::PointerGone);
        harness.run_steps(4 * (SHOWN as usize - 2));
        assert_eq!(harness.state().banner.rows.len(), 1);
        harness.run_steps(4 * 3);
        assert!(harness.state().banner.rows.is_empty());
    }

    /// Time spent hovering counts as held however long nothing is drawn
    /// meanwhile, as when the pointer rests: let go of
    /// after that, a notice has as long left as when it was hovered.
    #[test]
    fn holds_what_passes_while_hovered_however_long_undrawn() {
        let mut harness = crate::testing::harness(Config::default());
        let step_to = |harness: &mut Harness<App>, at: f64| {
            harness.input_mut().time = Some(at);
            harness.step();
        };
        harness.state_mut().say(Message::info("Passing"));
        harness.run_steps(2);
        let shown = harness.state().banner.rows[0].since.expect("drawn");
        // two seconds left, and then hovered
        step_to(&mut harness, shown + SHOWN - 2.0);
        let at = harness.get_by_label("Passing").rect().center();
        harness.event(egui::Event::PointerMoved(at));
        step_to(&mut harness, shown + SHOWN - 2.0);
        // resting there five seconds, undrawn, and then moved off it
        harness.event(egui::Event::PointerMoved(egui::pos2(1.0, 1.0)));
        step_to(&mut harness, shown + SHOWN + 3.0);
        assert_eq!(harness.state().banner.rows.len(), 1);
        // with about two seconds left
        step_to(&mut harness, shown + SHOWN + 4.5);
        assert_eq!(harness.state().banner.rows.len(), 1);
        step_to(&mut harness, shown + SHOWN + 5.5);
        assert!(harness.state().banner.rows.is_empty());
    }

    #[test]
    fn says_once_that_settings_it_cannot_read_are_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, path) = unreadable_settings(dir.path());
        for _ in 0..3 {
            harness.state_mut().save_setting(|gui| gui.overwrite = !gui.overwrite);
        }
        let rows = &harness.state().banner.rows;
        assert_eq!(rows.len(), 1);
        let (said, message) = (&rows[0], &rows[0].message);
        assert_eq!((said.count, message.severity), (1, Severity::Warning));
        assert_eq!(message.file.as_deref(), Some(path.as_path()));
        // core's message, without the file it names, or its last line break
        let detail = message.literal.as_deref().unwrap();
        assert!(detail.starts_with("TOML parse error at line 2"), "{detail}");
        assert!(detail.ends_with("expected `from` or `path`"), "{detail}");
        assert!(!message.headline.contains("config.toml"));
        assert_eq!(message.actions[0], Action::FreshSettings);
        // dismissed, it is said again at the next change
        harness.run_steps(2);
        click(&mut harness, &fl!("dismiss"));
        assert!(harness.state().banner.rows.is_empty());
        harness.state_mut().save_setting(|gui| gui.overwrite = true);
        assert_eq!(harness.state().banner.rows.len(), 1);
    }

    #[test]
    fn folds_long_details_away() {
        let mut harness = crate::testing::harness(Config::default());
        let detail = "TOML parse error\n  |\n2 | use\n  | ^^^\nunknown field `use`";
        harness.state_mut().say(Message::warning("Unreadable").literal(detail));
        harness.state_mut().say(Message::warning("Short").literal("Permission denied"));
        harness.run_steps(2);
        // the short shown, the long only once asked for
        harness.get_by_label("Permission denied");
        assert!(harness.query_by_label_contains("unknown field").is_none());
        click(&mut harness, &fl!("details"));
        harness.get_by_label_contains("unknown field");
        click(&mut harness, &fl!("details"));
        assert!(harness.query_by_label_contains("unknown field").is_none());
    }

    /// Why, in tgradish's words, is a line of its own; what the system or
    /// a program said is apart, as it said it.
    #[test]
    fn says_why_apart_from_what_was_said() {
        use tgradish_core::{ClipboardError, Error};
        let empty = crate::app::not_pasted(&Error::Clipboard(ClipboardError::Empty));
        assert_eq!(empty.headline, fl!("nothing-to-paste"));
        assert_eq!(empty.reason, Some(fl!("nothing-to-paste", "why")));
        assert_eq!(empty.literal, None);
        let said = ClipboardError::Unreadable("Unknown error while interacting".into());
        let unreadable = crate::app::not_pasted(&Error::Clipboard(said));
        assert_eq!(unreadable.reason, None);
        assert_eq!(unreadable.literal.as_deref(), Some("Unknown error while interacting"));
        let denied = std::io::Error::from_raw_os_error(13);
        let unsaved = crate::app::session_unsaved(Path::new("session.json"), &denied);
        assert_eq!(unsaved.literal, Some(denied.to_string()));
        let failed = Error::Ffmpeg {
            program: "ffmpeg",
            status: "exit status: 1".into(),
            stderr: "Invalid data found\n".into(),
        };
        let failed = crate::app::told(Message::warning("Failed"), &failed);
        assert!(failed.reason.as_deref().is_some_and(|reason| reason.starts_with("ffmpeg failed")));
        assert_eq!(failed.literal.as_deref(), Some("Invalid data found"));
        let mut harness = crate::testing::harness(Config::default());
        harness.state_mut().say(empty);
        harness.run_steps(2);
        harness.get_by_label(&fl!("nothing-to-paste", "why"));
    }

    #[test]
    fn starts_with_fresh_settings_keeping_the_old_beside_them() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, path) = unreadable_settings(dir.path());
        // a copy kept earlier, of something else, which stays
        let earlier = dir.path().join("config.toml.unreadable");
        std::fs::write(&earlier, "earlier = 1\n").unwrap();
        harness.state_mut().save_setting(|gui| gui.overwrite = true);
        harness.run_steps(2);
        click(&mut harness, &fl!("fresh-settings"));
        // the change that wasn't saved is now
        assert!(Config::load(&path).unwrap().gui.overwrite);
        let kept = dir.path().join("config.toml.unreadable-2");
        assert_eq!(std::fs::read_to_string(&kept).unwrap(), OLD);
        assert_eq!(std::fs::read_to_string(&earlier).unwrap(), "earlier = 1\n");
        // and said, with where the old file went
        let rows = &harness.state().banner.rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message.headline, fl!("fresh-settings", "saved"));
        assert_eq!(rows[0].message.file.as_deref(), Some(kept.as_path()));
    }

    #[test]
    fn says_why_fresh_settings_could_not_be_written() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, path) = unreadable_settings(dir.path());
        // where the copy would go, a folder, which nothing is written over
        std::fs::create_dir(dir.path().join("config.toml.unreadable")).unwrap();
        harness.state_mut().save_setting(|gui| gui.overwrite = true);
        harness.run_steps(2);
        click(&mut harness, &fl!("fresh-settings"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), OLD);
        let headlines: Vec<_> =
            harness.state().banner.rows.iter().map(|said| said.message.headline.clone()).collect();
        assert_eq!(
            headlines,
            [fl!("setting-unsaved", "unreadable"), fl!("fresh-settings", "failed")]
        );
        // about the folder in the copy's way, as the system said
        let failed = &harness.state().banner.rows[1].message;
        assert_eq!(
            failed.file.as_deref(),
            Some(dir.path().join("config.toml.unreadable").as_path())
        );
        assert!(failed.reason.is_none() && failed.literal.is_some());
    }

    #[test]
    fn takes_up_settings_once_they_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, path) = unreadable_settings(dir.path());
        harness.state_mut().save_setting(|gui| gui.overwrite = true);
        harness.run_steps(2);
        // still as it was: said so, and the message stays
        click(&mut harness, &fl!("read-again"));
        harness.get_by_label_contains(&fl!("read-again", "failed"));
        assert_eq!(harness.state().banner.rows[0].message.subject, Some(Subject::SettingsFile));
        // put right by hand, what it says is taken up
        std::fs::write(&path, "[gui]\nreopen-files = false\n").unwrap();
        click(&mut harness, &fl!("read-again"));
        let app = harness.state();
        assert!(!app.config.gui.reopen_files && !app.config.gui.overwrite);
        let subjects: Vec<_> = app.banner.rows.iter().map(|said| said.message.subject).collect();
        assert_eq!(subjects, [None]);
        // and changes are saved again
        harness.state_mut().save_setting(|gui| gui.overwrite = true);
        assert!(Config::load(&path).unwrap().gui.overwrite);
    }

    #[test]
    fn copies_the_whole_path() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, path) = unreadable_settings(dir.path());
        harness.state_mut().save_setting(|gui| gui.overwrite = true);
        harness.run_steps(2);
        harness.get_by_label(&fl!("copy-path")).click();
        harness.step();
        let copied: Vec<_> = harness
            .output()
            .platform_output
            .commands
            .iter()
            .filter_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(copied, [path.display().to_string()]);
        harness.run_steps(2);
        harness.get_by_label_contains(&fl!("copy-path", "done"));
        // relative, as given on the command line, made whole
        let relative = Path::new("config.toml");
        assert_eq!(
            copied_path(relative),
            std::env::current_dir().unwrap().join(relative).display().to_string()
        );
    }

    #[test]
    fn shortens_paths_from_home() {
        let home = Path::new("/home/someone");
        let file = home.join(".config").join("tgradish").join("config.toml");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            shortened(&file, Some(home)),
            format!(
                "~{sep}{}",
                Path::new(".config").join("tgradish").join("config.toml").display()
            )
        );
        assert_eq!(shortened(Path::new("/etc/tgradish"), Some(home)), "/etc/tgradish");
        assert_eq!(shortened(home, Some(home)), "~");
        assert_eq!(shortened(&file, None), file.display().to_string());
    }
}
