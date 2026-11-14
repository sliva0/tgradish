//! The list of files on the left: a row for each, with its input, its
//! result and how converting it went, under the buttons that add files,
//! switch the theme, and open the menu of Settings and the rest.

use std::path::PathBuf;

use eframe::egui::{self, Color32, FontId, RichText, vec2};
use tgradish_core::presets::Format;
use tgradish_core::{input, telegram};

use crate::app::{App, set_theme};
use crate::fonts::{drawable, missing_note, text_format};
use crate::inspect::Inspection;
use crate::item::{Blocker, Item};
use crate::jobs::Status;
use crate::l10n::fl;
use crate::media::Task;
use crate::{fallback, icon, theme, widgets};

/// Side of the thumbnails in the list.
pub(crate) const THUMB: f32 = 52.0;
/// Points for the app's icon before its name, above the list of files.
const ICON_ROOM: f32 = 48.0;

impl App {
    /// Switches between light and dark, and keeps the choice in the config
    /// file at once, without the rest of what Settings changed.
    fn theme_button(&mut self, ui: &mut egui::Ui) {
        let dark = ui.visuals().dark_mode;
        let (icon, hint, theme) = if dark {
            ("☀", fl!("light-theme"), crate::config::Theme::Light)
        } else {
            ("🌓", fl!("dark-theme"), crate::config::Theme::Dark)
        };
        if !widgets::named(ui.button(icon), &hint).on_hover_text(hint).clicked() {
            return;
        }
        set_theme(ui.ctx(), theme);
        self.save_setting(|gui| gui.theme = theme);
    }

    fn add_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button(fl!("add-folder")).on_hover_text(fl!("add-folder", "hint")).clicked() {
            self.pick(ui.ctx(), Pick::Folder);
            ui.close();
        }
        if ui.button(fl!("add-images")).on_hover_text(fl!("add-images", "hint")).clicked() {
            self.pick(ui.ctx(), Pick::Frames);
            ui.close();
        }
        if ui.button(fl!("add-pasted")).on_hover_text(fl!("add-pasted", "hint")).clicked() {
            self.paste();
            ui.close();
        }
    }

    /// The menu behind the gear over the list: Settings, inspecting a
    /// sticker, the keyboard's shortcuts, and what isn't about the files at
    /// all, feedback and About.
    fn main_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button(fl!("menu-settings")).clicked() {
            self.settings_dialog.open();
            ui.close();
        }
        if ui.button(fl!("inspect")).on_hover_text(fl!("inspect", "hint")).clicked() {
            self.pick(ui.ctx(), Pick::Inspected);
            ui.close();
        }
        let keys = crate::keys::keys_text(crate::keys::Command::Shortcuts);
        if ui.add(egui::Button::new(fl!("shortcuts")).shortcut_text(keys)).clicked() {
            self.shortcuts = true;
            ui.close();
        }
        ui.separator();
        if ui.button(fl!("feedback")).on_hover_text(fl!("feedback", "hint")).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(crate::about::FEEDBACK));
            ui.close();
        }
        if ui.button(fl!("about-and-licences")).clicked() {
            self.about = true;
            ui.close();
        }
    }

    /// Asks for something with one of the system's file dialogs, parented
    /// to the window. It runs on a thread of its own, so the window keeps
    /// drawing and converting meanwhile, and what is picked is taken in as
    /// it draws. One at a time: while one is open, others aren't asked for.
    pub(crate) fn pick(&mut self, ctx: &egui::Context, pick: Pick) {
        if self.picking.is_some() {
            return;
        }
        let dialog = pick.dialog(self.file_dialog.clone());
        self.picking = Some(Task::spawn(ctx, move |_| {
            Ok(match pick {
                Pick::Files => dialog.pick_files().map(Picked::Files),
                Pick::Folder => dialog.pick_folder().map(Picked::Folder),
                Pick::Frames => dialog.pick_files().map(Picked::Frames),
                // read here too, as large files take a moment
                Pick::Inspected => dialog
                    .pick_file()
                    .map(|file| Picked::Inspected(Box::new(Inspection::of(&file)))),
                Pick::Ffmpeg => dialog.pick_file().map(Picked::Ffmpeg),
                Pick::OutputDir => dialog.pick_folder().map(Picked::OutputDir),
            })
        }));
    }

    /// Takes in what a file dialog picked, once it closes.
    pub(crate) fn take_picked(&mut self) {
        let Some(result) = self.picking.as_ref().and_then(Task::poll) else { return };
        self.picking = None;
        match result {
            Ok(Some(Picked::Files(files))) => self.add(files),
            Ok(Some(Picked::Folder(dir))) => self.add(vec![dir]),
            Ok(Some(Picked::Frames(files))) => self.add_frames(files),
            Ok(Some(Picked::Inspected(inspection))) => self.inspection = Some(*inspection),
            // for Settings, which keeps it once saved; else, as a message
            // or a file's settings asked, kept at once, and used over one
            // given as the window was started
            Ok(Some(Picked::Ffmpeg(file))) if self.settings_dialog.is_open() => {
                self.config.ffmpeg.path = Some(file)
            }
            Ok(Some(Picked::Ffmpeg(file))) => {
                self.ffmpeg_override = (None, None);
                self.save_config_setting(|config| config.ffmpeg.path = Some(file.clone()));
            }
            Ok(Some(Picked::OutputDir(dir))) => self.config.gui.output_dir = Some(dir),
            Ok(None) => {}
            Err(message) => self.say(crate::banner::Message::warning(message)),
        }
    }

    pub(crate) fn files_panel(&mut self, ui: &mut egui::Ui) {
        // its buttons level with the bar over the preview beside it
        ui.add_space(4.0);
        let icon_side = icon::side(ui.ctx(), ICON_ROOM);
        // as tall as the icon, with the buttons beside its middle
        let header = vec2(ui.available_width(), icon_side);
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(header, layout, |ui| {
            let menu = ui.menu_button("⚙", |ui| self.main_menu(ui)).response;
            let name = fl!("main-menu");
            widgets::named(menu, &name).on_hover_text(name);
            self.theme_button(ui);
            // a split button: files, or the rest from its arrow
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                let more = ui.menu_button("⏷", |ui| self.add_menu(ui)).response;
                widgets::named(more, &fl!("add-more"));
                if ui.button(fl!("add-files")).clicked() {
                    self.pick(ui.ctx(), Pick::Files);
                }
            });
            // the icon and name, where the buttons leave room for them
            let title = egui::WidgetText::from(RichText::new("tgradish").heading().strong());
            let galley = title.into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Heading,
            );
            let gap = ui.spacing().item_spacing.x;
            let room = ui.available_width() - gap;
            if icon_side <= room {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    icon::show(ui, ICON_ROOM);
                    if icon_side + gap + galley.size().x <= room {
                        ui.label(galley);
                    }
                });
            }
        });
        ui.add_space(4.0);
        ui.separator();

        // what Convert all converts, and what it leaves out
        let convertible =
            |item: &Item| !item.is_busy() && !item.result_is_current() && !item.blocked();
        let changed = self.items.iter().filter(|item| convertible(item)).count();
        let attention = self.items.iter().filter(|item| !item.is_busy() && item.blocked()).count();
        // without files there is nothing to convert or remove, and the way
        // to add them, beside the list, is the one thing to do
        if !self.items.is_empty() {
            let mut stop = false;
            egui::Panel::bottom("files-footer").show(ui, |ui| {
                // level with the bar under the preview beside it: its line,
                // and the middle of its buttons, which are taller; that
                // bar's panel has more room under it than this one's
                let (above, below) = footer_spaces(ui.style());
                ui.add_space(above);
                // converting: how far it got of all asked for, and a way to
                // stop it all, each where it stays as the numbers change
                if let Some((current, total)) = self.batch_progress() {
                    ui.horizontal(|ui| {
                        let text = fl!("converting-batch", current = current, total = total);
                        let widest = fl!("converting-batch", current = total, total = total);
                        let width = widgets::text_width(ui, &widgets::zeros(&widest), 0.0);
                        let row = vec2(width.ceil() + 1.0, crate::theme::CONTROL);
                        let layout = egui::Layout::left_to_right(egui::Align::Center);
                        ui.allocate_ui_with_layout(row, layout, |ui| ui.label(text));
                        let hint = fl!("stop-all", "hint");
                        stop = ui.button(fl!("stop-all")).on_hover_text(hint).clicked();
                    });
                    ui.add_space(2.0);
                }
                // onto a second line where longer words leave no room
                ui.horizontal_wrapped(|ui| {
                    // as wide for any count, so what follows stays put
                    let widest = fl!("convert-all", count = self.items.len());
                    let padding = 2.0 * ui.spacing().button_padding.x;
                    let width = widgets::text_width(ui, &widgets::zeros(&widest), padding);
                    let all = egui::Button::new(fl!("convert-all", count = changed))
                        .min_size(vec2(width.ceil(), 0.0));
                    if ui
                        .add_enabled(changed > 0, all)
                        .on_hover_text(fl!("convert-all", "hint"))
                        .clicked()
                    {
                        let ids: Vec<u64> = self
                            .items
                            .iter()
                            .filter(|item| convertible(item))
                            .map(|item| item.id)
                            .collect();
                        for id in ids {
                            self.enqueue(id);
                        }
                    }
                    if ui.button(fl!("remove-all")).clicked() {
                        let ids: Vec<u64> = self.items.iter().map(|item| item.id).collect();
                        self.remove_all(&ids);
                    }
                    if attention > 0 {
                        // whole, on a line of its own where it doesn't fit
                        let text = format!("⚠ {}", fl!("need-attention", count = attention));
                        let text = RichText::new(text).color(theme::warn(ui.visuals()));
                        let label = egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend);
                        ui.add(label).on_hover_text(fl!("row-attention", "hint"));
                    }
                });
                ui.add_space(below);
            });
            if stop {
                self.stop_all();
            }
        }

        // under the bar and over the footer: what passes is said in its
        // corner, mostly empty
        self.banner.corner = Some(ui.available_rect_before_wrap());
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.items.is_empty() {
                ui.vertical_centered(|ui| {
                    let line = ui.text_style_height(&egui::TextStyle::Body);
                    ui.add_space(((ui.available_height() - line) / 2.0).max(20.0));
                    widgets::note(ui, fl!("no-files"));
                });
            }
            let mut clicked = None;
            let mut removed = None;
            let mut again = None;
            // Delete or Backspace on the row with the keyboard's focus,
            // which a text field with the focus would take instead
            use crate::keys::{Command, pressed};
            let delete = ui.input(|input| pressed(input, Command::Remove).is_some());
            let mut focus_next = None;
            for (index, item) in self.items.iter().enumerate() {
                let selected = self.selected == Some(item.id);
                let queued = self.queue.iter().position(|&id| id == item.id);
                let (response, action) = row(ui, item, selected, queued, &self.fallbacks);
                match action {
                    Some(RowAction::Remove) => removed = Some(item.id),
                    Some(RowAction::Convert) => again = Some(item.id),
                    None if response.clicked() => clicked = Some(item.id),
                    None if delete && response.has_focus() => {
                        removed = Some(item.id);
                        // the focus on to the next, or the one before at the end
                        let next =
                            self.items.get(index + 1).or(self.items.get(index.wrapping_sub(1)));
                        focus_next = next.map(|next| row_id(ui, next.id));
                    }
                    None => {}
                }
                response.context_menu(|ui| {
                    // the input in its folder; the result's is on the bar
                    // under the preview
                    let input = ui
                        .add_enabled(
                            item.paste_dir.is_none(),
                            egui::Button::new(fl!("show-input-in-folder")),
                        )
                        .on_hover_text(item.inputs[0].display().to_string())
                        .on_disabled_hover_text(fl!("show-input-in-folder", "pasted"));
                    if input.clicked() {
                        crate::system::reveal(&item.inputs[0]);
                        ui.close();
                    }
                    if ui.button(fl!("remove")).clicked() {
                        removed = Some(item.id);
                        ui.close();
                    }
                });
            }
            if let Some(id) = clicked {
                self.select(id);
            }
            if let Some(id) = removed {
                self.remove(id);
            }
            if let Some(id) = focus_next {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            if let Some(id) = again {
                self.enqueue(id);
            }
        });
    }

    /// How far converting got, while it is under way: the one at work, or
    /// the last begun, counted from the first asked for since nothing was
    /// converting, of all asked for since.
    pub(crate) fn batch_progress(&self) -> Option<(usize, usize)> {
        let jobs = self.items.iter().filter_map(|item| item.job.as_ref());
        let (busy, waiting) = jobs.fold((0, 0), |(busy, waiting), conversion| {
            (busy + 1, waiting + usize::from(conversion.job.is_waiting()))
        });
        if busy == 0 {
            return None;
        }
        let total = self.batch.max(busy);
        Some(((total - waiting).clamp(1, total), total))
    }

    pub(crate) fn empty(&mut self, ui: &mut egui::Ui) {
        let zone = ui.max_rect().shrink(28.0);
        let colour = if self.hovering {
            theme::mark(ui.visuals())
        } else {
            ui.visuals().widgets.noninteractive.bg_stroke.color
        };
        let dashes = egui::Shape::dashed_line(
            &[
                zone.left_top(),
                zone.right_top(),
                zone.right_bottom(),
                zone.left_bottom(),
                zone.left_top(),
            ],
            egui::Stroke::new(1.5, colour),
            8.0,
            6.0,
        );
        ui.painter().extend(dashes);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 110.0).max(40.0));
            ui.label(RichText::new(fl!("drop-here")).size(22.0));
            ui.add_space(6.0);
            widgets::note(ui, fl!("drop-here-note"));
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let width = 400.0;
                ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
                let add = egui::Button::new(widgets::medium(ui, fl!("add-files"), 14.0))
                    .min_size(vec2(130.0, 34.0));
                if widgets::primary(ui, true, add).clicked() {
                    self.pick(ui.ctx(), Pick::Files);
                }
                if ui
                    .add(egui::Button::new(fl!("add-folder")).min_size(vec2(150.0, 34.0)))
                    .clicked()
                {
                    self.pick(ui.ctx(), Pick::Folder);
                }
                if ui
                    .add(egui::Button::new(fl!("paste")).min_size(vec2(90.0, 34.0)))
                    .on_hover_text(fl!("paste", "hint"))
                    .clicked()
                {
                    self.paste();
                }
            });
        });
    }
}

/// The space over the list's bar, and under it, which lay its buttons
/// level with those of the bar under the preview beside it.
fn footer_spaces(style: &egui::Style) -> (f32, f32) {
    use crate::actions::{BAR_ABOVE, BAR_BELOW, BAR_ROW};
    let taller = (BAR_ROW - theme::CONTROL) / 2.0;
    let central = egui::Frame::central_panel(style).inner_margin.bottom;
    let side = egui::Frame::side_top_panel(style).inner_margin.bottom;
    (BAR_ABOVE + taller, BAR_BELOW + taller + f32::from(central - side))
}

/// What a file dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pick {
    /// Files to add, each its own.
    Files,
    /// A folder of frames to add.
    Folder,
    /// Images to add as one animation.
    Frames,
    /// A sticker to inspect.
    Inspected,
    /// The ffmpeg program, for Settings.
    Ffmpeg,
    /// The folder for results, for Settings.
    OutputDir,
}

impl Pick {
    /// `dialog`, which has the window as its parent, titled with what it
    /// is for, as the system's dialog may be another program's window,
    /// titled with that program's name otherwise, and showing only the
    /// files it is for where those are known. Made on the window's thread,
    /// where its messages are given.
    fn dialog(self, dialog: rfd::FileDialog) -> rfd::FileDialog {
        let dialog = dialog.set_title(match self {
            Pick::Files => fl!("dialog-add-files"),
            Pick::Folder => fl!("dialog-add-folder"),
            Pick::Frames => fl!("dialog-add-images"),
            Pick::Inspected => fl!("dialog-inspect"),
            Pick::Ffmpeg => fl!("dialog-ffmpeg"),
            Pick::OutputDir => fl!("dialog-results-folder"),
        });
        match self {
            Pick::Frames => {
                dialog.add_filter(fl!("images-filter"), &[input::IMAGES, input::ASEPRITE].concat())
            }
            Pick::Inspected => dialog.add_filter(fl!("stickers-filter"), &["webm", "tgs", "webp"]),
            Pick::Files | Pick::Folder | Pick::Ffmpeg | Pick::OutputDir => dialog,
        }
    }
}

/// What a file dialog picked, for what it asked.
pub(crate) enum Picked {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
    Frames(Vec<PathBuf>),
    Inspected(Box<Inspection>),
    Ffmpeg(PathBuf),
    OutputDir(PathBuf),
}

/// What a row's own buttons ask for.
enum RowAction {
    Remove,
    Convert,
}

/// One file in the list: its input, name and state, and its result, with
/// what its buttons asked for.
fn row(
    ui: &mut egui::Ui,
    item: &Item,
    selected: bool,
    queued: Option<usize>,
    fallbacks: &fallback::Fallbacks,
) -> (egui::Response, Option<RowAction>) {
    let width = ui.available_width();
    // by the file, so the focus stays with it as rows come and go
    let (_, rect) = ui.allocate_space(vec2(width, THUMB + 12.0));
    let response = ui.interact(rect, row_id(ui, item.id), egui::Sense::click());
    // read out as a choice of the list, by its file's name
    let name = item.name();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &name)
    });
    let visuals = ui.visuals();
    // as Breeze marks items: the accent let through, more for the
    // selected one, which is outlined in it too, yet little enough that
    // the row's small coloured text still reads
    if selected {
        ui.painter().rect_filled(rect, 6.0, theme::row(visuals, true));
        let outline = egui::Stroke::new(1.0, theme::mark(visuals));
        ui.painter().rect_stroke(rect, 6.0, outline, egui::StrokeKind::Inside);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, theme::row(visuals, false));
    }
    let inner = rect.shrink2(vec2(6.0, 6.0));
    let input = egui::Rect::from_min_size(inner.min, vec2(THUMB, THUMB));
    // unread for want of ffmpeg, which is said otherwise, isn't broken
    let broken = item.input_error().is_some() && !item.blockers.contains(&Blocker::NeedsFfmpeg);
    thumb(ui, input, item.input_thumb.as_ref(), broken, false);
    let result =
        egui::Rect::from_min_size(egui::pos2(inner.max.x - THUMB, inner.min.y), vec2(THUMB, THUMB));
    let mut action = None;
    if let Some(made) = &item.made {
        let stale = !item.result_is_current();
        thumb(ui, result, item.result_thumb.as_ref(), false, stale);
        // a result with no picture yet, or none to show, as a .tgs made in
        // an earlier run: its format where the picture would be
        if item.result_thumb.is_none() {
            let name = made.format.extension().to_uppercase();
            let colour = ui.visuals().weak_text_color();
            let font = FontId::proportional(12.0);
            ui.painter().text(result.center(), egui::Align2::CENTER_CENTER, name, font, colour);
        }
        if stale && !item.is_busy() && !item.blocked() {
            let badge =
                egui::Rect::from_min_size(result.right_top() - vec2(20.0, 0.0), vec2(20.0, 20.0));
            // outlined in the text colour, which reads on the selected row
            // whatever its accent, where the badge's orange may not
            let edge = egui::Stroke::new(1.0, theme::over_fill(ui.visuals()).core);
            let button = egui::Button::new(RichText::new("⟳").size(13.0).color(Color32::BLACK))
                .fill(theme::NEUTRAL)
                .stroke(edge)
                .corner_radius(4.0);
            // placed, so the rows below don't move
            let again = widgets::named(ui.place(badge, button), &fl!("row-convert-again"));
            if again.on_hover_text(fl!("row-convert-again")).clicked() {
                action = Some(RowAction::Convert);
            }
        }
    }
    // where the result will show, a way to make it, as large, so the row
    // looks the same before and after: a bare mark, framed as a button while
    // the row is hovered, so the bar's Convert stays the one that stands out
    if offers_convert(item) {
        let framed = response.contains_pointer();
        // placed, so the rows below don't move
        let button = egui::Button::new("").min_size(result.size()).frame(framed);
        let button = widgets::named(ui.place(result, button), &fl!("convert"));
        let colour = ui.style().interact(&button).text_color();
        let font = FontId::proportional(18.0);
        ui.painter().text(result.center(), egui::Align2::CENTER_CENTER, "▶", font, colour);
        if button.on_hover_text(fl!("convert", "hint")).clicked() {
            action = Some(RowAction::Convert);
        }
    }
    let mut text = egui::Rect::from_min_max(
        egui::pos2(input.max.x + 10.0, inner.min.y),
        egui::pos2(result.min.x - 8.0, inner.max.y),
    );
    if response.hovered() || response.contains_pointer() {
        let button =
            egui::Rect::from_min_size(egui::pos2(text.max.x - 20.0, text.min.y), vec2(20.0, 20.0));
        text.max.x = button.min.x - 4.0;
        // placed, so the rows below don't move
        let remove = ui.place(button, egui::Button::new("✖").small().frame(false));
        if widgets::named(remove, &fl!("remove")).on_hover_text(fl!("remove-file")).clicked() {
            action = Some(RowAction::Remove);
        }
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(text).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 2.0;
    // nothing in it takes the click that selects the row
    child.style_mut().interaction.selectable_labels = false;
    // the file's name in the headings' weight, over what is said of it
    let mut strong = text_format(&child, child.visuals().strong_text_color());
    strong.font_id.family = crate::fonts::medium_in(child.ctx());
    let (name, _) = drawable(&child, fallbacks, &name, strong);
    child.add(egui::Label::new(name).truncate());
    let what = kind(item.format, item.choices.target);
    child.add(egui::Label::new(RichText::new(what).small().weak()));
    status(&mut child, item, queued);
    let response = response.on_hover_ui(|ui| {
        let plain = text_format(ui, ui.visuals().text_color());
        let mut missing = Vec::new();
        for input in &item.inputs {
            let (path, lacking) =
                drawable(ui, fallbacks, &input.display().to_string(), plain.clone());
            ui.label(path);
            missing.extend(lacking);
        }
        if let Some(note) = missing_note(&missing) {
            widgets::note(ui, note);
        }
    });
    (response, action)
}

/// The row of item `id` in the list.
fn row_id(ui: &egui::Ui, id: u64) -> egui::Id {
    ui.id().with(("file", id))
}

/// Whether a row has a button to convert where its result would be: until
/// there is a result, or one of an earlier run, and not while one is
/// being made, or while something keeps it from being converted.
fn offers_convert(item: &Item) -> bool {
    item.last_result().is_none() && !item.is_busy() && !item.blocked()
}

/// A thumbnail in `rect`, marked when it is out of date.
fn thumb(
    ui: &egui::Ui,
    rect: egui::Rect,
    texture: Option<&egui::TextureHandle>,
    failed: bool,
    stale: bool,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, theme::stage(ui.visuals().dark_mode));
    match texture {
        Some(texture) => {
            let size = texture.size_vec2();
            let scale = (THUMB / size.x).min(THUMB / size.y);
            let shown = egui::Rect::from_center_size(rect.center(), size * scale);
            widgets::checkerboard(&painter, shown, 6.0);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            let tint = if stale { Color32::from_gray(140) } else { Color32::WHITE };
            painter.image(texture.id(), shown, uv, tint);
        }
        None if failed => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "✖",
                egui::FontId::proportional(16.0),
                theme::bad(ui.visuals()),
            );
        }
        None => {}
    }
}

fn status(ui: &mut egui::Ui, item: &Item, queued: Option<usize>) {
    let small = |text: String| RichText::new(text).small();
    let job = item.job.as_ref().map(|conversion| &conversion.job);
    // what keeps it from being converted, said where its settings are
    if job.is_none() && item.blocked() {
        // ffmpeg alone, which the banner says how to get
        let (text, hint) = match &item.blockers[..] {
            [Blocker::NeedsFfmpeg] => (fl!("row-needs-ffmpeg"), fl!("row-needs-ffmpeg", "hint")),
            _ => (fl!("row-attention"), fl!("row-attention", "hint")),
        };
        let text = small(format!("⚠ {text}"));
        ui.colored_label(theme::warn(ui.visuals()), text).on_hover_text(hint);
        return;
    }
    match (job, item.failed.as_ref().or(item.made.as_ref())) {
        (Some(job), _) if job.is_running() => {
            ui.add_space(3.0);
            let size = vec2(ui.available_width(), 6.0);
            let colour = theme::mark(ui.visuals());
            let fill = widgets::Fill::working(job.progress.overall());
            widgets::bar_sized(ui, size, fill, "", colour);
        }
        (Some(_), _) => {
            ui.label(
                small(match queued {
                    Some(0) | None => fl!("row-next"),
                    Some(n) => fl!("row-waiting", ahead = n),
                })
                .weak(),
            );
        }
        (None, Some(made)) => match &made.job.status {
            // out of date, it says nothing of how it went: the badge on
            // its picture says to make it again
            Status::Done(done) if !item.result_is_current() => {
                let stale = small(fl!("row-stale", size = widgets::kib(done.bytes))).weak();
                ui.add(egui::Label::new(stale).truncate());
            }
            Status::Done(done) => {
                let refused = done.problems.iter().any(|p| p.refused);
                let colour =
                    if refused { theme::bad(ui.visuals()) } else { theme::good(ui.visuals()) };
                let mark = if refused { "✖" } else { "✔" };
                let size = widgets::kib(done.bytes);
                ui.colored_label(colour, small(format!("{mark} {size}")));
            }
            Status::Failed(_) => {
                ui.colored_label(
                    theme::bad(ui.visuals()),
                    small(format!("✖ {}", fl!("row-failed"))),
                );
            }
            Status::Exists(_) => {
                ui.colored_label(theme::warn(ui.visuals()), small(fl!("row-exists")));
            }
            Status::Cancelled => {
                ui.label(small(fl!("row-stopped")).weak());
            }
            Status::Waiting | Status::Running => {}
        },
        (None, None) => {}
    }
}

/// What a result in `format` for `target` is called.
pub(crate) fn kind(format: Format, target: telegram::Target) -> String {
    match (format, target) {
        (Format::Webm, telegram::Target::Sticker) => fl!("webm-sticker"),
        (Format::Webm, telegram::Target::Emoji) => fl!("webm-emoji"),
        (Format::Tgs, telegram::Target::Sticker) => fl!("tgs-sticker"),
        (Format::Tgs, telegram::Target::Emoji) => fl!("tgs-emoji"),
        (Format::Webp, telegram::Target::Sticker) => fl!("image-sticker"),
        (Format::Webp, telegram::Target::Emoji) => fl!("image-emoji"),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use tgradish_core::ffmpeg::FfmpegChoice;
    use tgradish_core::options::Fit;

    use super::*;
    use crate::testing::{convert, finish, harness, square, video};

    #[test]
    fn takes_in_what_a_file_dialog_picked() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
        square(&a, [255, 0, 0, 255]);
        square(&b, [0, 0, 255, 255]);
        let mut harness = harness(Config::default());
        // as a dialog answers, a moment later, while the window draws on
        let (answer, picked) = std::sync::mpsc::channel();
        let ctx = harness.ctx.clone();
        harness.state_mut().picking =
            Some(Task::spawn(&ctx, move |_| picked.recv().map_err(|err| err.to_string())));
        harness.run_steps(3);
        // another isn't asked for meanwhile
        harness.state_mut().pick(&ctx, Pick::Files);
        assert!(harness.state().items.is_empty());
        answer.send(Some(Picked::Frames(vec![a, b]))).unwrap();
        finish(&mut harness);
        let app = harness.state();
        assert!(app.picking.is_none());
        assert_eq!((app.items.len(), app.items[0].inputs.len()), (1, 2));
        // what Settings asks for goes there
        let ctx = harness.ctx.clone();
        let folder = dir.path().join("out");
        harness.state_mut().picking =
            Some(Task::spawn(&ctx, move |_| Ok(Some(Picked::OutputDir(folder)))));
        finish(&mut harness);
        assert_eq!(harness.state().config.gui.output_dir, Some(dir.path().join("out")));
    }

    /// Each file dialog says what it is for, and shows the files it is for
    /// where they are known: as rfd's dialog keeps them, which only its
    /// Debug shows.
    #[test]
    fn titles_each_file_dialog_with_what_it_is_for() {
        let picks = [
            Pick::Files,
            Pick::Folder,
            Pick::Frames,
            Pick::Inspected,
            Pick::Ffmpeg,
            Pick::OutputDir,
        ];
        let titles = [
            fl!("dialog-add-files"),
            fl!("dialog-add-folder"),
            fl!("dialog-add-images"),
            fl!("dialog-inspect"),
            fl!("dialog-ffmpeg"),
            fl!("dialog-results-folder"),
        ];
        let filters = [None, None, Some(fl!("images-filter")), Some(fl!("stickers-filter"))];
        for (n, pick) in picks.into_iter().enumerate() {
            let dialog = format!("{:?}", pick.dialog(rfd::FileDialog::new()));
            assert!(dialog.contains(&format!("title: Some({:?})", titles[n])), "{dialog}");
            match filters.get(n).cloned().flatten() {
                Some(name) => assert!(dialog.contains(&format!("name: {name:?}")), "{dialog}"),
                None => assert!(dialog.contains("filters: []"), "{dialog}"),
            }
        }
        let distinct: std::collections::BTreeSet<_> = titles.iter().collect();
        assert_eq!(distinct.len(), titles.len());
    }

    /// The window stays the dialog's parent, with its display, which rfd
    /// needs on Wayland to export the window for the portal's dialog.
    #[cfg(all(unix, not(target_os = "macos")))]
    #[allow(unsafe_code)]
    #[test]
    fn keeps_the_window_as_the_file_dialogs_parent() {
        use raw_window_handle::{
            DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
            RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
        };
        use std::ptr::NonNull;
        struct Window(NonNull<std::ffi::c_void>);
        impl HasWindowHandle for Window {
            fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
                let raw = RawWindowHandle::Wayland(WaylandWindowHandle::new(self.0));
                // SAFETY: only kept by the dialog, which isn't shown
                Ok(unsafe { WindowHandle::borrow_raw(raw) })
            }
        }
        impl HasDisplayHandle for Window {
            fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
                let raw = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(self.0));
                // SAFETY: as above
                Ok(unsafe { DisplayHandle::borrow_raw(raw) })
            }
        }
        let window = Window(NonNull::dangling());
        let dialog =
            format!("{:?}", Pick::Files.dialog(rfd::FileDialog::new().set_parent(&window)));
        assert!(dialog.contains("parent: Some(Wayland("), "{dialog}");
        assert!(dialog.contains("parent_display: Some(Wayland("), "{dialog}");
    }

    #[test]
    fn opens_settings_and_about_from_the_menu_over_the_list() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let mut harness = harness(Config::default());
        let menu = |harness: &mut Harness<App>, item: &str| {
            harness.get_by_label(&fl!("main-menu")).click();
            harness.run_steps(2);
            harness.get_by_label(item).click();
            harness.run_steps(2);
        };
        menu(&mut harness, &fl!("menu-settings"));
        assert!(harness.state().settings_dialog.is_open());
        // Settings has no way to About of its own any more
        assert!(harness.query_by_label(&fl!("about-and-licences")).is_none());
        harness.key_press(egui::Key::Escape);
        harness.run_steps(2);
        assert!(!harness.state().settings_dialog.is_open());
        // About, a licence opened in it, and About again: as it first was
        let notice = crate::about::notices().swap_remove(1);
        let (title, text) = (notice.title, notice.text.trim().lines().next().unwrap().to_owned());
        menu(&mut harness, &fl!("about-and-licences"));
        assert!(harness.state().about);
        harness.get_by_label(&title).click_accesskit();
        harness.run_steps(2);
        harness.get_by_label_contains(&text);
        harness.get_all_by_label(&fl!("close")).last().unwrap().click_accesskit();
        harness.run_steps(2);
        assert!(!harness.state().about);
        menu(&mut harness, &fl!("about-and-licences"));
        assert!(harness.query_by_label_contains(&text).is_none());
    }

    #[test]
    fn removes_the_file_whose_row_has_the_focus_by_its_key() {
        use egui_kittest::kittest::{NodeT, Queryable, by};
        let dir = tempfile::tempdir().unwrap();
        let arts: Vec<PathBuf> = (0..3)
            .map(|n| {
                let art = dir.path().join(format!("{n}.png"));
                square(&art, [255, 0, 0, 255]);
                art
            })
            .collect();
        let mut harness = harness(Config::default());
        harness.state_mut().add(arts);
        finish(&mut harness);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        let names: Vec<String> = harness.state().items.iter().map(Item::name).collect();
        let order = |harness: &egui_kittest::Harness<App>| -> Vec<u64> {
            harness.state().items.iter().map(|item| item.id).collect()
        };
        fn row(name: &str) -> egui_kittest::kittest::By<'_> {
            by().label(name).role(egui::accesskit::Role::Button)
        }
        // the middle one, though the last is selected; the focus goes on
        // to the next, which Backspace removes as well
        harness.get(row(&names[1])).focus();
        harness.run_steps(2);
        harness.key_press(egui::Key::Delete);
        harness.run_steps(2);
        assert_eq!(order(&harness), [ids[0], ids[2]]);
        assert!(harness.get(row(&names[2])).accesskit_node().is_focused());
        harness.key_press(egui::Key::Backspace);
        harness.run_steps(2);
        assert_eq!(order(&harness), [ids[0]]);
        // and back, as any removed are, and read again: what shows of the
        // one selected changes once it is, which a field may not keep the
        // focus through
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        finish(&mut harness);
        assert_eq!(order(&harness), ids);
        // typing in a field is typing: a number's, once it has the focus
        let field = egui::accesskit::Role::SpinButton;
        harness.query_all_by_role(field).next().expect("a number field").focus();
        harness.run_steps(2);
        assert!(harness.ctx.text_edit_focused());
        for key in [egui::Key::Delete, egui::Key::Backspace] {
            harness.key_press(key);
            harness.run_steps(2);
        }
        assert_eq!(order(&harness), ids);
    }

    #[test]
    fn shows_results_of_last_time_in_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let Some(video) = video(dir.path(), "64x48") else { return };
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let state = dir.path().join("state").join("session.json");
        let mut config = Config::default();
        config.ffmpeg.choice = FfmpegChoice::System;
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut first = harness(config.clone());
        first.state_mut().reopen(state.clone());
        first.state_mut().add(vec![video, art]);
        finish(&mut first);
        for index in [0, 1] {
            let id = first.state().items[index].id;
            first.state_mut().select(id);
            let choices = &mut first.state_mut().items[index].choices;
            (choices.preset, choices.webm.fit) = (Some("fast".into()), Some(Fit::Off));
            finish(&mut first);
            convert(&first);
            first.run_steps(2);
            finish(&mut first);
        }
        first.state_mut().save_session();
        drop(first);

        let mut second = harness(config);
        second.state_mut().reopen(state);
        finish(&mut second);
        let items = &second.state().items;
        assert_eq!(second.state().selected, Some(items[1].id));
        // the video's, not selected, read for its thumbnail alone
        assert!(items[0].result_thumb.is_some() && items[0].result_clip.is_idle());
        // the .tgs, which has none to show, is made all the same
        assert_eq!(items[1].made.as_ref().map(|made| made.format), Some(Format::Tgs));
        assert!(items.iter().all(|item| !offers_convert(item)));
    }
}
