//! The banner over the bottom of the window, where it says what happened
//! that wasn't asked for, as a file that couldn't be read or saved. Each
//! message has its row, with what happened first, the file it is about,
//! what to do and buttons that do it, and what went wrong as the program
//! said it, folded away when it is long.

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
}

/// A button in a message's row, which the window does when it is clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Writes the settings the window has in place of the settings file,
    /// which can't be read, keeping that beside it.
    FreshSettings,
    /// Reads the settings file again, as after it was put right by hand.
    ReadSettingsAgain,
    /// Reads the presets again.
    ReadPresetsAgain,
    /// Copies the message's file's whole path.
    CopyPath,
    /// Opens the folder of the message's file, the file selected.
    ShowInFolder,
}

impl Action {
    fn label(self) -> String {
        match self {
            Action::FreshSettings => fl!("fresh-settings"),
            Action::ReadSettingsAgain | Action::ReadPresetsAgain => fl!("read-again"),
            Action::CopyPath => fl!("copy-path"),
            Action::ShowInFolder => fl!("folder-of"),
        }
    }

    fn hint(self) -> String {
        match self {
            Action::FreshSettings => fl!("fresh-settings", "hint"),
            Action::ReadSettingsAgain | Action::ReadPresetsAgain => fl!("read-again", "hint"),
            Action::CopyPath => fl!("copy-path", "hint"),
            Action::ShowInFolder => fl!("folder-of", "hint"),
        }
    }

    /// The way out most people want, a button among links.
    fn primary(self) -> bool {
        self == Action::FreshSettings
    }
}

/// Something the window says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) severity: Severity,
    /// What happened, in a short sentence of the window's own.
    pub(crate) headline: String,
    /// The file it is about.
    pub(crate) file: Option<PathBuf>,
    /// What it means or what can be done, in a line of its own.
    pub(crate) note: Option<String>,
    /// What went wrong as the program said it, shown in its own lines and
    /// columns.
    pub(crate) detail: Option<String>,
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
            note: None,
            detail: None,
            actions: Vec::new(),
            subject: None,
        }
    }

    pub(crate) fn file(self, file: impl Into<PathBuf>) -> Message {
        Message { file: Some(file.into()), ..self }
    }

    pub(crate) fn note(self, note: impl Into<String>) -> Message {
        Message { note: Some(note.into()), ..self }
    }

    /// With `detail`, the blank lines and spaces at its end left out.
    pub(crate) fn detail(self, detail: impl AsRef<str>) -> Message {
        let detail = detail.as_ref().trim_end();
        Message { detail: (!detail.is_empty()).then(|| detail.to_owned()), ..self }
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
        let parts = [&self.note, &self.detail].into_iter().flatten().cloned();
        std::iter::once(self.headline.clone()).chain(file).chain(parts)
    }
}

/// A message shown, with how it is shown.
pub(crate) struct Said {
    /// Tells its row's state apart in egui's memory.
    id: u64,
    pub(crate) message: Message,
    /// How many times it was said while it showed.
    pub(crate) count: u32,
    /// When it was first drawn, or last hovered: information goes after a
    /// while.
    since: Option<f64>,
    pub(crate) details_open: bool,
    /// What an action said of itself, as "Copied", and until when.
    pub(crate) flash: Option<(String, f64)>,
}

/// The messages the window shows, oldest first.
#[derive(Default)]
pub(crate) struct Banner {
    pub(crate) rows: Vec<Said>,
    next_id: u64,
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
/// The icon's side, and the room between it, the text and the button.
const ICON: f32 = 16.0;
const GAP: f32 = 10.0;
/// The dismiss button's width.
const DISMISS: f32 = 22.0;
/// The tallest the details get before they scroll.
const DETAIL_HEIGHT: f32 = 160.0;

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
        });
    }

    /// Stops showing what was said about `subject`.
    pub(crate) fn forget(&mut self, subject: Subject) {
        self.rows.retain(|said| said.message.subject != Some(subject));
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

    /// Draws the banner, if there is anything to say, and takes in its
    /// dismissals. Gives the actions clicked, with the messages they are
    /// of, for the window to do.
    pub(crate) fn show(&mut self, ctx: &egui::Context) -> Vec<(Message, Action)> {
        let mut clicked = Vec::new();
        if self.rows.is_empty() {
            return clicked;
        }
        let now = ctx.input(|input| input.time);
        let widest = WIDEST.min(ctx.content_rect().width() - 2.0 * SIDE_ROOM).max(200.0);
        let mut dismissed = Vec::new();
        let mut all = false;
        let area = egui::Area::new(egui::Id::new("banner"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -56.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(12)).show(
                    ui,
                    |ui| {
                        let room = widest - 24.0 - ICON - 2.0 * GAP - DISMISS;
                        let column =
                            self.rows.iter().map(|said| needs(ui, said, room)).fold(0.0, f32::max);
                        let column = column.clamp(160.0_f32.min(room), room);
                        ui.set_width(ICON + 2.0 * GAP + column + DISMISS);
                        for (index, said) in self.rows.iter_mut().enumerate() {
                            if index > 0 {
                                hairline(ui);
                            }
                            let row = draw(ui, said, column, now);
                            clicked
                                .extend(row.actions.into_iter().map(|a| (said.message.clone(), a)));
                            if row.dismissed {
                                dismissed.push(said.id);
                            }
                        }
                        if self.rows.len() > 2 {
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
        // hovering keeps information showing
        let hovered = area.response.contains_pointer();
        for said in &mut self.rows {
            match said.since {
                _ if hovered => said.since = Some(now),
                None => said.since = Some(now),
                Some(since) if said.message.severity == Severity::Info && now - since > SHOWN => {
                    dismissed.push(said.id);
                }
                Some(_) => {}
            }
            if said.flash.as_ref().is_some_and(|(_, until)| now > *until) {
                said.flash = None;
            }
        }
        if all {
            self.rows.clear();
        }
        self.rows.retain(|said| !dismissed.contains(&said.id));
        let waiting = self
            .rows
            .iter()
            .any(|said| said.message.severity == Severity::Info || said.flash.is_some());
        if waiting {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        clicked
    }
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
                    // taken up as when the window opens
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
            Action::ReadPresetsAgain => {
                if !self.reload_presets() {
                    self.banner.flash(message, fl!("read-again", "failed"), now);
                }
            }
        }
    }

    /// Writes the settings the window has in place of the settings file,
    /// which can't be read, keeping that beside it, as Settings saves.
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
            Err(error) => {
                let message = Message::warning(fl!("fresh-settings", "failed"));
                self.say(message.file(path).detail(error));
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
    if let Some(note) = &message.note {
        let job = egui::text::LayoutJob::simple(note.clone(), body.clone(), Color32::WHITE, room);
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
    if message.detail.as_deref().is_some_and(|detail| !short(detail)) {
        buttons += painter.layout_no_wrap(fl!("details"), body, Color32::WHITE).size().x + 20.0;
    }
    widest = widest.max(buttons);
    let open = said.details_open || message.detail.as_deref().is_some_and(short);
    if let Some(detail) = message.detail.as_deref().filter(|_| open) {
        let block = painter.layout_job(detail_job(ui, detail, f32::INFINITY)).size().x;
        widest = widest.max(block + DETAIL_MARGIN.sum().x + ui.spacing().scroll.bar_width);
    }
    widest.min(room)
}

fn draw(ui: &mut egui::Ui, said: &mut Said, column: f32, now: f64) -> Row {
    let mut row = Row { actions: Vec::new(), dismissed: false };
    let line = ui.text_style_height(&TextStyle::Body);
    let top_left = ui.cursor().min;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (icon, _) = ui.allocate_exact_size(vec2(ICON, line), Sense::hover());
        paint_icon(ui, said.message.severity, icon);
        ui.add_space(GAP);
        ui.vertical(|ui| {
            ui.set_width(column);
            ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
            text(ui, said, column, now, &mut row);
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
    row.dismissed = response.on_hover_text(dismiss).clicked();
    row
}

/// The text column of a row: headline, file, note, buttons and details.
fn text(ui: &mut egui::Ui, said: &mut Said, column: f32, now: f64, row: &mut Row) {
    let message = &said.message;
    ui.add(egui::Label::new(headline(ui, said, column)).wrap());
    if let Some(file) = &message.file {
        let body = TextStyle::Body.resolve(ui.style());
        let shown = elided(ui, &home_shortened(file), &body, column);
        let label = egui::RichText::new(shown).color(ui.visuals().weak_text_color());
        ui.add(egui::Label::new(label).extend()).on_hover_text(file.display().to_string());
    }
    if let Some(note) = &message.note {
        ui.add(egui::Label::new(note.as_str()).wrap());
    }
    let long = message.detail.as_deref().is_some_and(|detail| !short(detail));
    if !message.actions.is_empty() || long {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(12.0, 6.0);
            for &action in &message.actions {
                let label = action.label();
                let response = if action.primary() {
                    let button = egui::Button::new(label).stroke(theme::on_outline(ui.visuals()));
                    ui.add(button)
                } else {
                    link_button(ui, label)
                };
                if response.on_hover_text(action.hint()).clicked() {
                    row.actions.push(action);
                }
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
    if let Some(detail) = message.detail.as_deref().filter(|_| !long || said.details_open) {
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
/// a page opened: a button to whatever reads the window out.
fn link_button(ui: &mut egui::Ui, text: String) -> egui::Response {
    let colour = ui.visuals().hyperlink_color;
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = 0.0;
        let response =
            ui.add(egui::Button::new(egui::RichText::new(text).color(colour)).frame(false));
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
            painter.circle_filled(centre, side * 0.5, theme::accent(ui.visuals()));
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
            Message::warning("Unreadable").detail(detail).about(Subject::SettingsFile)
        };
        banner.say(about("line 2"));
        banner.say(about("line 2"));
        assert_eq!(banner.rows[2].count, 1);
        banner.say(about("line 3\n"));
        assert_eq!(banner.rows.len(), 3);
        assert_eq!(banner.rows[2].message.detail.as_deref(), Some("line 3"));
        banner.forget(Subject::SettingsFile);
        assert_eq!(banner.rows.len(), 2);
    }

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use crate::app::App;
    use crate::config::Config;

    /// A window whose settings are kept in `dir`, in a file of an older
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
            harness.state_mut().say(Message::warning("Nothing could be pasted"));
        }
        harness.run_steps(2);
        assert_eq!(harness.get_all_by_label_contains("Nothing could be pasted").count(), 1);
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
        // two left: no more dismissing them all at once
        assert!(harness.query_by_label(&fl!("dismiss", "all")).is_none());
        harness.state_mut().say(Message::info("Fourth"));
        harness.run_steps(2);
        click(&mut harness, &fl!("dismiss", "all"));
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
        let detail = message.detail.as_deref().unwrap();
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
        harness.state_mut().say(Message::warning("Unreadable").detail(detail));
        harness.state_mut().say(Message::warning("Short").detail("Permission denied"));
        harness.run_steps(2);
        // the short shown, the long only once asked for
        harness.get_by_label("Permission denied");
        assert!(harness.query_by_label_contains("unknown field").is_none());
        click(&mut harness, &fl!("details"));
        harness.get_by_label_contains("unknown field");
        click(&mut harness, &fl!("details"));
        assert!(harness.query_by_label_contains("unknown field").is_none());
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
