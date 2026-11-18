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
use crate::keys::{Command, keys_text, pressed, with_keys};
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
        let pasted = egui::Button::new(fl!("add-pasted")).shortcut_text(keys_text(Command::Paste));
        if ui.add(pasted).on_hover_text(fl!("add-pasted", "hint")).clicked() {
            self.paste();
            ui.close();
        }
    }

    /// The menu behind the gear over the list: undoing and redoing,
    /// Settings, inspecting a sticker, the keyboard's shortcuts, and what
    /// isn't about the files at all, feedback and About.
    fn main_menu(&mut self, ui: &mut egui::Ui) {
        let item =
            |text: String, command| egui::Button::new(text).shortcut_text(keys_text(command));
        // greyed out, rather than gone, where there is nothing to undo
        let undo = ui.add_enabled(self.can_undo(), item(fl!("menu-undo"), Command::Undo));
        if undo.on_hover_text(fl!("menu-undo", "hint")).clicked() {
            self.undo(ui.ctx());
            ui.close();
        }
        // its first keys alone: both would make the menu wider than the
        // rest need, and leave its name little room
        let first = crate::keys::key_text(&crate::keys::shortcut_of(Command::Redo).keys[0]);
        let redo = egui::Button::new(fl!("menu-redo")).shortcut_text(first);
        let redo = ui.add_enabled(self.can_redo(), redo);
        if redo.on_hover_text(fl!("menu-redo", "hint")).clicked() {
            self.redo(ui.ctx());
            ui.close();
        }
        ui.separator();
        if ui.add(item(fl!("menu-settings"), Command::Settings)).clicked() {
            self.settings_dialog.open();
            ui.close();
        }
        let inspect = ui.add(item(fl!("inspect"), Command::Inspect));
        if inspect.on_hover_text(fl!("inspect", "hint")).clicked() {
            self.pick(ui.ctx(), Pick::Inspected);
            ui.close();
        }
        if ui.add(item(fl!("shortcuts"), Command::Shortcuts)).clicked() {
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
    /// to the window. It runs on a thread of its own, so the GUI keeps
    /// drawing and converting meanwhile, and what is picked is taken in as
    /// it draws. One at a time: while one is open, others aren't asked for.
    pub(crate) fn pick(&mut self, ctx: &egui::Context, pick: Pick) {
        if self.picking.is_some() {
            return;
        }
        // tests open none of the system's dialogs, only say one was asked for
        if cfg!(test) {
            #[cfg(test)]
            self.asked_to_pick.push(pick);
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
            // for Settings, which keeps it as it keeps any change; else, as
            // a message or a file's settings asked, kept at once, and used
            // over one given as the GUI was started
            Ok(Some(Picked::Ffmpeg(file))) if self.settings_dialog.is_open() => {
                self.config.ffmpeg.path = Some(file)
            }
            Ok(Some(Picked::Ffmpeg(file))) => {
                self.ffmpeg_override = (None, None);
                self.save_config_setting(|config| config.ffmpeg.path = Some(file.clone()));
            }
            Ok(Some(Picked::OutputDir(dir))) => self.config.gui.output_dir = Some(dir),
            Ok(None) => {}
            // what the task says is of a file shown, as "Reading it stopped"
            Err(_) => self.say(crate::banner::Message::warning(fl!("pick-failed"))),
        }
    }

    /// The icon and name, and beside them Add files… with its menu where
    /// `add`, the theme's button and the menu of the rest: over the list,
    /// or over the whole GUI while it has no files.
    fn header(&mut self, ui: &mut egui::Ui, add: bool) {
        // as far over the row as the line is under it, past the space egui
        // leaves after the row and the line's half of its own; its buttons
        // level with the bar over the preview beside it
        let margin = egui::Frame::side_top_panel(ui.style()).inner_margin.top;
        ui.add_space(under_header(ui.style()) - f32::from(margin));
        let icon_side = icon::side(ui.ctx(), ICON_ROOM);
        // as tall as the icon's room, or as the icon where it takes more,
        // with the buttons beside its middle: a fractional scale's icon
        // is smaller, by whole screen pixels, and the buttons stay level
        let header = vec2(ui.available_width(), icon_side.max(ICON_ROOM));
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(header, layout, |ui| {
            let menu = ui.menu_button("⚙", |ui| self.main_menu(ui)).response;
            let name = fl!("main-menu");
            widgets::named(menu, &name).on_hover_text(name);
            self.theme_button(ui);
            // a split button: files, or the rest from its arrow; without
            // files, the drop zone's own is the one
            if add {
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    let add = egui::Button::new(fl!("add-files"));
                    self.add_button(ui, add, egui::Button::new("⏷"));
                });
            }
            // the icon and name, where the buttons leave room for them, the
            // space after the last already left out; the icon smaller where
            // it must be, down to a screen pixel for each of its own; below
            // a scale of 1 that is more than its room, as tall as the header
            let title = egui::WidgetText::from(RichText::new("tgradish").heading().strong());
            let galley = title.into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Heading,
            );
            let gap = ui.spacing().item_spacing.x;
            let room = ui.available_width();
            if let Some(shown) = icon::fitting(ui.ctx(), room.min(icon_side)) {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    icon::show(ui, room.min(icon_side));
                    if shown + gap + galley.size().x <= room {
                        ui.label(galley);
                    }
                });
            }
        });
    }

    /// A split button: `add` for Add files…, and after it `more`, the
    /// arrow of its menu of other ways to add files.
    fn add_button(&mut self, ui: &mut egui::Ui, add: egui::Button<'_>, more: egui::Button<'_>) {
        let files = |app: &mut App, ui: &mut egui::Ui| {
            if ui.add(add).on_hover_text(keys_text(Command::AddFiles)).clicked() {
                app.pick(ui.ctx(), Pick::Files);
            }
        };
        let menu = |app: &mut App, ui: &mut egui::Ui| {
            let button = egui::containers::menu::MenuButton::from_button(more);
            let (response, _) = button.ui(ui, |ui| app.add_menu(ui));
            widgets::named(response, &fl!("add-more"));
        };
        if ui.layout().prefer_right_to_left() {
            menu(self, ui);
            files(self, ui);
        } else {
            files(self, ui);
            menu(self, ui);
        }
    }

    pub(crate) fn files_panel(&mut self, ui: &mut egui::Ui) {
        self.header(ui, true);
        let rule = ui.separator().rect.center().y;

        // what Convert all converts, and what it leaves out
        let changed = self.items.iter().filter(|item| convertible(item)).count();
        let attention = self.items.iter().filter(|item| !item.is_busy() && item.blocked()).count();
        // without files there is nothing to convert or remove
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
                    let hint = with_keys(&fl!("convert-all", "hint"), Command::ConvertAll);
                    if ui.add_enabled(changed > 0, all).on_hover_text(hint).clicked() {
                        self.convert_all();
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
        let corner = ui.available_rect_before_wrap();
        self.banner.corner = Some(corner);
        // the room under the header's line, which the list ends with too:
        // its last row, scrolled to, and its drop highlight stand as far
        // over the footer's
        let under = corner.top() - (ui.painter().round_to_pixel_center(rule) + 0.5);
        let now = ui.input(|input| input.time);
        self.rows.follow(&self.items, now);
        let list = egui::ScrollArea::vertical().auto_shrink([false, false]);
        widgets::scrolled(ui, list, |ui| {
            let mut clicked = None;
            let mut removed = None;
            let mut again = None;
            // Delete or Backspace on the row with the keyboard's focus,
            // which a text field with the focus would take instead
            let delete = ui.input(|input| pressed(input, Command::Remove).is_some());
            let mut focus_next = None;
            let reveal = std::mem::take(&mut self.reveal_selected);
            let gap = |ui: &mut egui::Ui, rows: &Rows, before: Option<u64>| {
                let closing = rows.closing(ui.ctx(), before);
                if closing > 0.0 {
                    let step = ROW_HEIGHT + ui.spacing().item_spacing.y;
                    ui.add_space(step * closing);
                }
            };
            for (index, item) in self.items.iter().enumerate() {
                gap(ui, &self.rows, Some(item.id));
                let selected = self.selected == Some(item.id);
                let queued = self.queue.iter().position(|&id| id == item.id);
                let shown = self.rows.shown(ui.ctx(), item.id);
                let opacity = ui.opacity();
                ui.multiply_opacity(shown.row);
                let (response, action) = row(ui, item, selected, queued, &self.fallbacks, shown);
                ui.set_opacity(opacity);
                if selected && reveal {
                    response.scroll_to_me(None);
                }
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
            gap(ui, &self.rows, None);
            // after the spacing that follows the last row
            ui.add_space((under - ui.spacing().item_spacing.y).max(0.0));
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
        if let Some(list) = self.banner.corner {
            self.drop_target(ui, list.with_max_y(list.bottom() - under));
        }
    }

    /// The list lit up as where files dragged over the window go, with
    /// words that say so in its middle, fading in and out.
    fn drop_target(&self, ui: &egui::Ui, list: egui::Rect) {
        let time = if crate::motion::motion(ui.ctx()) { DROP_FADE } else { 0.0 };
        let shown = ui.ctx().animate_bool_with_time(drop_id(), self.hovering, time);
        if shown == 0.0 {
            return;
        }
        ui.data_mut(|data| data.insert_temp(drop_id(), list));
        let painter = ui.painter_at(list);
        let visuals = ui.visuals();
        let accent = theme::mark(visuals);
        painter.rect_filled(list, 6.0, accent.gamma_multiply(0.12 * shown));
        let edge = egui::Stroke::new(2.0, accent.gamma_multiply(shown));
        painter.rect_stroke(list, 6.0, edge, egui::StrokeKind::Inside);
        // on the panel's colour, to read over the rows
        let colour = visuals.strong_text_color().gamma_multiply(shown);
        let font = FontId::proportional(18.0);
        let galley = painter.layout(fl!("drop-to-add"), font, colour, list.width() - 48.0);
        let card = egui::Rect::from_center_size(list.center(), galley.size() + vec2(24.0, 16.0));
        painter.rect_filled(card, 6.0, visuals.panel_fill.gamma_multiply(shown));
        let outline = egui::Stroke::new(1.0, accent.gamma_multiply(shown));
        painter.rect_stroke(card, 6.0, outline, egui::StrokeKind::Inside);
        painter.galley(card.center() - galley.size() / 2.0, galley, colour);
    }

    /// Converts every file Convert all says it does.
    pub(crate) fn convert_all(&mut self) {
        let ids: Vec<u64> =
            self.items.iter().filter(|item| convertible(item)).map(|item| item.id).collect();
        for id in ids {
            self.enqueue(id);
        }
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

    /// The GUI while it has no files: the icon, name and buttons over a
    /// drop zone taking the rest, as there is no list to show yet. What
    /// passes is said in the zone's corner, where the list's would be,
    /// `list` wide.
    pub(crate) fn welcome(&mut self, ui: &mut egui::Ui, list: f32) {
        // the zone's edge as far under the header's row as the list's line
        self.header(ui, false);
        ui.add_space(SEPARATOR / 2.0);
        // as far from the bottom as from the sides, which the panel's own
        // margin is less than
        let mut zone = ui.available_rect_before_wrap();
        zone.max.y -= 8.0 - f32::from(egui::Frame::side_top_panel(ui.style()).inner_margin.bottom);
        let right = (zone.left() + list).min(zone.right());
        self.banner.corner = Some(egui::Rect::from_x_y_ranges(zone.left()..=right, zone.y_range()));
        self.drop_zone(ui, zone);
    }

    /// Where files are dropped, framed in `zone`, with the buttons that add
    /// them otherwise in its middle.
    pub(crate) fn drop_zone(&mut self, ui: &mut egui::Ui, zone: egui::Rect) {
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
        let inside = zone.shrink(16.0);
        let layout = egui::Layout::top_down(egui::Align::Center);
        ui.scope_builder(egui::UiBuilder::new().max_rect(inside).layout(layout), |ui| {
            // a little over the middle, where the eye falls
            ui.add_space((inside.height() / 2.0 - 110.0).max(0.0));
            ui.add(egui::Label::new(RichText::new(fl!("drop-here")).size(22.0)).wrap());
            ui.add_space(6.0);
            widgets::note(ui, fl!("drop-here-note"));
            ui.add_space(18.0);
            // in the middle, as wide as they were last drawn
            let id = ui.id().with("buttons");
            let width = ui.data(|data| data.get_temp::<f32>(id));
            let row = ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - width.unwrap_or(0.0)).max(0.0) / 2.0);
                let start = ui.cursor().left();
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 1.0;
                    theme::primary(ui.visuals_mut());
                    let add = egui::Button::new(widgets::medium(ui, fl!("add-files"), 14.0))
                        .min_size(vec2(130.0, ZONE_BUTTON));
                    let more = egui::Button::new("⏷").min_size(vec2(ZONE_BUTTON, ZONE_BUTTON));
                    self.add_button(ui, add, more);
                });
                if ui
                    .add(egui::Button::new(fl!("add-folder")).min_size(vec2(150.0, ZONE_BUTTON)))
                    .on_hover_text(fl!("add-folder", "hint"))
                    .clicked()
                {
                    self.pick(ui.ctx(), Pick::Folder);
                }
                if ui
                    .add(egui::Button::new(fl!("paste")).min_size(vec2(90.0, ZONE_BUTTON)))
                    .on_hover_text(keys_text(Command::Paste))
                    .clicked()
                {
                    self.paste();
                }
                ui.min_rect().right() - start
            });
            if width != Some(row.inner) {
                ui.data_mut(|data| data.insert_temp(id, row.inner));
                ui.ctx().request_discard("drop zone's buttons measured");
            }
        });
    }
}

/// The room egui's separator takes, its line in the middle.
const SEPARATOR: f32 = 6.0;

/// How far under the header's row the line under it is: the space egui
/// leaves after the row, and the line's half of the separator's room.
fn under_header(style: &egui::Style) -> f32 {
    style.spacing.item_spacing.y + SEPARATOR / 2.0
}

/// Height of the drop zone's buttons, larger than others, as they are the
/// one thing to do.
const ZONE_BUTTON: f32 = 34.0;

/// The space over the list's bar, and under it, which lay its buttons
/// level with those of the bar under the preview beside it.
fn footer_spaces(style: &egui::Style) -> (f32, f32) {
    use crate::actions::{BAR_ABOVE, BAR_ROW, below_bar};
    let taller = (BAR_ROW - theme::CONTROL) / 2.0;
    let central = egui::Frame::central_panel(style).inner_margin.bottom;
    let side = egui::Frame::side_top_panel(style).inner_margin.bottom;
    (BAR_ABOVE + taller, below_bar(style) + taller + f32::from(central - side))
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
    /// files it is for where those are known. Made on the GUI's thread,
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

/// Height of a row of the list.
const ROW_HEIGHT: f32 = THUMB + 12.0;
/// How long a row added takes to fade in, and the room of one removed to
/// close up, in seconds.
const ROW_CHANGE: f64 = 0.15;
/// How long a result's size and picture take to fade in as it is made.
const RESULT_FADE: f64 = 0.18;
/// How long a row not selected glows once as its result is made, and how
/// much of the accent shows through at the most.
const PULSE: f64 = 0.7;
const PULSE_MOST: f32 = 0.25;
/// How long the list takes to light up as files are dragged over the
/// window, and to go back.
const DROP_FADE: f32 = 0.12;

/// How far the list is lit up as where files go, in egui's memory.
fn drop_id() -> egui::Id {
    egui::Id::new("files dropping")
}
/// How long a row's tint takes to follow it being chosen or let go of;
/// its outline follows at once.
const SELECTING: f32 = 0.09;

/// How the rows change as files come and go and are made, for each to
/// move as it does.
#[derive(Default)]
pub(crate) struct Rows {
    /// The files listed last frame, in order; none before the first, when
    /// the rows there are already show as they are.
    listed: Option<Vec<u64>>,
    /// The files being made last frame.
    busy: std::collections::HashSet<u64>,
    /// When each row came, while it fades in.
    came: std::collections::HashMap<u64, f64>,
    /// When each file's result was made, while it fades in.
    made: std::collections::HashMap<u64, f64>,
    /// The room of rows taken out, closing up before the row of the file
    /// named, or at the end, and since when.
    gone: Vec<(Option<u64>, f64)>,
}

/// How a row shows as it changes, from 0 to 1: how far it has faded in,
/// and its result, and how much of the accent it glows with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shown {
    pub(crate) row: f32,
    pub(crate) result: f32,
    pub(crate) glow: f32,
}

impl Rows {
    /// Notes what changed since the last frame of `items`, at `now`.
    pub(crate) fn follow(&mut self, items: &[Item], now: f64) {
        let ids: Vec<u64> = items.iter().map(|item| item.id).collect();
        if let Some(listed) = &self.listed {
            // where a row was: before the first after it that stays
            let next = |id: u64| {
                let at = listed.iter().position(|&other| other == id)?;
                listed[at + 1..].iter().find(|other| ids.contains(other)).copied()
            };
            for before in self.gone.iter_mut().map(|(before, _)| before) {
                if let Some(id) = *before
                    && !ids.contains(&id)
                {
                    *before = next(id);
                }
            }
            for &id in listed.iter().filter(|id| !ids.contains(id)) {
                self.gone.push((next(id), now));
            }
            for &id in ids.iter().filter(|id| !listed.contains(id)) {
                self.came.insert(id, now);
            }
        }
        for item in items {
            let last = item.failed.as_ref().or(item.made.as_ref());
            let done = last.is_some_and(|last| matches!(last.job.status, Status::Done(_)));
            if item.job.is_none() && done && self.busy.contains(&item.id) {
                self.made.insert(item.id, now);
            }
        }
        self.busy = items.iter().filter(|item| item.job.is_some()).map(|item| item.id).collect();
        self.listed = Some(ids);
        self.came.retain(|_, since| now - *since < ROW_CHANGE);
        self.made.retain(|_, since| now - *since < PULSE);
        self.gone.retain(|(_, since)| now - *since < ROW_CHANGE);
    }

    /// How the row of file `id` shows now.
    pub(crate) fn shown(&self, ctx: &egui::Context, id: u64) -> Shown {
        let along = |since: Option<&f64>, time| {
            since.map_or(1.0, |since| crate::motion::along(ctx, *since, time))
        };
        let made = self.made.get(&id);
        Shown {
            row: along(self.came.get(&id), ROW_CHANGE),
            result: along(made, RESULT_FADE),
            glow: made.map_or(0.0, |_| glow(along(made, PULSE))),
        }
    }

    /// How many rows' room is still closing up before the row of file
    /// `before`, or at the end.
    pub(crate) fn closing(&self, ctx: &egui::Context, before: Option<u64>) -> f32 {
        let gone = self.gone.iter().filter(|(at, _)| *at == before);
        gone.map(|(_, since)| {
            1.0 - crate::inset::eased(f64::from(crate::motion::along(ctx, *since, ROW_CHANGE)))
        })
        .sum()
    }
}

/// How much of the accent a row glows with, `share` of the way through its
/// glow: none, up to the most halfway, and none again.
fn glow(share: f32) -> f32 {
    PULSE_MOST * (share.clamp(0.0, 1.0) * std::f32::consts::PI).sin()
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
    shown: Shown,
) -> (egui::Response, Option<RowAction>) {
    let width = ui.available_width();
    // by the file, so the focus stays with it as rows come and go
    let (_, rect) = ui.allocate_space(vec2(width, ROW_HEIGHT));
    let response = ui.interact(rect, row_id(ui, item.id), egui::Sense::click());
    // read out as a choice of the list, by its file's name
    let name = item.name();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &name)
    });
    let visuals = ui.visuals();
    // as Breeze marks items: the accent let through, more for the
    // selected one, which is outlined in it too, yet little enough that
    // the row's small coloured text still reads. The tint follows a
    // moment behind, the outline at once, so which is chosen is plain
    // from the click on
    let time = if crate::motion::motion(ui.ctx()) { SELECTING } else { 0.0 };
    let chosen = ui.ctx().animate_bool_with_time(response.id.with("chosen"), selected, time);
    let under = if response.hovered() { theme::row(visuals, false) } else { Color32::TRANSPARENT };
    let tint = under.lerp_to_gamma(theme::row(visuals, true), chosen);
    if tint != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 6.0, tint);
    }
    // made a moment ago while another is chosen: it glows once, to be seen
    if !selected && shown.glow > 0.0 {
        ui.painter().rect_filled(rect, 6.0, theme::mark(visuals).gamma_multiply(shown.glow));
    }
    if selected {
        let outline = egui::Stroke::new(1.0, theme::mark(visuals));
        ui.painter().rect_stroke(rect, 6.0, outline, egui::StrokeKind::Inside);
    }
    let inner = rect.shrink2(vec2(6.0, 6.0));
    let input = egui::Rect::from_min_size(inner.min, vec2(THUMB, THUMB));
    // unread for want of ffmpeg, which is said otherwise, isn't broken
    let broken = item.input_error().is_some() && !item.blockers.contains(&Blocker::NeedsFfmpeg);
    thumb(ui, input, item.input_thumb.as_ref(), broken, false, 1.0);
    let result =
        egui::Rect::from_min_size(egui::pos2(inner.max.x - THUMB, inner.min.y), vec2(THUMB, THUMB));
    let mut action = None;
    if let Some(made) = &item.made {
        let stale = !item.result_is_current();
        thumb(ui, result, item.result_thumb.as_ref(), false, stale, shown.result);
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
            // a button as the others are, solid over any picture, and
            // outlined in what is seen on the selected row whatever its
            // accent, and on the picture under it
            let edge = egui::Stroke::new(1.0, theme::badge_edge(ui.visuals()));
            let glyph = RichText::new("⟳").size(13.0).color(ui.visuals().strong_text_color());
            let button = egui::Button::new(glyph).stroke(edge).corner_radius(4.0);
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
    let text = egui::Rect::from_min_max(
        egui::pos2(input.max.x + 10.0, inner.min.y),
        egui::pos2(result.min.x - 8.0, inner.max.y),
    );
    // at the end of the name's line while the row is hovered, which the
    // name always leaves room for, so that it isn't cut short again then
    let button =
        egui::Rect::from_min_size(egui::pos2(text.max.x - 20.0, text.min.y), vec2(20.0, 20.0));
    if response.hovered() || response.contains_pointer() {
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
    child.scope(|ui| {
        ui.set_max_width(button.min.x - 4.0 - text.min.x);
        ui.add(egui::Label::new(name).truncate());
    });
    let what = kind(item.format, item.choices.target);
    child.add(egui::Label::new(RichText::new(what).small().weak()));
    status(&mut child, item, queued, shown.result);
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

/// Whether Convert all converts `item`: not while it converts or waits to,
/// nor with its result up to date, nor while something keeps it from
/// being converted.
fn convertible(item: &Item) -> bool {
    !item.is_busy() && !item.result_is_current() && !item.blocked()
}

/// Whether a row has a button to convert where its result would be: until
/// there is a result, or one of an earlier run, and not while one is
/// being made, or while something keeps it from being converted.
fn offers_convert(item: &Item) -> bool {
    item.last_result().is_none() && !item.is_busy() && !item.blocked()
}

/// A thumbnail in `rect`, marked when it is out of date, `opacity` of it
/// drawn.
fn thumb(
    ui: &egui::Ui,
    rect: egui::Rect,
    texture: Option<&egui::TextureHandle>,
    failed: bool,
    stale: bool,
    opacity: f32,
) {
    let mut painter = ui.painter_at(rect);
    painter.multiply_opacity(opacity);
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

/// What is said of `item` under its name; a result made a moment ago,
/// `fading` of it faded in.
fn status(ui: &mut egui::Ui, item: &Item, queued: Option<usize>, fading: f32) {
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
            // its picture says to make it again, so where the size and that
            // it is out of date don't fit together, the size alone, whole
            Status::Done(done) if !item.result_is_current() => {
                let size = widgets::kib(done.bytes);
                let stale = fl!("row-stale", size = size.as_str());
                let font = egui::TextStyle::Small.resolve(ui.style());
                let fits = |text: &str| {
                    let colour = ui.visuals().text_color();
                    let galley = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), colour);
                    galley.size().x <= ui.available_width()
                };
                let said = if fits(&stale) { stale } else { size };
                ui.add(egui::Label::new(small(said).weak()).truncate());
            }
            Status::Done(done) => {
                let refused = done.problems.iter().any(|p| p.refused);
                let colour =
                    if refused { theme::bad(ui.visuals()) } else { theme::good(ui.visuals()) };
                let mark = if refused { "✖" } else { "✔" };
                let size = widgets::kib(done.bytes);
                let opacity = ui.opacity();
                ui.multiply_opacity(fading);
                ui.colored_label(colour, small(format!("{mark} {size}")));
                ui.set_opacity(opacity);
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
        // as a dialog answers, a moment later, while the GUI draws on
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
            // which says its keys after it, where it has any
            harness.get_by_label_contains(item).click();
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
        let title = crate::about::title(notice.kind);
        let text = notice.text.trim().lines().next().unwrap().to_owned();
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
    fn is_all_one_drop_zone_until_a_file_is_added() {
        use crate::banner::Message;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let mut harness = harness(Config::default());
        harness.run_steps(3);
        let screen = harness.ctx.content_rect();
        // the zone in the middle of the whole GUI, under the icon, the
        // theme's button and the menu, with the one Add files…
        let drop = harness.get_by_label(&fl!("drop-here")).rect();
        assert!((drop.center().x - screen.center().x).abs() < 1.0, "{drop:?}");
        let name = fl!("add-files");
        let add: Vec<_> = harness.get_all_by_label(&name).map(|node| node.rect()).collect();
        assert_eq!(add.len(), 1);
        assert!(add[0].top() > drop.bottom() && add[0].top() - drop.bottom() < 80.0, "{add:?}");
        let menu = harness.get_by_label(&fl!("main-menu")).rect();
        assert!(menu.right() > screen.right() - 16.0 && menu.bottom() < drop.top());
        harness.get_by_label(&fl!("light-theme"));
        // and its other ways to add files, the menu's among them
        harness.get_by_label(&fl!("add-folder"));
        harness.get_by_label(&fl!("paste"));
        harness.get_by_label(&fl!("add-more")).click();
        harness.run_steps(2);
        harness.get_by_label(&fl!("add-images")).click();
        harness.run_steps(2);
        assert_eq!(harness.state().asked_to_pick, [Pick::Frames]);
        assert!(harness.query_by_label(&fl!("remove-all")).is_none());
        // what passes, in the corner the list would have, and what stays
        // clear of it, right of where the list would be
        let area = |harness: &Harness<App>, id: &str| {
            harness.ctx.memory(|memory| memory.area_rect(egui::Id::new(id))).unwrap()
        };
        harness.state_mut().say(Message::warning("Stays"));
        harness.state_mut().say(Message::info("Passes"));
        harness.run_steps(3);
        let passes = area(&harness, "notices");
        assert!(passes.left() < 20.0 && passes.bottom() > screen.bottom() - 20.0, "{passes:?}");
        let stays = area(&harness, "banner");
        assert!(stays.left() > passes.right(), "{stays:?} beside {passes:?}");

        // a file brings the list, and the zone goes; what stays doesn't move
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        assert!(harness.query_by_label(&fl!("drop-here")).is_none());
        harness.get_by_label(&fl!("remove-all"));
        assert!((area(&harness, "banner").left() - stays.left()).abs() < 1.0);
        harness.state_mut().banner = Default::default();
        let add = harness.get_by_label(&fl!("add-files")).rect();
        assert!(add.right() < screen.center().x, "{add:?}");
        // and comes back as the last goes
        let id = harness.state().items[0].id;
        harness.state_mut().remove(id);
        harness.run_steps(3);
        harness.get_by_label(&fl!("drop-here"));
        harness.get_by_label(&fl!("undo-removing")).click();
        harness.run_steps(3);
        assert!(harness.query_by_label(&fl!("drop-here")).is_none());
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

    /// The list's row of the file `name`, which is named elsewhere too.
    fn list_row<'a>(
        harness: &'a egui_kittest::Harness<App>,
        name: &'a str,
    ) -> egui_kittest::Node<'a> {
        use egui_kittest::kittest::{Queryable, by};
        harness.get(by().label(name).role(egui::accesskit::Role::Button))
    }

    /// The GUI drawn a sixtieth of a second apart, as on a screen.
    fn stepped(config: Config) -> egui_kittest::Harness<'static, App> {
        egui_kittest::Harness::builder()
            .with_size([1280.0, 820.0])
            .with_step_dt(1.0 / 60.0)
            .with_max_steps(120)
            .build_eframe(move |_| {
                App::with(config.clone(), None, tgradish_core::presets::Presets::builtin())
            })
    }

    #[test]
    fn fades_rows_in_and_closes_up_the_room_of_those_removed() {
        let dir = tempfile::tempdir().unwrap();
        let names = ["a.png", "b.png", "c.png"];
        let paths: Vec<PathBuf> = names.iter().map(|name| dir.path().join(name)).collect();
        paths.iter().for_each(|path| square(path, [255, 0, 0, 255]));
        let mut harness = stepped(Config::default());
        harness.state_mut().add(paths[..2].to_vec());
        finish(&mut harness);
        // the third fades in over 150 ms
        harness.state_mut().add(vec![paths[2].clone()]);
        harness.step();
        let id = harness.state().items[2].id;
        let row =
            |harness: &egui_kittest::Harness<App>| harness.state().rows.shown(&harness.ctx, id).row;
        assert!(row(&harness) < 0.15, "{}", row(&harness));
        harness.run_steps(4);
        assert!((0.3..0.7).contains(&row(&harness)), "{}", row(&harness));
        harness.run_steps(6);
        assert_eq!(row(&harness), 1.0);
        // the first taken out: those after it move up into its room, from
        // where they were
        let top =
            |harness: &egui_kittest::Harness<App>, name: &str| list_row(harness, name).rect().top();
        let (first, second) = (top(&harness, "a.png"), top(&harness, "b.png"));
        let id = harness.state().items[0].id;
        harness.state_mut().remove(id);
        harness.step();
        // most of the way where it was: the notice of it, laid out again,
        // draws the frame again a moment on
        let start = top(&harness, "b.png");
        assert!(start > first + 0.6 * (second - first) && start <= second, "{start}");
        harness.run_steps(4);
        let mid = top(&harness, "b.png");
        assert!(mid < second - 5.0 && mid > first + 5.0, "{mid}");
        harness.run_steps(6);
        assert_eq!(top(&harness, "b.png"), first);
        assert_eq!(top(&harness, "c.png"), second);
        // and nothing is drawn again once done
        harness.try_run().expect("still once done");
    }

    #[test]
    fn shows_a_result_made_as_it_is_and_lets_its_row_glow_once_unless_selected() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
        square(&a, [255, 0, 0, 255]);
        square(&b, [0, 0, 255, 255]);
        let mut config = Config::default();
        config.gui.output_dir = Some(dir.path().join("out"));
        let mut harness = stepped(config);
        harness.state_mut().add(vec![a, b]);
        finish(&mut harness);
        // the first is converted while the second is chosen
        let (first, second) = (harness.state().items[0].id, harness.state().items[1].id);
        harness.state_mut().select(second);
        harness.state_mut().enqueue(first);
        let mut waited = 0;
        while harness.state().item(first).is_some_and(|item| item.job.is_some() || waited == 0) {
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(5));
            waited += 1;
            assert!(waited < 2000, "never made");
        }
        assert!(harness.state().item(first).unwrap().made.is_some());
        let shown =
            |harness: &egui_kittest::Harness<App>| harness.state().rows.shown(&harness.ctx, first);
        // from nothing, its size and picture over 180 ms, its glow up to a
        // quarter of the accent halfway through 700 ms, and none at its end
        let start = shown(&harness);
        assert!(start.result < 0.15 && start.glow < 0.05, "{start:?}");
        harness.run_steps(5);
        let fading = shown(&harness);
        assert!((0.3..0.8).contains(&fading.result), "{fading:?}");
        harness.run_steps(15);
        let middle = shown(&harness);
        assert_eq!(middle.result, 1.0);
        assert!((middle.glow - PULSE_MOST).abs() < 0.03, "{middle:?}");
        harness.run_steps(30);
        assert_eq!(shown(&harness), Shown { row: 1.0, result: 1.0, glow: 0.0 });
        assert!(glow(0.0) == 0.0 && glow(1.0).abs() < 1e-6 && glow(0.5) == PULSE_MOST);
        harness.try_run().expect("still once done");
    }

    #[test]
    fn marks_a_row_chosen_at_once() {
        use egui_kittest::kittest::NodeT;
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
        square(&a, [255, 0, 0, 255]);
        square(&b, [0, 0, 255, 255]);
        let mut harness = stepped(Config::default());
        harness.state_mut().add(vec![a, b]);
        finish(&mut harness);
        list_row(&harness, "a.png").click();
        // chosen as the click is taken, and drawn so the frame after, its
        // tint following
        harness.run_steps(2);
        let selected = |harness: &egui_kittest::Harness<App>, name: &str| {
            list_row(harness, name).accesskit_node().toggled()
        };
        use egui::accesskit::Toggled;
        assert_eq!(selected(&harness, "a.png"), Some(Toggled::True));
        assert_eq!(selected(&harness, "b.png"), Some(Toggled::False));
        harness.try_run().expect("still once the tint is there");
    }

    #[test]
    fn cuts_a_long_name_short_the_same_whether_its_row_is_hovered_or_not() {
        use egui_kittest::kittest::{NodeT, Queryable};
        let dir = tempfile::tempdir().unwrap();
        let long = dir.path().join(format!("{}.png", "a_rather_long_name_of_a_file".repeat(3)));
        square(&long, [255, 0, 0, 255]);
        let mut harness =
            egui_kittest::Harness::builder().with_size([1280.0, 820.0]).build_eframe(|cc| {
                let app =
                    App::with(Config::default(), None, tgradish_core::presets::Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
        harness.state_mut().add(vec![long]);
        finish(&mut harness);
        // the name as the row shows it, cut short
        let shown = |harness: &egui_kittest::Harness<App>| {
            let labels = harness.query_all_by_role(egui::accesskit::Role::Label);
            let values = labels.filter_map(|node| node.accesskit_node().value());
            values
                .filter(|value| value.starts_with("a_rather") && value.contains('…'))
                .collect::<Vec<_>>()
        };
        let before = shown(&harness);
        assert_eq!(before.len(), 1, "{before:?}");
        let row = list_row(&harness, &harness.state().items[0].name()).rect().center();
        harness.event(egui::Event::PointerMoved(row));
        harness.run_steps(2);
        harness.get_by_label(&fl!("remove"));
        assert_eq!(shown(&harness), before);
    }

    #[test]
    fn ends_the_list_as_far_over_the_footer_as_it_starts_under_the_header() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..20)
            .map(|n| {
                let path = dir.path().join(format!("{n:02}.png"));
                square(&path, [255, 0, 0, 255]);
                path
            })
            .collect();
        let mut harness = stepped(Config::default());
        // spaced as the GUI is
        let ctx = harness.ctx.clone();
        harness.state().style(&ctx);
        harness.state_mut().add(paths);
        finish(&mut harness);
        let list = harness.state().banner.corner.unwrap();
        harness.event(egui::Event::PointerMoved(list.center()));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -10_000.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.run_steps(60);
        let last = harness.state().items.last().unwrap().name();
        let under = list.bottom() - list_row(&harness, &last).rect().bottom();
        assert!((7.5..=8.5).contains(&under), "{under}");
    }

    #[test]
    fn centres_the_headers_row_between_the_top_and_the_line_under_it() {
        use egui_kittest::kittest::{NodeT, Queryable};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        square(&path, [255, 0, 0, 255]);
        for ppp in [0.75, 0.9, 1.0, 1.25, 1.5, 2.0] {
            let mut harness = egui_kittest::Harness::builder()
                .with_size([1280.0, 820.0])
                .with_pixels_per_point(ppp)
                .build_eframe(|cc| {
                    let app = App::with(
                        Config::default(),
                        None,
                        tgradish_core::presets::Presets::builtin(),
                    );
                    app.style(&cc.egui_ctx);
                    app
                });
            // the buttons' top and bottom, in points
            let buttons = |harness: &egui_kittest::Harness<'_, App>| {
                let name = fl!("main-menu");
                let menu = harness.get_by_label(&name);
                let rect = menu.accesskit_node().bounding_box().unwrap();
                (rect.y0 as f32 / ppp, rect.y1 as f32 / ppp)
            };
            // without files, the drop zone's edge is the line
            harness.run_steps(3);
            let (top, bottom) = buttons(&harness);
            let zone = harness.state().banner.corner.unwrap().top();
            assert!(((zone - bottom) - top).abs() * ppp <= 1.0, "{ppp}: {top} {bottom} {zone}");
            harness.state_mut().add(vec![path.clone()]);
            finish(&mut harness);
            harness.run_steps(3);
            let (top, bottom) = buttons(&harness);
            let line = harness
                .output()
                .shapes
                .iter()
                .find_map(|clipped| match clipped.shape {
                    egui::Shape::LineSegment { points: [from, to], .. }
                        if from.y == to.y && from.y > bottom && to.x - from.x > 100.0 =>
                    {
                        Some(from.y)
                    }
                    _ => None,
                })
                .expect("the line under the header");
            assert!(((line - bottom) - top).abs() * ppp <= 1.0, "{ppp}: {top} {bottom} {line}");
            // level with the bar over the preview where the icon fits in its
            // room
            if ppp >= 1.0 {
                let name = fl!("show-input");
                let input = harness.get_by_label(&name);
                let input = input.accesskit_node().bounding_box().unwrap().y0 as f32 / ppp;
                assert!((input - top).abs() * ppp <= 1.0, "{ppp}: {top} {input}");
            }
        }
    }

    #[test]
    fn lights_up_the_list_rather_than_the_window_as_files_are_dragged_over() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        square(&a, [255, 0, 0, 255]);
        let mut harness = stepped(Config::default());
        // spaced as the GUI is
        let ctx = harness.ctx.clone();
        harness.state().style(&ctx);
        harness.state_mut().add(vec![a]);
        finish(&mut harness);
        // as far as it is lit: asked again in the same frame, it stays
        let lit = |harness: &egui_kittest::Harness<App>, over: bool| {
            harness.ctx.animate_bool_with_time(drop_id(), over, DROP_FADE)
        };
        assert_eq!(lit(&harness, false), 0.0);
        // files over the window, as egui says each frame
        let over = |harness: &mut egui_kittest::Harness<App>, frames: usize| {
            for _ in 0..frames {
                harness.state_mut().hovering = true;
                harness.step();
            }
        };
        over(&mut harness, 2);
        let start = lit(&harness, true);
        assert!(start > 0.0 && start < 0.4, "{start}");
        over(&mut harness, 2);
        let middle = lit(&harness, true);
        assert!(middle > start && middle < 0.8, "{middle}");
        over(&mut harness, 6);
        assert_eq!(lit(&harness, true), 1.0);
        // as far over the footer's line, where the list ends, as under the
        // header's, which is the list's own spacing above it
        let shown = harness.ctx.data(|data| data.get_temp::<egui::Rect>(drop_id())).unwrap();
        let list = harness.state().banner.corner.unwrap();
        assert_eq!(shown.top(), list.top());
        let under = list.bottom() - shown.bottom();
        assert!((7.5..=8.5).contains(&under), "{under}");
        // and back as they leave
        harness.run_steps(10);
        assert_eq!(lit(&harness, false), 0.0);
    }
}
