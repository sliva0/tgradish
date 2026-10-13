//! The list of files on the left: a row for each, with its input, its
//! result and how converting it went, under the buttons that add files,
//! inspect a sticker, and open the settings.

use eframe::egui::{self, Color32, RichText, vec2};
use tgradish_core::presets::Format;
use tgradish_core::{inputs, telegram};

use crate::app::{App, set_theme};
use crate::fonts::{drawable, missing_note, text_format};
use crate::item::Item;
use crate::jobs::Status;
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
            ("☀", "Light theme", tgradish_core::config::Theme::Light)
        } else {
            ("🌓", "Dark theme", tgradish_core::config::Theme::Dark)
        };
        if !ui.button(icon).on_hover_text(hint).clicked() {
            return;
        }
        set_theme(ui.ctx(), theme);
        self.keep("theme", |gui| gui.theme = theme);
    }

    fn add_menu(&mut self, ui: &mut egui::Ui) {
        if ui
            .button("A folder of frames…")
            .on_hover_text("Its images, in name order, as one animation")
            .clicked()
        {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.add(vec![dir]);
            }
            ui.close();
        }
        if ui
            .button("Images as one animation…")
            .on_hover_text("The images picked, in name order")
            .clicked()
        {
            if let Some(files) = rfd::FileDialog::new()
                .add_filter("images", &[inputs::IMAGES, inputs::ASEPRITE].concat())
                .pick_files()
            {
                self.add_frames(files);
            }
            ui.close();
        }
        if ui
            .button("From the clipboard")
            .on_hover_text("Copied files, paths or an image (Ctrl+V)")
            .clicked()
        {
            self.paste();
            ui.close();
        }
    }

    fn pick_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new().pick_files() {
            self.add(files);
        }
    }

    pub(crate) fn files_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        let icon_side = icon::side(ui.ctx(), ICON_ROOM);
        // as tall as the icon, with the buttons beside its middle
        let header = vec2(ui.available_width(), icon_side);
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(header, layout, |ui| {
            if ui.button("⚙").on_hover_text("Settings").clicked() {
                self.prefs.open();
            }
            self.theme_button(ui);
            if ui
                .button("🔬")
                .on_hover_text("Inspect a sticker: check a file against Telegram's rules")
                .clicked()
            {
                self.pick_inspected();
            }
            // a split button: files, or the rest from its arrow
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 1.0;
                ui.menu_button("⏷", |ui| self.add_menu(ui));
                if ui.button("Add files…").clicked() {
                    self.pick_files();
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

        let changed =
            self.items.iter().filter(|item| !item.is_busy() && !item.result_is_current()).count();
        egui::Panel::bottom("files-footer").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let text = if changed > 0 {
                    format!("Convert all ({changed})")
                } else {
                    "Convert all".into()
                };
                if ui
                    .add_enabled(changed > 0, egui::Button::new(text))
                    .on_hover_text("Every file without an up-to-date result")
                    .clicked()
                {
                    let ids: Vec<u64> = self
                        .items
                        .iter()
                        .filter(|item| !item.is_busy() && !item.result_is_current())
                        .map(|item| item.id)
                        .collect();
                    for id in ids {
                        self.enqueue(id);
                    }
                }
                if !self.items.is_empty() && ui.button("Remove all").clicked() {
                    let ids: Vec<u64> = self.items.iter().map(|item| item.id).collect();
                    for id in ids {
                        self.remove(id);
                    }
                }
            });
            ui.add_space(4.0);
        });

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.items.is_empty() {
                ui.add_space(20.0);
                widgets::note(ui, "Files you add show up here.");
            }
            let mut clicked = None;
            let mut removed = None;
            let mut again = None;
            for item in &self.items {
                let selected = self.selected == Some(item.id);
                let queued = self.queue.iter().position(|&id| id == item.id);
                let (response, action) = row(ui, item, selected, queued, &self.fallbacks);
                match action {
                    Some(RowAction::Remove) => removed = Some(item.id),
                    Some(RowAction::Convert) => again = Some(item.id),
                    None if response.clicked() => clicked = Some(item.id),
                    None => {}
                }
                response.context_menu(|ui| {
                    if ui.button("Remove").clicked() {
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
            if let Some(id) = again {
                self.enqueue(id);
            }
        });
    }

    pub(crate) fn empty(&mut self, ui: &mut egui::Ui) {
        let zone = ui.max_rect().shrink(28.0);
        let colour = if self.hovering {
            theme::accent(ui.visuals())
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
            ui.label(RichText::new("Drop videos, GIFs, images or Aseprite files here").size(22.0));
            ui.add_space(6.0);
            widgets::note(
                ui,
                "Videos become WebM stickers, pictures image stickers; pixel art can also become a crisp TGS animation.",
            );
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                let width = 400.0;
                ui.add_space((ui.available_width() - width).max(0.0) / 2.0);
                let add = egui::Button::new(RichText::new("Add files…").strong())
                    .min_size(vec2(130.0, 34.0));
                if ui.add(add).clicked() {
                    self.pick_files();
                }
                if ui
                    .add(egui::Button::new("A folder of frames…").min_size(vec2(150.0, 34.0)))
                    .clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder()
                {
                    self.add(vec![dir]);
                }
                if ui
                    .add(egui::Button::new("Paste").min_size(vec2(90.0, 34.0)))
                    .on_hover_text("Ctrl+V")
                    .clicked()
                {
                    self.paste();
                }
            });
        });
    }
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
    let (rect, response) = ui.allocate_exact_size(vec2(width, THUMB + 12.0), egui::Sense::click());
    let visuals = ui.visuals();
    // as Breeze marks items: the accent let through, more for the
    // selected one, which is outlined in it too
    let accent = theme::accent(visuals);
    if selected {
        ui.painter().rect_filled(rect, 6.0, visuals.selection.bg_fill);
        let outline = egui::Stroke::new(1.0, accent);
        ui.painter().rect_stroke(rect, 6.0, outline, egui::StrokeKind::Inside);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, theme::tint(accent, 0.15));
    }
    let inner = rect.shrink2(vec2(6.0, 6.0));
    let input = egui::Rect::from_min_size(inner.min, vec2(THUMB, THUMB));
    thumb(ui, input, item.input_thumb.as_ref(), item.input_error().is_some(), false);
    let result =
        egui::Rect::from_min_size(egui::pos2(inner.max.x - THUMB, inner.min.y), vec2(THUMB, THUMB));
    let mut action = None;
    if item.result_thumb.is_some() {
        let stale = !item.result_is_current();
        thumb(ui, result, item.result_thumb.as_ref(), false, stale);
        if stale && !item.is_busy() {
            let badge =
                egui::Rect::from_min_size(result.right_top() - vec2(20.0, 0.0), vec2(20.0, 20.0));
            let button = egui::Button::new(RichText::new("⟳").size(13.0).color(Color32::BLACK))
                .fill(widgets::WARN)
                .corner_radius(4.0);
            // placed, so the rows below don't move
            if ui.place(badge, button).on_hover_text("Settings changed: convert again").clicked() {
                action = Some(RowAction::Convert);
            }
        }
    }
    // where the result will show, a way to make it, as wide as it says
    let mut text_end = result.min.x;
    if item.result_thumb.is_none() && !item.is_busy() && item.job.is_none() {
        let width = widgets::text_width(ui, "Convert", 16.0).max(THUMB);
        let button = egui::Rect::from_min_size(
            egui::pos2(result.max.x - width, result.center().y - 12.0),
            vec2(width, 24.0),
        );
        text_end = button.min.x;
        // placed, so the rows below don't move
        if ui
            .place(button, egui::Button::new("Convert"))
            .on_hover_text("Make its sticker")
            .clicked()
        {
            action = Some(RowAction::Convert);
        }
    }
    let mut text = egui::Rect::from_min_max(
        egui::pos2(input.max.x + 10.0, inner.min.y),
        egui::pos2(text_end - 8.0, inner.max.y),
    );
    if response.hovered() || response.contains_pointer() {
        let button =
            egui::Rect::from_min_size(egui::pos2(text.max.x - 20.0, text.min.y), vec2(20.0, 20.0));
        text.max.x = button.min.x - 4.0;
        // placed, so the rows below don't move
        if ui
            .place(button, egui::Button::new("✖").small().frame(false))
            .on_hover_text("Remove it from the list")
            .clicked()
        {
            action = Some(RowAction::Remove);
        }
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(text).layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.spacing_mut().item_spacing.y = 2.0;
    let strong = text_format(&child, child.visuals().strong_text_color());
    let (name, _) = drawable(&child, fallbacks, &item.name(), strong);
    child.add(egui::Label::new(name).truncate().selectable(false));
    let what = match (item.format, item.choices.target) {
        (Format::Webm, telegram::Target::Sticker) => "WebM sticker",
        (Format::Webm, telegram::Target::Emoji) => "WebM emoji",
        (Format::Tgs, telegram::Target::Sticker) => "TGS sticker",
        (Format::Tgs, telegram::Target::Emoji) => "TGS emoji",
        (Format::Image, telegram::Target::Sticker) => "Image sticker",
        (Format::Image, telegram::Target::Emoji) => "Image emoji",
    };
    child.add(egui::Label::new(RichText::new(what).small().weak()).selectable(false));
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
                widgets::BAD,
            );
        }
        None => {}
    }
}

fn status(ui: &mut egui::Ui, item: &Item, queued: Option<usize>) {
    let small = |text: String| RichText::new(text).small();
    match (&item.job, item.failed.as_ref().or(item.made.as_ref())) {
        (Some((job, ..)), _) if job.is_running() => {
            ui.add_space(3.0);
            let size = vec2(ui.available_width(), 6.0);
            let colour = theme::accent(ui.visuals());
            let fill = widgets::Fill::working(job.progress.overall());
            widgets::bar_sized(ui, size, fill, "", colour);
        }
        (Some(_), _) => {
            ui.label(
                small(match queued {
                    Some(0) | None => "next".into(),
                    Some(n) => format!("waiting, {n} ahead"),
                })
                .weak(),
            );
        }
        (None, Some(made)) => match &made.job.status {
            Status::Done(done) => {
                let refused = done.problems.iter().any(|p| p.refused);
                let colour = if refused { widgets::BAD } else { widgets::GOOD };
                let mark = if refused { "✖" } else { "✔" };
                let stale = if item.result_is_current() { "" } else { " · settings changed" };
                ui.colored_label(
                    colour,
                    small(format!("{mark} {}{stale}", widgets::kib(done.bytes))),
                );
            }
            Status::Failed(_) => {
                ui.colored_label(widgets::BAD, small("✖ failed".into()));
            }
            Status::Exists(_) => {
                ui.colored_label(widgets::WARN, small("a file is in the way".into()));
            }
            Status::Cancelled => {
                ui.label(small("stopped".into()).weak());
            }
            Status::Waiting | Status::Running => {}
        },
        (None, None) => {}
    }
}
