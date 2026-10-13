//! The main panel of the selected item: the big preview, where it is
//! cropped and trimmed, with the small picture of what the result will
//! look like, and its settings beside and below it. What is drawn on the
//! preview itself is `canvas`'s, and its timeline `timeline`'s.

use eframe::egui::{self, Color32, Rect, RichText, pos2, vec2};
use tgradish_core::backend::Backend;
use tgradish_core::config::ResultPreview;
use tgradish_core::convert;
use tgradish_core::options::{Crop, Resize, Scaling};
use tgradish_core::presets::Format;
use tgradish_core::telegram;
use tgradish_core::tgs::TgsOptions;

use crate::app::App;
use crate::canvas::{self, Cropping, Inset, InsetChange, Picture, Screen};
use crate::item::{Framing, Item, Show, Zoom};
use crate::result::{details, summary};
use crate::{predict, settings, theme, timeline, widgets};

/// Width of the column beside the preview, and the least it narrows to
/// before the preview does.
const SIDE: f32 = 300.0;
const NARROWEST_SIDE: f32 = 240.0;
/// The least the preview narrows to.
const NARROWEST_PREVIEW: f32 = 200.0;
/// Side of a `.tgs` result's small picture at its largest: shapes have no
/// pixels, so this is as sharp as it needs.
const TGS_INSET: usize = 256;
/// Height kept for the timeline under a preview that plays: its 30 point
/// track and the space around it.
const TIMELINE: f32 = 38.0;

impl App {
    pub(crate) fn main_panel(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.filter(|&id| self.item(id).is_some()) else {
            self.empty(ui);
            return;
        };
        let extra_args = self.backend().as_ref().is_ok_and(Backend::supports_extra_args);
        let queued = self.queue.iter().position(|&other| other == id);
        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(8.0);
            self.actions(ui, id);
            ui.add_space(6.0);
        });
        let output = self.item(id).map(|item| self.output_for(item));
        let context = settings::Context {
            presets: &self.presets,
            config: &self.config,
            extra_args,
            output,
            config_path: self.config_path.as_deref(),
        };
        let height = ui.available_height();
        let preview_height =
            (height * self.preview_share).clamp(240.0, (height - 140.0).max(240.0));
        let mut in_frames = self.config.gui.timeline_frames;
        let mut look = self.config.gui.result_preview;
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        egui::Panel::top("preview").exact_size(preview_height).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let room = ui.available_width() - 16.0;
                let side = SIDE.min(room - NARROWEST_PREVIEW).max(NARROWEST_SIDE);
                let width = (room - side).max(NARROWEST_PREVIEW);
                ui.allocate_ui_with_layout(
                    vec2(width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let tempo = item.tempo(&self.presets, &self.config);
                        // what a .tgs is made with, the preset's included
                        let tgs = item.choices.tgs_options(&self.presets, &self.config);
                        preview(
                            ui,
                            &mut self.screen,
                            &mut self.inset,
                            item,
                            (tempo, &mut in_frames),
                            &tgs.unwrap_or_default(),
                            &mut look,
                        );
                    },
                );
                ui.add_space(8.0);
                ui.separator();
                ui.allocate_ui_with_layout(
                    vec2(side, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(side);
                        egui::ScrollArea::vertical()
                            .id_salt("side")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_max_width(side - 12.0);
                                ui.add_space(6.0);
                                settings::make(ui, item, &context);
                                settings::plan(ui, item, &context);
                                summary(ui, item, queued);
                                details(ui, item);
                            });
                    },
                );
            });
        });
        // the line under the preview moves it
        let (line, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 8.0), egui::Sense::drag());
        let response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        // never so small the preview is no use, nor so large the settings
        // below lose their first row
        if response.dragged() {
            self.preview_share =
                ((preview_height + response.drag_delta().y) / height).clamp(0.2, 0.85);
        }
        let colour = if response.hovered() || response.dragged() {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().widgets.noninteractive.bg_stroke.color
        };
        ui.painter().hline(line.x_range(), line.center().y, egui::Stroke::new(1.0, colour));
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                settings::show(ui, item, &context);
                ui.add_space(12.0);
            });
        });
        if in_frames != self.config.gui.timeline_frames {
            self.keep("timeline's unit", |gui| gui.timeline_frames = in_frames);
        }
        if look != self.config.gui.result_preview {
            self.keep("result preview's size and place", |gui| gui.result_preview = look);
        }
    }
}

/// The big preview of an item, its header and timeline. The input plays at
/// `tempo`, as the result will.
fn preview(
    ui: &mut egui::Ui,
    screen: &mut Screen,
    inset: &mut Inset,
    item: &mut Item,
    (tempo, in_frames): (f64, &mut bool),
    tgs: &TgsOptions,
    look: &mut ResultPreview,
) {
    let has_result = item.result_clip.ready().is_some();
    // a change to what is used or how it is fitted shows on the input
    let framing = Framing::of(item);
    if item.view.framing.as_ref().is_some_and(|seen| *seen != framing) {
        item.view.show = Show::Input;
    }
    item.view.framing = Some(framing);
    if !has_result {
        item.view.show = Show::Input;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let mut show = item.view.show;
        let choices = [(Show::Input, "Input", ""), (Show::Result, "Result", "")];
        // .tgs results of before a restart, which tgradish can't draw
        let unseen = if item.made.as_ref().is_some_and(|made| made.format == Format::Tgs) {
            "This window can only show .tgs results it made itself; convert again to see it"
        } else {
            "Convert it first"
        };
        widgets::segments(ui, &mut show, &choices, |show| {
            if show == Show::Result && !has_result { Err(unseen) } else { Ok(()) }
        });
        item.view.show = show;
        // where it came from
        let original = ui
            .add_enabled(item.pasted.is_none(), egui::Button::new("Show original"))
            .on_hover_text("The input file, in its folder")
            .on_disabled_hover_text("A pasted image has no file of its own");
        if original.clicked() {
            crate::system::reveal(&item.inputs[0]);
        }
        // what doesn't fit is cut short or left out, the hint first
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if item.view.show == Show::Input {
                let full = "Result preview";
                let room = ui.available_width();
                let text =
                    if room >= widgets::text_width(ui, full, 8.0) { full } else { "Preview" };
                ui.toggle_value(&mut item.view.inset, text)
                    .on_hover_text("What the result will look like, small, beside the input");
            }
            // always there, so nothing beside them moves as zooming starts
            let zoom = item.view.zoom_mut();
            let zoomed = zoom.level > 1.0;
            let fit = ui
                .add_enabled(zoomed, egui::Button::new("Fit"))
                .on_hover_text("Show all of it")
                .on_disabled_hover_text("All of it shows");
            if fit.clicked() {
                *zoom = Zoom::default();
            }
            let level = RichText::new(format!("{:.0}%", zoom.level * 100.0));
            let level = if zoomed { level } else { level.weak() };
            ui.add_sized(vec2(42.0, 18.0), egui::Label::new(level));
            if has_result && !item.result_is_current() {
                let stale = RichText::new("made with earlier settings").color(widgets::WARN);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(stale).truncate());
                });
            } else if ui.available_width() > 60.0 {
                let hint = match (item.view.show, item.choices.crop.is_some()) {
                    (Show::Input, false) => "Drag to crop · wheel to zoom · right button to pan",
                    (Show::Input, true) => "Drag the crop or its edges · double-click for all",
                    (Show::Result, _) => "Wheel to zoom · right button to pan",
                };
                ui.add(egui::Label::new(RichText::new(hint).weak().size(12.5)).truncate());
            }
        });
    });

    let length = item.input_length();
    let result_length =
        item.result_clip.ready().filter(|clip| clip.is_animated()).map(|clip| clip.end());
    let timeline_height = match item.view.show {
        Show::Input if length.is_some() => TIMELINE,
        Show::Result if result_length.is_some() => TIMELINE,
        _ => 0.0,
    };
    let size = ui.available_size() - vec2(0.0, timeline_height + 6.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    // after a stall, playing goes on where it was rather than jumping ahead
    let dt = ui.input(|input| f64::from(input.stable_dt.min(0.1)));
    let range = item.range();
    let input_size = item.input_size();
    let error = item.input_error().map(str::to_owned);
    // fields rather than methods, so the view and crop can change
    let (overview, part, still) = match item.format {
        Format::Webm | Format::Image => (
            item.video.ready().map(|video| &video.overview),
            item.part.as_ref().and_then(|(_, part)| part.ready()),
            item.still.as_ref().and_then(|(at, still)| Some((*at, still.ready()?))),
        ),
        Format::Tgs => (item.art.ready().map(|art| &art.clip), None, None),
    };
    let target = item.choices.target;
    let resize = settings::resize_of(item);
    let filling = (item.format != Format::Tgs && resize == Resize::Crop).then(|| {
        let (width, height) = target.box_size();
        f64::from(width) / f64::from(height)
    });

    match item.view.show {
        Show::Input => {
            if let Some(message) = error {
                message_in(ui, rect, &format!("Couldn't read it: {message}"), widgets::BAD);
            } else if let (Some(clip), Some(size)) = (overview, input_size) {
                // a picture is one frame: shown still, where it is picked
                let picking = item.format == Format::Image;
                if picking {
                    item.view.playing = false;
                    let frame = item.choices.frame.unwrap_or(0.0);
                    item.view.time = length.map_or(0.0, |length| frame.clamp(0.0, length));
                }
                // play within the used part, as fast as the result will
                if let Some((start, end)) = range.filter(|_| !picking) {
                    if item.view.playing {
                        item.view.time += dt * tempo;
                        ui.ctx().request_repaint();
                    }
                    if item.view.time < start || item.view.time >= end {
                        item.view.time = start;
                    }
                }
                let time = item.view.time;
                // its last frame starts at its end, and shows a little past it
                let part = part.filter(|part| part.start() <= time && time < part.end() + 0.05);
                // paused: the sharp frame, once it is there
                // and still images, which don't play, always
                let still =
                    still.filter(|(at, _)| range.is_none() || (!item.view.playing && *at == time));
                let clip = still.map(|(_, still)| still).or(part).unwrap_or(clip);
                let frame = clip.index_at(time);
                let picture = Picture { clip, frame, size, square: false };
                // the result's small picture beside the input, not over it,
                // with room for its longest label, eased into place when it
                // is turned on or off, resized or moved
                let widest = match item.format {
                    Format::Webm | Format::Image => "Result at 100%\n512 × 512 px",
                    Format::Tgs => "Result",
                };
                let text = ui.visuals().text_color();
                let label = ui.painter().layout_no_wrap(widest.into(), canvas::INSET_LABEL, text);
                let step = usize::from(look.size.clamp(1, 5)) - 1;
                let corner = look.corner;
                let (beside, room) = canvas::split(
                    rect,
                    size,
                    canvas::label_box(label.size()),
                    canvas::INSET_SIZES[step],
                    corner,
                );
                let on = item.view.inset;
                // turned off, it shrinks into its corner
                let layout = if on {
                    (beside, room)
                } else {
                    let x = if corner.left() { room.left() } else { room.right() };
                    let y = if corner.top() { room.top() } else { room.bottom() };
                    (rect, Rect::from_min_max(pos2(x, y), pos2(x, y)))
                };
                let (input_area, room, align) =
                    inset.ease(ui.ctx(), item.id, (on, step, corner), layout);
                ui.painter().rect_filled(rect, 0.0, theme::stage(ui.visuals().dark_mode));
                let ratio = settings::ratio(item.view.aspect, size);
                let exact = settings::exact_of(item);
                let cropping = Cropping { crop: &mut item.choices.crop, ratio, filling, exact };
                let zoom = &mut item.view.input_zoom;
                canvas::show(ui, screen, (rect, input_area), &picture, zoom, Some(cropping));
                // after the crop moved
                let showing = on || inset.leaving();
                let mut change = None;
                let place = ((rect, room, align), (corner, step, on));
                if let Some(label) = inset_label(item, size).filter(|_| showing) {
                    let crop = item.choices.crop;
                    let where_ = predict::Part::of(crop, size, (clip.width, clip.height));
                    match item.format {
                        Format::Webm | Format::Image => {
                            let used = crop.map_or(size, |crop| (crop.width, crop.height));
                            let full = Crop { x: 0, y: 0, width: size.0, height: size.1 };
                            // a crop that doesn't suit the exact scale yet is made to
                            // fit on the next frame
                            if let Some(mut sizes) =
                                settings::webm_sizes(item, crop.unwrap_or(full))
                            {
                                let art_key = inset_key((clip.id, where_));
                                let scaling = item.choices.webm.scaling.unwrap_or(Scaling::Auto);
                                if scaling == Scaling::Auto
                                    && sizes.enlarges(used) >= convert::AUTO_SHARP_FROM
                                    && inset.looks_like_art(art_key, || {
                                        predict::looks_like_art(clip, where_)
                                    })
                                {
                                    sizes.scaling = Scaling::Sharp;
                                }
                                let key = inset_key((clip.id, frame, where_, sizes));
                                let result = vec2(sizes.width as f32, sizes.height as f32);
                                let label = label.as_deref();
                                change =
                                    inset.show(ui, place.0, place.1, key, label, result, |most| {
                                        let rgba = &clip.frames[frame];
                                        predict::webm(
                                            rgba,
                                            clip.width,
                                            where_,
                                            &predict::within(&sizes, most),
                                        )
                                    });
                            }
                        }
                        Format::Tgs => {
                            let keep = tgs.keep_canvas.unwrap_or(false);
                            let resize = tgs.resize.unwrap_or(Resize::Pad);
                            let scale = tgs.pixel_scale;
                            let key = inset_key((clip.id, where_, keep, resize, scale));
                            let bounds = inset.tgs_bounds(key, || {
                                let found =
                                    || predict::tgs_bounds(&clip.frames, clip.width, where_, keep);
                                // cut square on the art's grid, as converting does
                                match resize {
                                    Resize::Crop => predict::tgs_square(clip, where_, keep, scale)
                                        .unwrap_or_else(found),
                                    _ => found(),
                                }
                            });
                            let key = inset_key((clip.id, frame, bounds, resize));
                            let label = label.as_deref();
                            let result = vec2(TGS_INSET as f32, TGS_INSET as f32);
                            change = inset.show(ui, place.0, place.1, key, label, result, |most| {
                                let rgba = &clip.frames[frame];
                                predict::tgs(rgba, clip.width, bounds, most, resize)
                            });
                        }
                    }
                }
                match change {
                    Some(InsetChange::Size(step)) => look.size = step as u8 + 1,
                    Some(InsetChange::Corner(corner)) => look.corner = corner,
                    None => {}
                }
            } else {
                message_in(ui, rect, "Reading…", ui.visuals().weak_text_color());
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        Show::Result => {
            if let Some(clip) = item.result_clip.ready() {
                let length = clip.end().max(1e-3);
                if item.view.playing && clip.is_animated() {
                    item.view.result_time = (item.view.result_time + dt).rem_euclid(length);
                    ui.ctx().request_repaint();
                }
                let square = item.made.as_ref().is_some_and(|made| made.format == Format::Tgs);
                let picture = Picture {
                    clip,
                    frame: clip.index_at(item.view.result_time.rem_euclid(length)),
                    size: (clip.width, clip.height),
                    square,
                };
                let zoom = &mut item.view.result_zoom;
                canvas::show(ui, screen, (rect, rect), &picture, zoom, None);
            }
        }
    }

    if timeline_height == 0.0 {
        return;
    }
    ui.add_space(6.0);
    match (item.view.show, length, result_length) {
        (Show::Input, Some(_), _) if item.format == Format::Image => {
            if let Some(frames) = item.input_frames() {
                timeline::frame_picker(ui, &frames, &mut item.choices.frame, in_frames);
            }
        }
        (Show::Input, Some(length), _) => {
            let frames = item.input_frames();
            timeline::show(
                ui,
                timeline::Timeline {
                    length,
                    start: &mut item.choices.start,
                    used: &mut item.choices.length,
                    time: &mut item.view.time,
                    playing: &mut item.view.playing,
                    // in the input's seconds, which the tempo makes 3 of the
                    // result's
                    limit: telegram::MAX_SECONDS * tempo,
                    frames: frames.as_ref(),
                    in_frames,
                },
            )
        }
        (Show::Result, _, Some(length)) => {
            timeline::player(ui, length, &mut item.view.result_time, &mut item.view.playing);
        }
        _ => {}
    }
}

/// What the label of an item's small picture of its result says under
/// "Result": its size, which `.tgs` results, vectors, have none of in
/// pixels. `None` while its size isn't known.
fn inset_label(item: &Item, size: (u32, u32)) -> Option<Option<String>> {
    match item.format {
        Format::Webm | Format::Image => {
            let full = Crop { x: 0, y: 0, width: size.0, height: size.1 };
            let sizes = settings::webm_sizes(item, item.choices.crop.unwrap_or(full))?;
            Some(Some(format!("{} × {} px", sizes.width, sizes.height)))
        }
        Format::Tgs => Some(None),
    }
}

/// A key that changes when any of `parts` does.
fn inset_key(parts: impl std::fmt::Debug) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    format!("{parts:?}").hash(&mut hasher);
    hasher.finish()
}

fn message_in(ui: &egui::Ui, rect: egui::Rect, text: &str, colour: Color32) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::stage(ui.visuals().dark_mode));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(15.0),
        colour,
    );
}
