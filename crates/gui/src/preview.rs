//! Main panel for the selected item: the interactive preview stage (cropping,
//! trimming, and the live result inset), plus the settings column beside it
//! and the settings scroll area below it. Stage rendering lives in `canvas`
//! and the playback/trim bar in `timeline`.

use crate::config::ResultPreview;
use eframe::egui::{self, Color32, Rect, RichText, pos2, vec2};
use tgradish_core::backend::Backend;
use tgradish_core::options::{Crop, Resize, Scaling};
use tgradish_core::picture;
use tgradish_core::presets::Format;
use tgradish_core::telegram;
use tgradish_core::tgs;
use tgradish_core::webm::Options;

use crate::app::App;
use crate::canvas::{self, Cropping, Kept, Picture, Screen};
use crate::inset::{self, Inset, InsetChange};
use crate::item::{Framing, Item, Recipe, Show};
use crate::l10n::fl;
use crate::media::{Clip, Load};
use crate::result::{details, summary};
use crate::{file_settings, predict, theme, timeline, widgets};

/// Default and minimum width of the settings column beside the preview (the
/// side column shrinks down to `NARROWEST_SIDE` before the preview narrows).
const SIDE: f32 = 300.0;
const NARROWEST_SIDE: f32 = 228.0;
/// Minimum preview width needed to fit the timeline row beneath it.
const NARROWEST_PREVIEW: f32 = 366.0;
/// Minimum width of the main panel before the file list must give way: fits
/// both the preview and side column, as well as the widest settings rows below.
pub(crate) const NARROWEST_MAIN: f32 = 656.0;
/// Maximum side in pixels of the `.tgs` result inset preview (vector shapes
/// are resolution-independent, so 256 px is sharp enough).
const TGS_INSET: usize = 256;
/// Vertical spacing between the stage and the timeline row, and between the
/// timeline row and the divider line (centring the row in its space; without a
/// timeline row, the stage ends by the same margin above the line).
const AROUND_ROW: f32 = 12.0;
/// Interactive drag height of the horizontal divider under the preview (drawn
/// through its vertical centre).
const HANDLE: f32 = 8.0;
/// Distance between the inner bottom of the preview top panel and the divider
/// line centre: egui's 2 pt top-panel inner margin plus half of [`HANDLE`].
const UNDER_PANEL: f32 = 2.0 + HANDLE / 2.0;

/// Minimum and maximum fraction of the main panel height that the preview may
/// occupy when dragging the divider (keeps the preview usable and leaves at
/// least one row of settings visible below).
pub(crate) const LEAST_SHARE: f32 = 0.2;
pub(crate) const MOST_SHARE: f32 = 0.85;

impl App {
    pub(crate) fn main_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let selected = self.selected.filter(|&id| self.item(id).is_some());
        // If the currently drawn item has an uncommitted number edit and is
        // being deselected or removed, draw it for one more frame after
        // surrendering focus so its field commits the typed value to the right
        // item before switching.
        let typed = widgets::draft_file(&ctx);
        if typed.is_some_and(|file| self.item(file).is_none()) {
            widgets::discard_draft(&ctx);
        }
        let held = typed.filter(|&file| {
            let leaving = Some(file) != selected || self.removing.contains(&file);
            leaving && Some(file) == self.drawn && self.item(file).is_some()
        });
        if held.is_some() {
            widgets::commit_draft(&ctx);
        }
        self.drawn = held.or(selected);
        let Some(id) = self.drawn else {
            self.drop_zone(ui, ui.max_rect().shrink(28.0));
            return;
        };
        let ffmpeg_args = self.backend().as_ref().is_ok_and(Backend::supports_ffmpeg_args);
        let queued = self.queue.iter().position(|&other| other == id);
        let bar = egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(crate::actions::BAR_ABOVE);
            self.actions(ui, id);
            ui.add_space(crate::actions::below_bar(ui.style()));
        });
        self.banner.bar = Some(bar.response.rect.top());
        let output = self.item(id).map(|item| self.output_for(item));
        let copyable = self.copyable(id, output.as_deref()).is_some();
        let (choice, path) = self.ffmpeg_settings();
        let mut in_frames = self.config.gui.timeline_frames;
        let mut look = self.config.gui.result_preview;
        let context = file_settings::Context {
            presets: &self.presets,
            config: &self.config,
            ffmpeg: tgradish_core::config::FfmpegConfig { choice, path },
            ffmpeg_args,
            output,
            config_path: self.config_path.as_deref(),
            presets_dir: self.presets_dir.as_deref(),
            in_frames,
        };
        let height = ui.available_height();
        let preview_height =
            (height * self.preview_share).clamp(240.0, (height - 140.0).max(240.0));
        let item = self.items.iter_mut().find(|item| item.id == id).expect("selected");
        // Omit the built-in panel separator; the custom drag handle draws the line below.
        let panel = egui::Panel::top("preview").exact_size(preview_height);
        panel.show_separator_line(false).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                // Subtract the gap and vertical separator between the preview and side column.
                let room = ui.available_width() - 30.0;
                let side = SIDE.min(room - NARROWEST_PREVIEW).max(NARROWEST_SIDE);
                let width = (room - side).max(NARROWEST_PREVIEW);
                ui.allocate_ui_with_layout(
                    vec2(width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        let (tempo, webm, tgs) = made_with(item, &context);
                        preview(
                            ui,
                            (&mut self.screen, &mut self.inset),
                            item,
                            (tempo, &mut in_frames),
                            (&webm, &tgs),
                            (&mut look, crate::motion::motion(ui.ctx())),
                        );
                    },
                );
                ui.add_space(8.0);
                ui.separator();
                // Extend to the right edge of the panel so the side column's
                // scroll bar aligns with the lower settings scroll bar.
                let side = ui.available_width().max(side);
                ui.allocate_ui_with_layout(
                    vec2(side, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(side);
                        let column = egui::ScrollArea::vertical()
                            .id_salt(("side", id))
                            .auto_shrink([false, false]);
                        widgets::scrolled(ui, column, |ui| {
                            side_settings(ui, item, &context, (queued, copyable));
                        });
                    },
                );
            });
        });
        // Draggable horizontal divider under the preview.
        let (line, response) = ui
            .allocate_exact_size(vec2(ui.available_width(), HANDLE), egui::Sense::click_and_drag());
        let response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, fl!("preview-divider"))
        });
        // Clamp between LEAST_SHARE and MOST_SHARE; double-click resets to default.
        if response.dragged() {
            self.preview_share = ((preview_height + response.drag_delta().y) / height)
                .clamp(LEAST_SHARE, MOST_SHARE);
        }
        if response.double_clicked() {
            self.preview_share = crate::app::PREVIEW_SHARE;
        }
        // Highlight and thicken the line on hover or drag.
        let line_width = if response.hovered() || response.dragged() { 2.0 } else { 1.0 };
        let colour = if response.hovered() || response.dragged() {
            ui.visuals().widgets.hovered.fg_stroke.color
        } else {
            ui.visuals().widgets.noninteractive.bg_stroke.color
        };
        let stroke = egui::Stroke::new(line_width, colour);
        ui.painter().hline(line.x_range(), line.center().y, stroke);
        egui::CentralPanel::default().show(ui, |ui| {
            // Salt the scroll area with the item ID so each item preserves its scroll offset.
            let settings =
                egui::ScrollArea::vertical().id_salt(("settings", id)).auto_shrink([false, false]);
            widgets::scrolled(ui, settings, |ui| {
                under_settings(ui, item, &context);
            });
        });
        if in_frames != self.config.gui.timeline_frames {
            self.save_setting(|gui| gui.timeline_frames = in_frames);
        }
        if look != self.config.gui.result_preview {
            self.save_setting(|gui| gui.result_preview = look);
        }
    }
}

/// Draws the side column beside the preview: target/format/preset selectors,
/// blockers, conversion progress or result summary/details, and the pre-run
/// plan.
fn side_settings(
    ui: &mut egui::Ui,
    item: &mut Item,
    context: &file_settings::Context,
    (queued, copyable): (Option<usize>, bool),
) {
    widgets::of_file(ui, "side", item.id, |ui| {
        widgets::settings_spacing(ui);
        ui.add_space(6.0);
        file_settings::make(ui, item, context);
        file_settings::blocked(ui, item);
        // What converting did goes above the plan, where the eye is once
        // Convert is pressed; the plan matters most before there is a result.
        summary(ui, item, queued, copyable);
        details(ui, item);
        file_settings::plan(ui, item, context);
    });
}

/// Draws the per-item settings below the preview.
fn under_settings(ui: &mut egui::Ui, item: &mut Item, context: &file_settings::Context) {
    widgets::of_file(ui, "settings", item.id, |ui| {
        widgets::settings_spacing(ui);
        file_settings::show(ui, item, context);
        ui.add_space(12.0);
    });
}

/// Returns `item`'s effective playback tempo and merged WebM and `.tgs`
/// options.
fn made_with(item: &Item, context: &file_settings::Context) -> (f64, Options, tgs::Options) {
    let (presets, config) = (context.presets, context.config);
    let recipe = |format| item.choices.recipe(format, presets, config);
    let tgs = recipe(Format::Tgs).map(Recipe::into_tgs).unwrap_or_default();
    let webm = recipe(Format::Webm).map_or_else(|_| item.choices.webm.clone(), Recipe::into_webm);
    (item.tempo(presets, config), webm, tgs)
}

/// Draws the item preview header, stage, and timeline row. The input plays at
/// `tempo` to match the output speed.
fn preview(
    ui: &mut egui::Ui,
    (screen, inset): (&mut Screen, &mut Inset),
    item: &mut Item,
    (tempo, in_frames): (f64, &mut bool),
    (webm, tgs): (&Options, &tgs::Options),
    (look, smooth): (&mut ResultPreview, bool),
) {
    let has_result = item.result_clip.ready().is_some();
    // Switch back to the input view whenever crop, trim, or fitting changes.
    let framing = Framing::of(item);
    if item.view.framing.as_ref().is_some_and(|seen| *seen != framing) {
        item.view.show = Show::Input;
    }
    item.view.framing = Some(framing);
    // Show the result once decoded, or fall back to the input while loading.
    let shown = item.shown();
    // Align vertically with the header buttons above the file list.
    ui.add_space(9.0);
    // Result preview toggle: placed on the header bar when it fits, or on a
    // second row below when narrow. Remains visible (disabled) while viewing
    // the result so adjacent controls do not shift when switching views.
    let toggle = |ui: &mut egui::Ui, inset: &mut bool, input: bool| {
        ui.add_enabled_ui(input, |ui| ui.toggle_value(inset, fl!("result-preview")))
            .inner
            .on_hover_text(fl!("result-preview", "hint"))
            .on_disabled_hover_text(fl!("result-preview", "input-only"));
    };
    let mut toggle_below = false;
    ui.horizontal(|ui| {
        let mut show = shown;
        // Measure button widths dynamically to accommodate translations.
        let gap = ui.spacing().item_spacing.x;
        let button =
            |text: &str| widgets::text_width(ui, text, 2.0 * ui.spacing().button_padding.x);
        // Reserve width for the outdated result label so the segment group
        // does not shift when a result becomes stale.
        let shows = button(&fl!("show-input")) + 1.0 + button(&fl!("show-result", "outdated"));
        let zoom_out = zoom_out_width(ui);
        let zooming = gap + zoom_out;
        let preview = gap + button(&fl!("result-preview"));
        let room = ui.available_width();
        // In narrow layouts, show the Zoom out button only while zoomed in.
        let compact = shows + zooming + preview > room;
        toggle_below = room < shows + preview;
        // Show the mouse interaction hint only if the longest hint fits beside
        // the widest button states, so it never shifts or flickers as state
        // changes.
        let right = zooming + if toggle_below { 0.0 } else { preview };
        let hint_at = shows + gap + HINT_ROOM;
        let hint_fits = hint_at + widest_mouse_hint(ui) + HINT_ROOM + right <= room;
        let bar_left = ui.cursor().left();
        let outdated = has_result && !item.result_is_current();
        let result = if outdated { fl!("show-result", "outdated") } else { fl!("show-result") };
        let result_hint = if outdated { fl!("show-result", "hint") } else { String::new() };
        let choices =
            [(Show::Input, fl!("show-input"), String::new()), (Show::Result, result, result_hint)];
        // If decoding the result failed, show the decode error on the disabled
        // Result segment tooltip.
        let unseen = match &item.result_clip {
            Load::Failed(why) if item.made.is_some() => why.clone(),
            _ => fl!("convert-first"),
        };
        if widgets::segments(ui, &mut show, &choices, |show| {
            if show == Show::Result && !has_result { Err(unseen.clone()) } else { Ok(()) }
        }) {
            item.view.show = show;
        }
        // Leave out what does not fit.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !toggle_below {
                toggle(ui, &mut item.view.inset, show == Show::Input);
            }
            // Keep Zoom out visible whenever there is room so adjacent controls
            // do not move when zooming begins.
            let zoom = item.view.zoom_mut(show);
            // Disable Zoom out while an ease-out animation is already running.
            let zoomed = zoom.level > 1.0 && zoom.easing.is_none();
            if zoomed || !compact {
                // Show the current pixel scale on the fixed-width Zoom out
                // button so changing zoom level does not shift the layout.
                let scale = zoom.scale.map(inset::scale_text).unwrap_or_default();
                let button = egui::Button::new(fl!("zoom-out"))
                    .shortcut_text(scale)
                    .gap(ZOOM_GAP)
                    .min_size(vec2(zoom_out, 0.0));
                let out = ui
                    .scope(|ui| {
                        ui.spacing_mut().button_padding.x = ZOOM_PADDING;
                        ui.add_enabled(zoomed, button)
                    })
                    .inner
                    .on_hover_text(fl!("zoom-out", "hint"))
                    .on_disabled_hover_text(fl!("zoom-out", "all"));
                if out.clicked() {
                    zoom.zoom_out(ui.input(|input| input.time), smooth);
                }
            }
        });
        if hint_fits {
            // Anchor the hint after the widest Result segment so the outdated
            // indicator does not shift it.
            let row = ui.min_rect().y_range();
            let at = pos2(bar_left + hint_at, row.min);
            let place = Rect::from_min_size(at, vec2(room - hint_at - right, row.span()));
            let text = RichText::new(mouse_hint(show, item.choices.crop.is_some()));
            let label = egui::Label::new(text.font(HINT_FONT).weak()).extend().selectable(false);
            let layout = egui::Layout::left_to_right(egui::Align::Center);
            ui.scope_builder(egui::UiBuilder::new().max_rect(place).layout(layout), |ui| {
                ui.add(label);
            });
        }
    });
    let shown = item.shown();
    if toggle_below {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            toggle(ui, &mut item.view.inset, shown == Show::Input);
        });
    }

    let length = item.input_length();
    let row_height = row_height(ui, item, shown, ui.available_width());
    let row_room = if row_height > 0.0 { row_height + AROUND_ROW } else { 0.0 };
    let size = ui.available_size() - vec2(0.0, row_room + AROUND_ROW - UNDER_PANEL);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    // Cap frame delta at 0.1 s so playback resumes smoothly after a stall
    // instead of jumping ahead.
    let dt = ui.input(|input| f64::from(input.stable_dt.min(0.1)));
    let range = item.range();
    let input_size = item.input_size();
    // Strip any duplicate "Couldn't read <file>: " prefix (as when ffprobe
    // fails on one of the inputs) so the filename is not repeated inside the
    // preview error banner.
    let error = item.input_error().map(|message| {
        let said = item.inputs.iter().filter_map(|path| path.file_name()).find_map(|name| {
            let name = name.to_string_lossy();
            message.strip_prefix(&fl!("unreadable-file", name = name, details = ""))
        });
        said.unwrap_or(message).to_owned()
    });
    // Borrow fields directly so `item.view` and `item.choices.crop` remain
    // mutably accessible below.
    let (overview, part, still) = match item.format {
        Format::Webm | Format::Webp => (
            item.video.ready().map(|video| &video.overview),
            item.part.as_ref().and_then(|(_, part)| part.ready()),
            item.still.as_ref().and_then(|(at, still)| Some((*at, still.ready()?))),
        ),
        Format::Tgs => (item.art.ready().map(|art| &art.clip), None, None),
    };
    let target = item.choices.target;
    let resize = file_settings::resize_of(webm);
    let filling = (item.format != Format::Tgs && resize == Resize::Crop).then(|| {
        let (width, height) = target.box_size();
        f64::from(width) / f64::from(height)
    });

    match shown {
        Show::Input => {
            // When ffmpeg is missing, show a neutral notice rather than a
            // file error.
            if error.is_some() && item.blockers.contains(&crate::item::Blocker::NeedsFfmpeg) {
                message_in(ui, rect, &fl!("needs-ffmpeg"), ui.visuals().weak_text_color());
            } else if let Some(message) = error {
                message_in(ui, rect, &fl!("unreadable", error = message), theme::bad(ui.visuals()));
            } else if let (Some(clip), Some(size)) = (overview, input_size) {
                // Static WebP output picks a single frame: pause playback at
                // the chosen timestamp.
                let picking = item.format == Format::Webp;
                if picking {
                    item.view.playing = false;
                    let frame = item.choices.frame.unwrap_or(0.0);
                    item.view.time = length.map_or(0.0, |length| frame.clamp(0.0, length));
                }
                // Loop playback within the trimmed range at the effective
                // output tempo.
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
                // Keep using `part` slightly past `part.end()` so its final
                // frame remains visible.
                let part = part.filter(|part| part.start() <= time && time < part.end() + 0.05);
                // Use the full-resolution `still` frame when paused at `time`
                // (or for still images).
                let still =
                    still.filter(|(at, _)| range.is_none() || (!item.view.playing && *at == time));
                let clip = still.map(|(_, still)| still).or(part).unwrap_or(clip);
                let frame = clip.index_at(time);
                let picture = Picture { clip, frame, size, square: false };
                // Reserve space for the result inset beside the input (sized for its
                // widest label) and ease between layouts when toggled, resized, or moved.
                let widest = inset::widest_label(item.format != Format::Tgs);
                let text = ui.visuals().text_color();
                let label = ui.painter().layout_no_wrap(widest, inset::INSET_LABEL, text);
                // User-facing size steps are 1-based.
                let largest = inset::INSET_SIZES.len() as u8;
                let step = usize::from(look.size.clamp(1, largest)) - 1;
                let corner = look.corner;
                let (beside, room) = inset::split(
                    rect,
                    size,
                    inset::label_box(label.size()),
                    inset::INSET_SIZES[step],
                    corner,
                );
                let on = item.view.inset;
                // When turned off, collapse the inset into its chosen corner.
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
                let ratio = file_settings::ratio(item.view.aspect, size);
                let exact = file_settings::exact_of(webm).filter(|_| item.format != Format::Tgs);
                // For `.tgs` with Crop resize, shade the region outside the
                // central square of the content bounds.
                let kept = match item.format {
                    Format::Tgs if tgs.resize == Some(Resize::Crop) => {
                        let where_ =
                            predict::Part::of(item.choices.crop, size, (clip.width, clip.height));
                        let [x0, y0, x1, y1] =
                            predict::middle_square(tgs_bounds(inset, clip, where_, tgs));
                        let across = |x: usize| x * size.0 as usize / clip.width.max(1) as usize;
                        let down = |y: usize| y * size.1 as usize / clip.height.max(1) as usize;
                        Some(Kept::Part([across(x0), down(y0), across(x1), down(y1)]))
                    }
                    _ => filling.map(Kept::Ratio),
                };
                let crop = &mut item.choices.crop;
                let cropping = Cropping { file: item.id, crop, ratio, kept, exact };
                let zoom = &mut item.view.input_zoom;
                // Scope by item ID so a drag started on one item's canvas stops
                // if another item replaces it mid-drag (e.g. via Remove/Undo).
                ui.push_id(item.id, |ui| {
                    canvas::show(ui, screen, (rect, input_area), &picture, zoom, Some(cropping));
                });
                // Render the result inset after any crop drag update.
                let showing = on || inset.leaving();
                let mut change = None;
                let place = ((rect, room, align), (corner, step, on));
                if let Some(label) = inset_label(item, size, webm).filter(|_| showing) {
                    let crop = item.choices.crop;
                    let where_ = predict::Part::of(crop, size, (clip.width, clip.height));
                    match item.format {
                        Format::Webm | Format::Webp => {
                            let used = crop.map_or(size, |crop| (crop.width, crop.height));
                            let full = Crop::whole(size);
                            // If the crop does not yet satisfy the integer
                            // scale, it will be adjusted next frame.
                            if let Some(mut sizes) =
                                file_settings::webm_sizes(item.format, webm, crop.unwrap_or(full))
                            {
                                let start = webm.start.unwrap_or(0.0);
                                let art_key = inset_key((clip.id, where_, start.to_bits()));
                                let looks = || {
                                    inset.looks_like_art(art_key, || {
                                        predict::looks_like_art(clip, where_, start)
                                    })
                                };
                                if picture::turns_sharp(webm, sizes.enlarges(used), looks) {
                                    sizes.scaling = Scaling::Sharp;
                                }
                                let key = inset_key((clip.id, frame, where_, sizes));
                                let result = vec2(sizes.width as f32, sizes.height as f32);
                                let label = label.as_deref();
                                change =
                                    inset.show(ui, place.0, place.1, key, label, result, |most| {
                                        let rgba = &clip.frames[frame];
                                        predict::webm(
                                            item.format,
                                            rgba,
                                            clip.width,
                                            where_,
                                            &predict::within(&sizes, most),
                                        )
                                    });
                            }
                        }
                        Format::Tgs => {
                            let resize = tgs.resize_or_default();
                            let bounds = tgs_bounds(inset, clip, where_, tgs);
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
                // Background loader requests a repaint when ready.
                message_in(ui, rect, &fl!("reading"), ui.visuals().weak_text_color());
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
    if row_height > 0.0 {
        let row = row_place(ui, item, shown, rect);
        timeline_row(ui, row, item, shown, (tempo, in_frames), (webm, tgs));
    }
}

/// Returns the height of the timeline/player row beneath the stage for `item`
/// in view `shown` at `width`, or `0.0` for static images without a timeline.
fn row_height(ui: &egui::Ui, item: &Item, shown: Show, width: f32) -> f32 {
    let result_length = item.result_clip.ready().filter(|clip| clip.is_animated());
    match shown {
        Show::Input if item.input_length().is_some() => {
            let picker = item.format == Format::Webp;
            theme::CONTROL + timeline::extra_height(ui, width, picker)
        }
        Show::Result if result_length.is_some() => theme::CONTROL,
        _ => 0.0,
    }
}

/// Computes the rectangle for the timeline row directly below `stage` (anchored
/// to `stage` so overlays drawn on the stage cannot shift the cursor).
fn row_place(ui: &egui::Ui, item: &Item, shown: Show, stage: Rect) -> Rect {
    let height = row_height(ui, item, shown, stage.width());
    Rect::from_min_size(
        pos2(stage.left(), stage.bottom() + AROUND_ROW),
        vec2(stage.width(), height),
    )
}

/// Draws the row beneath the stage inside `row`: the input timeline, frame
/// picker, or result player depending on `shown`.
fn timeline_row(
    ui: &mut egui::Ui,
    row: Rect,
    item: &mut Item,
    shown: Show,
    (tempo, in_frames): (f64, &mut bool),
    (webm, tgs): (&Options, &tgs::Options),
) {
    let length = item.input_length();
    let result_length =
        item.result_clip.ready().filter(|clip| clip.is_animated()).map(|clip| clip.end());
    let layout = egui::Layout::top_down(egui::Align::Min);
    let place = egui::UiBuilder::new().max_rect(row).layout(layout);
    widgets::of_file_in(ui, place, "row", item.id, |ui| {
        match (shown, length, result_length) {
            (Show::Input, Some(_), _) if item.format == Format::Webp => {
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
                        // Convert Telegram's 3 s output limit into input seconds at `tempo`.
                        limit: telegram::MAX_SECONDS * tempo,
                        past: timeline::PastLimit::of(item.format, webm, tgs),
                        frames: frames.as_deref(),
                        in_frames,
                    },
                )
            }
            (Show::Result, _, Some(length)) => {
                timeline::player(ui, length, &mut item.view.result_time, &mut item.view.playing);
            }
            _ => {}
        }
    });
}

/// Horizontal padding on either side of the mouse interaction hint in the
/// preview header bar.
const HINT_ROOM: f32 = 12.0;
/// Font size for the mouse interaction hint (slightly smaller than bar buttons).
const HINT_FONT: egui::FontId = egui::FontId::proportional(12.5);

/// Returns the mouse interaction hint for the current view and crop state.
fn mouse_hint(show: Show, cropped: bool) -> String {
    match (show, cropped) {
        (Show::Input, false) => fl!("input-hint"),
        (Show::Input, true) => fl!("crop-hint"),
        (Show::Result, _) => fl!("result-hint"),
    }
}

/// Returns the pixel width of the longest mouse interaction hint.
fn widest_mouse_hint(ui: &egui::Ui) -> f32 {
    let states = [(Show::Input, false), (Show::Input, true), (Show::Result, false)];
    let width = |(show, cropped)| {
        let galley =
            ui.painter().layout_no_wrap(mouse_hint(show, cropped), HINT_FONT, Color32::WHITE);
        galley.size().x
    };
    states.into_iter().map(width).fold(0.0, f32::max).ceil()
}

/// Gap and horizontal padding inside the Zoom out button (slightly tighter
/// than standard buttons so the header's words fit whole at
/// [`crate::MIN_WIDTH`]).
const ZOOM_GAP: f32 = 2.0;
const ZOOM_PADDING: f32 = 6.0;

/// Returns a fixed width for the Zoom out button large enough for its widest
/// scale readout (either percentage or multiplier), preventing layout shifts
/// as zoom changes.
fn zoom_out_width(ui: &egui::Ui) -> f32 {
    let widest = [inset::scale_text(1.99), inset::scale_text(canvas::MOST_TIMES)];
    let widest = widest.map(|text| widgets::text_width(ui, &widgets::zeros(&text), 0.0));
    // The gap sits on either side of the space between the two labels.
    let padding = 2.0 * (ZOOM_PADDING + ZOOM_GAP);
    let words = widgets::text_width(ui, &fl!("zoom-out"), padding);
    (words + widest[0].max(widest[1])).ceil()
}

/// Returns the bounding box of `clip` used by a `.tgs` result before squaring:
/// `part`, trimmed to visible pixels across the selected time range unless
/// `keep_canvas` is enabled.
fn tgs_bounds(
    inset: &mut Inset,
    clip: &Clip,
    part: predict::Part,
    tgs: &tgs::Options,
) -> [usize; 4] {
    let keep = tgs.keep_canvas_or_default();
    // Include every frame overlapping the trimmed [start, end) range, matching
    // normalisation.
    let start = tgs.start.unwrap_or(0.0);
    let end = tgs.length.map_or(f64::INFINITY, |length| start + length);
    let spans = clip.times.windows(2).take(clip.frames.len());
    let first = spans.clone().position(|span| span[1] > start).unwrap_or(0);
    let after = spans.filter(|span| span[0] < end).count().max(first + 1);
    let used = &clip.frames[first..after.min(clip.frames.len())];
    let key = inset_key((clip.id, part, keep, first, after));
    inset.tgs_bounds(key, || predict::tgs_bounds(used, clip.width, part, keep))
}

/// Returns the dimension sub-label shown beneath "Result" in the result inset
/// (`Some(None)` for resolution-independent `.tgs` vectors, or `None` when the
/// crop is out of bounds or dimensions are not yet known).
fn inset_label(item: &Item, size: (u32, u32), webm: &Options) -> Option<Option<String>> {
    if item.choices.crop.is_some_and(|crop| !crop.fits(size.0, size.1)) {
        return None;
    }
    match item.format {
        Format::Webm | Format::Webp => {
            let full = Crop::whole(size);
            let sizes =
                file_settings::webm_sizes(item.format, webm, item.choices.crop.unwrap_or(full))?;
            Some(Some(format!("{} × {} px", sizes.width, sizes.height)))
        }
        Format::Tgs => Some(None),
    }
}

/// Hashes `parts` via `Debug` into a cache key for the result inset.
fn inset_key(parts: impl std::fmt::Debug) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    format!("{parts:?}").hash(&mut hasher);
    hasher.finish()
}

fn message_in(ui: &egui::Ui, rect: egui::Rect, text: &str, colour: Color32) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::stage(ui.visuals().dark_mode));
    // Wrap long error messages to fit within the preview stage width.
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        egui::FontId::proportional(15.0),
        colour,
        (rect.width() - 32.0).max(80.0),
    );
    job.halign = egui::Align::Center;
    let galley = painter.layout_job(job);
    let at = rect.center() - galley.rect.center().to_vec2();
    painter.galley(at, galley, colour);
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use tgradish_core::presets::Presets;

    use super::*;
    use crate::app::App;
    use crate::testing::{finish, square};

    #[test]
    fn shows_inputs_larger_than_a_texture_can_be() {
        // a strip of sprites, read as pixel art, and a picture of many
        // colours, shown sharp at its full size once paused, each wider
        // than the renderer takes, which is 2048 in tests
        let dir = tempfile::tempdir().unwrap();
        let strip = dir.path().join("strip.png");
        image::RgbaImage::from_pixel(3000, 16, image::Rgba([200, 40, 90, 255]))
            .save(&strip)
            .unwrap();
        let photo = dir.path().join("photo.png");
        image::RgbaImage::from_fn(2400, 300, |x, y| {
            image::Rgba([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8, 255])
        })
        .save(&photo)
        .unwrap();
        let mut harness = crate::testing::harness(Config::default());
        widgets::test_fonts(&harness);
        harness.state_mut().add(vec![strip, photo]);
        finish(&mut harness);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        for id in ids {
            harness.state_mut().select(id);
            finish(&mut harness);
            harness.run_steps(4);
            let item = harness.state().item(id).unwrap();
            assert!(item.input_size().is_some_and(|size| size.0 > 2048), "{:?}", item.name());
            // what shows it, the clip read or the still at its full size,
            // no larger than the renderer takes
            let most = harness.ctx.input(|input| input.max_texture_side);
            let texture = harness.state().screen.texture_size().unwrap();
            assert!(texture[0] <= most && texture[1] <= most, "{:?}: {texture:?}", item.name());
            assert_eq!(texture[0], most, "{:?}", item.name());
        }
    }

    #[test]
    fn the_narrowest_window_holds_the_preview_and_column() {
        let dir = tempfile::tempdir().unwrap();
        let frames = dir.path().join("walk");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [255, 0, 0, 255]);
        square(&frames.join("2.png"), [0, 0, 255, 255]);
        // in the GUI's fonts and style, as its words take their room
        let mut harness =
            Harness::builder().with_size([crate::MIN_WIDTH, 640.0]).build_eframe(|cc| {
                let app = App::with(Config::default(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
        harness.state_mut().add(vec![frames]);
        finish(&mut harness);
        let column = harness.get_by_label(&fl!("make")).accesskit_node().bounding_box().unwrap();
        let row = harness.get_by_label(&fl!("in-frames")).accesskit_node().bounding_box().unwrap();
        // the column's whole width, up to where the panel's margin clips it
        assert!(
            column.x0 + f64::from(NARROWEST_SIDE) <= f64::from(crate::MIN_WIDTH) - 8.0,
            "{column:?}"
        );
        // and the row under the preview stops short of it
        assert!(row.x1 < column.x0, "{row:?} runs into {column:?}");
        // the unit's segments under the bar, rather than leaving it narrow,
        // ending where the field before them does
        let play =
            harness.get_by_label(&fl!("play", "pause")).accesskit_node().bounding_box().unwrap();
        assert!(row.y0 >= play.y1, "{row:?} beside {play:?}");
        let spin = egui_kittest::kittest::by().role(egui::accesskit::Role::SpinButton);
        let fields: Vec<_> = harness
            .query_all(spin)
            .map(|node| node.accesskit_node().bounding_box().unwrap())
            .filter(|field| (field.y0 - play.y0).abs() < 1.0)
            .collect();
        let [start, end] = fields[..] else { panic!("{fields:?}") };
        assert!(end.x0 - start.x1 >= 160.0, "{start:?} to {end:?}");
        assert!((row.x1 - end.x1).abs() < 1.0, "{row:?} under {end:?}");
    }

    #[test]
    fn lines_up_the_bars_of_the_column_and_the_settings_with_the_edge() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        // short enough that the column and the settings both scroll
        let mut harness =
            Harness::builder().with_size([crate::MIN_WIDTH, 560.0]).build_eframe(|cc| {
                let app = App::with(Config::default(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
        harness.state_mut().add(vec![art]);
        finish(&mut harness);
        let bar = egui_kittest::kittest::by().role(egui::accesskit::Role::ScrollBar);
        let bars: Vec<_> = harness
            .query_all(bar)
            .map(|node| node.accesskit_node().bounding_box().unwrap())
            .filter(|bar| bar.x0 > f64::from(crate::MIN_WIDTH) / 2.0)
            .collect();
        let [first, second] = bars[..] else { panic!("{bars:?}") };
        // one above the other, at the edge of the panel within the
        // window's margin and its own, where the buttons under them end
        assert!((first.x1 - second.x1).abs() < 0.5, "{first:?} beside {second:?}");
        let edge = f64::from(crate::MIN_WIDTH) - 16.0;
        assert!((first.x1 - edge).abs() < 0.5, "{first:?} short of {edge}");
    }

    /// Its words whole in British English; another language's, often
    /// longer, may be cut short in a narrow window.
    #[test]
    fn keeps_the_english_bar_whole_and_still_as_zoom_and_result_change() {
        use tgradish_core::options::Crop;
        crate::l10n::english();
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        for width in [crate::MIN_WIDTH, 1280.0, 1920.0] {
            let mut harness = Harness::builder().with_size([width, 820.0]).build_eframe(|cc| {
                let app = App::with(Config::default(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
            harness.state_mut().add(vec![art.clone()]);
            finish(&mut harness);
            crate::testing::convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            harness.state_mut().items[0].view.show = Show::Input;
            harness.state_mut().items[0].choices.crop =
                Some(Crop { x: 1, y: 1, width: 2, height: 2 });
            harness.run_steps(2);
            let place = |harness: &Harness<App>| {
                let out = harness.get_by_label_contains(&fl!("zoom-out")).rect();
                let preview = harness.get_by_label(&fl!("result-preview")).rect();
                let (result, outdated) = (fl!("show-result"), fl!("show-result", "outdated"));
                let result = harness.query_by_label(&result);
                let result = result.or_else(|| harness.query_by_label(&outdated)).unwrap().rect();
                (out, preview, result)
            };
            let before = place(&harness);
            // zoomed in as far as it goes, and out of date
            harness.state_mut().items[0].view.input_zoom.level = f32::MAX;
            harness.state_mut().items[0].choices.tgs.keep_canvas = Some(true);
            harness.run_steps(2);
            harness.get_by_label(&fl!("show-result", "outdated"));
            let out = fl!("zoom-out");
            let times = harness.get_by_label_contains(&out).accesskit_node().label();
            let times = times.unwrap_or_default();
            assert!(times.ends_with(&fl!("inset-scale", "times", times = "")), "{times}");
            // the buttons at the right stay, and the segments at the left
            let (out, preview, result) = place(&harness);
            assert_eq!((out, preview), (before.0, before.1), "{width}");
            assert_eq!(result.left(), before.2.left(), "{width}");
            // nothing cut short, and no way to the folders over the preview
            for shape in shapes(&harness) {
                if let egui::Shape::Text(text) = &shape {
                    let said = text.galley.text();
                    assert!(!text.galley.elided, "{width}: {said:?} cut short");
                }
            }
            assert!(
                harness
                    .query_by_label(&fl!("show-in-folder"))
                    .is_none_or(|button| { button.rect().top() > out.bottom() + 200.0 })
            );
        }
    }

    /// Every shape drawn, those drawn in lists of shapes too.
    fn shapes(harness: &Harness<App>) -> Vec<egui::Shape> {
        fn flatten(shape: egui::Shape, into: &mut Vec<egui::Shape>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.into_iter().for_each(|s| flatten(s, into)),
                shape => into.push(shape),
            }
        }
        let mut all = Vec::new();
        for clipped in &harness.output().shapes {
            flatten(clipped.shape.clone(), &mut all);
        }
        all
    }

    #[test]
    fn puts_the_row_under_the_preview_in_the_middle_of_its_room() {
        use tgradish_core::options::Crop;
        let dir = tempfile::tempdir().unwrap();
        let frames = dir.path().join("walk");
        std::fs::create_dir(&frames).unwrap();
        square(&frames.join("1.png"), [255, 0, 0, 255]);
        square(&frames.join("2.png"), [0, 0, 255, 255]);
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).build_eframe(|cc| {
            let app = App::with(Config::default(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
        harness.state_mut().add(vec![frames]);
        finish(&mut harness);
        crate::testing::convert(&harness);
        harness.run_steps(2);
        finish(&mut harness);
        let first = [fl!("play"), fl!("frame-before")];
        type Case = (&'static str, fn(&mut Item));
        let cases: [Case; 4] = [
            ("the result's player", |item| item.view.show = Show::Result),
            ("the timeline", |item| item.view.show = Show::Input),
            // the crop's tag over the stage
            ("the timeline, cropped", |item| {
                item.choices.crop = Some(Crop { x: 1, y: 1, width: 2, height: 2 });
            }),
            ("the frame picker", |item| item.format = Format::Webp),
        ];
        for (case, set) in cases {
            set(&mut harness.state_mut().items[0]);
            harness.state_mut().items[0].view.playing = false;
            finish(&mut harness);
            harness.run_steps(2);
            let row = harness
                .query_all(egui_kittest::kittest::by())
                .find(|node| {
                    node.accesskit_node().label().is_some_and(|label| first.contains(&label))
                })
                .unwrap_or_else(|| panic!("{case}: no row"))
                .rect();
            let shapes = shapes(&harness);
            let stage = [theme::stage(true), theme::stage(false)];
            let stage = shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Rect(rect) if stage.contains(&rect.fill) => Some(rect.rect),
                    _ => None,
                })
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .unwrap();
            // the line under the preview, across all of it
            let line = shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::LineSegment { points: [a, b], .. }
                        if a.y == b.y && (b.x - a.x).abs() >= stage.width() =>
                    {
                        Some(a.y)
                    }
                    _ => None,
                })
                .filter(|&y| y > stage.bottom())
                .min_by(f32::total_cmp)
                .unwrap();
            let (above, below) = (row.top() - stage.bottom(), line - row.bottom());
            assert!((above - below).abs() <= 0.5, "{case}: {above} over it, {below} under it");
            // from one end of the preview to the other
            let end = harness
                .query_all(egui_kittest::kittest::by())
                .filter(|node| {
                    node.accesskit_node().role() != egui::accesskit::Role::GenericContainer
                })
                .map(|node| node.rect())
                .filter(|rect| {
                    row.y_range().contains(rect.center().y) && rect.left() < stage.right()
                })
                .map(|rect| rect.right())
                .fold(f32::MIN, f32::max);
            assert_eq!(row.left(), stage.left(), "{case}");
            assert!((end - stage.right()).abs() <= 1.0, "{case}: ends at {end}, not {stage:?}");
            // the track and what is drawn on it, in line with the buttons
            // and fields beside it
            let tracks: Vec<Rect> = shapes
                .iter()
                .filter_map(|shape| match shape {
                    egui::Shape::Rect(rect) => Some(rect.rect),
                    _ => None,
                })
                .filter(|rect| {
                    let within = row.top() <= rect.top() && rect.bottom() <= row.bottom();
                    within && rect.left() > row.right() && rect.height() < row.height() - 4.0
                })
                .collect();
            assert!(!tracks.is_empty(), "{case}");
            for track in tracks {
                let off = track.center().y - row.center().y;
                assert!(off.abs() <= 0.5, "{case}: {track:?} beside {row:?}");
            }
        }
    }

    /// What the mouse does, in the bar over the preview where it fits
    /// whole, in the same place whatever is shown, cropped or zoomed; not at
    /// all where it doesn't; and never over the preview.
    #[test]
    fn says_what_the_mouse_does_in_the_bar_where_it_fits_whole() {
        use tgradish_core::options::Crop;
        crate::l10n::english();
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        let hints = [fl!("input-hint"), fl!("crop-hint"), fl!("result-hint")];
        for width in [crate::MIN_WIDTH, 1280.0, 1920.0] {
            let mut harness = Harness::builder().with_size([width, 820.0]).build_eframe(|cc| {
                let app = App::with(Config::default(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
            harness.state_mut().add(vec![art.clone()]);
            finish(&mut harness);
            crate::testing::convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            // the pointer over the preview, where it was once said
            let input = harness.get_by_label(&fl!("show-input")).rect();
            harness.event(egui::Event::PointerMoved(input.left_bottom() + vec2(100.0, 120.0)));
            type Case = (&'static str, fn(&mut Item));
            let cases: [Case; 6] = [
                ("the input", |item| item.view.show = Show::Input),
                ("zoomed in", |item| item.view.input_zoom.level = 2.0),
                ("zoomed in further", |item| item.view.input_zoom.level = f32::MAX),
                ("cropped", |item| {
                    item.choices.crop = Some(Crop { x: 1, y: 1, width: 2, height: 2 })
                }),
                ("out of date", |item| item.choices.tgs.keep_canvas = Some(true)),
                ("the result", |item| item.view.show = Show::Result),
            ];
            let mut seen = Vec::new();
            for (case, set) in cases {
                set(&mut harness.state_mut().items[0]);
                harness.run_steps(3);
                let bar = harness.get_by_label_contains(&fl!("zoom-out")).rect();
                let segments = harness.get_by_label(&fl!("show-input")).rect();
                let texts: Vec<_> = shapes(&harness)
                    .into_iter()
                    .filter_map(|shape| match shape {
                        egui::Shape::Text(text) => Some(text),
                        _ => None,
                    })
                    .collect();
                // none of it cut short, nor said anywhere but in the bar
                for text in &texts {
                    let said = text.galley.text().trim_end_matches('…');
                    let place = text.galley.rect.translate(text.pos.to_vec2());
                    if !said.is_empty() && hints.iter().any(|hint| hint.starts_with(said)) {
                        assert_eq!(said, text.galley.text(), "{width}, {case}: cut short");
                        assert!(!text.galley.elided, "{width}, {case}: {said:?} cut short");
                        assert!(place.bottom() <= bar.bottom() + 1.0, "{width}, {case}: {place:?}");
                    }
                }
                let hint = texts
                    .iter()
                    .filter(|text| hints.iter().any(|hint| hint == text.galley.text()))
                    .map(|text| text.galley.rect.translate(text.pos.to_vec2()))
                    .collect::<Vec<_>>();
                assert!(hint.len() <= 1, "{width}, {case}: {hint:?}");
                if let Some(hint) = hint.first() {
                    // between the segments and Zoom out
                    assert!(hint.left() > segments.right() && hint.right() < bar.left(), "{width}");
                    assert!(bar.y_range().contains(hint.center().y), "{width}, {case}");
                }
                seen.push(hint.first().map(|hint| hint.left()));
            }
            // there in full where there is room, as in a wide window, and
            // in one place, or never there
            if width >= 1920.0 {
                assert!(seen.iter().all(Option::is_some), "{width}: {seen:?}");
            }
            if width <= crate::MIN_WIDTH {
                assert!(seen.iter().all(Option::is_none), "{width}: {seen:?}");
            }
            assert!(seen.windows(2).all(|pair| pair[0] == pair[1]), "{width}: {seen:?}");
        }
    }

    #[test]
    fn offers_the_whole_picture_over_the_preview_too() {
        use tgradish_core::options::Crop;
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        // at the narrowest, where there is least room for it
        for width in [crate::MIN_WIDTH, 1280.0] {
            let mut harness = Harness::builder()
                .with_size([width, 820.0])
                .build_eframe(|_| App::with(Config::default(), None, Presets::builtin()));
            harness.state_mut().add(vec![art.clone()]);
            finish(&mut harness);
            let whole = fl!("whole-picture");
            let buttons = |harness: &Harness<App>| {
                let all: Vec<_> = harness.query_all_by_label(&whole).collect();
                let enabled =
                    all.iter().filter(|node| !node.accesskit_node().is_disabled()).count();
                (all.len(), enabled)
            };
            // without a crop, only the one under the crop's fields, greyed out
            assert_eq!(buttons(&harness), (1, 0), "{width}");
            let zoom =
                |harness: &Harness<App>| harness.get_by_label_contains(&fl!("zoom-out")).rect();
            let before = zoom(&harness);
            harness.state_mut().items[0].choices.crop =
                Some(Crop { x: 1, y: 1, width: 2, height: 2 });
            harness.run_steps(2);
            // with one, over the preview too, nothing else in the bar moved
            assert_eq!(buttons(&harness), (2, 2), "{width}");
            assert_eq!(zoom(&harness), before, "{width}");
            // the one over the preview, the first, takes the crop away, and
            // the one under the fields follows
            let first = harness
                .query_all_by_label(&whole)
                .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()));
            first.unwrap().click();
            harness.run_steps(2);
            assert_eq!(harness.state().items[0].choices.crop, None, "{width}");
            assert_eq!(buttons(&harness), (1, 0), "{width}");
            // and the one under the fields does as it did
            harness.state_mut().items[0].choices.crop =
                Some(Crop { x: 1, y: 1, width: 2, height: 2 });
            harness.run_steps(2);
            let last = harness
                .query_all_by_label(&whole)
                .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()));
            last.unwrap().click();
            harness.run_steps(2);
            assert_eq!(harness.state().items[0].choices.crop, None, "{width}");
        }
    }

    #[test]
    fn keeps_the_result_previews_toggle_greyed_while_the_result_shows() {
        let dir = tempfile::tempdir().unwrap();
        let art = dir.path().join("art.png");
        square(&art, [255, 0, 0, 255]);
        // where it is beside the rest, and where it goes under the bar
        for width in [1280.0, crate::MIN_WIDTH] {
            let mut harness = Harness::builder().with_size([width, 820.0]).build_eframe(|cc| {
                let app = App::with(Config::default(), None, Presets::builtin());
                app.style(&cc.egui_ctx);
                app
            });
            harness.state_mut().add(vec![art.clone()]);
            finish(&mut harness);
            crate::testing::convert(&harness);
            harness.run_steps(2);
            finish(&mut harness);
            let label = fl!("result-preview");
            let place = |harness: &Harness<App>| {
                let toggle = harness.get_by_label(&label);
                let stage = harness.get_by_label(&fl!("show-input")).rect();
                (toggle.rect(), toggle.accesskit_node().is_disabled(), stage)
            };
            harness.state_mut().items[0].view.show = Show::Input;
            harness.run_steps(2);
            let (input, disabled, stage) = place(&harness);
            assert!(!disabled, "{width}");
            harness.state_mut().items[0].view.show = Show::Result;
            harness.run_steps(2);
            // where it was, greyed, and nothing moved
            assert_eq!(place(&harness), (input, true, stage), "{width}");
        }
    }

    #[test]
    fn keeps_each_files_settings_scrolled_as_it_was_left() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
        square(&a, [255, 0, 0, 255]);
        square(&b, [0, 0, 255, 255]);
        let mut harness = Harness::builder().with_size([1280.0, 820.0]).build_eframe(|cc| {
            let app = App::with(Config::default(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
        harness.state_mut().add(vec![a, b]);
        finish(&mut harness);
        let ids: Vec<u64> = harness.state().items.iter().map(|item| item.id).collect();
        let select = |harness: &mut Harness<App>, id: u64| {
            harness.state_mut().select(id);
            harness.run_steps(3);
        };
        // the settings under the preview, by a row of them
        let row = |harness: &Harness<App>| harness.get_by_label(&fl!("crop")).rect().top();
        select(&mut harness, ids[0]);
        let top = row(&harness);
        let at = harness.get_by_label(&fl!("crop")).rect().center();
        harness.event(egui::Event::PointerMoved(at));
        let (unit, phase) = (egui::MouseWheelUnit::Point, egui::TouchPhase::Move);
        let (delta, modifiers) = (vec2(0.0, -60.0), egui::Modifiers::NONE);
        harness.event(egui::Event::MouseWheel { unit, delta, phase, modifiers });
        harness.run_steps(6);
        let scrolled = row(&harness);
        assert!(scrolled < top - 20.0, "{scrolled} from {top}");
        // the other file's from their top, and the first's as it was left
        select(&mut harness, ids[1]);
        assert_eq!(row(&harness), top);
        select(&mut harness, ids[0]);
        assert_eq!(row(&harness), scrolled);
    }

    #[test]
    fn reflows_the_settings_scrolled_to_their_end_as_the_list_is_dragged_narrower() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("photo.png");
        image::RgbaImage::from_fn(500, 500, |x, y| {
            image::Rgba([(x / 2) as u8, (y / 2) as u8, ((x + y) / 4) as u8, 255])
        })
        .save(&photo)
        .unwrap();
        let mut harness = Harness::builder().with_size([1500.0, 900.0]).build_eframe(|cc| {
            let app = App::with(Config::default(), None, Presets::builtin());
            app.style(&cc.egui_ctx);
            app
        });
        harness.state_mut().add(vec![photo]);
        finish(&mut harness);
        // the settings, in one column, scrolled to their end
        let divider = harness.get_by_label(&fl!("preview-divider")).rect();
        let in_settings = egui::pos2(divider.center().x, divider.bottom() + 60.0);
        harness.event(egui::Event::PointerMoved(in_settings));
        let (unit, phase) = (egui::MouseWheelUnit::Point, egui::TouchPhase::Move);
        let (delta, modifiers) = (vec2(0.0, -20_000.0), egui::Modifiers::NONE);
        harness.event(egui::Event::MouseWheel { unit, delta, phase, modifiers });
        harness.run_steps(10);
        let end = |harness: &Harness<App>| harness.get_by_label(&fl!("commands")).rect().bottom();
        let at_end = end(&harness);
        let crop = |harness: &Harness<App>| harness.get_by_label(&fl!("crop")).rect().left();
        let column = crop(&harness);
        // the list's edge dragged narrower, the button still held: the
        // settings go in two columns, shorter, and are as they will be once
        // it is let go of, with no empty room kept under them
        let from = egui::pos2(divider.left() - 8.0, 300.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: from - vec2(150.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        harness.event(egui::Event::PointerMoved(from));
        harness.run_steps(2);
        harness.event(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        harness.step();
        for step in 1..=15 {
            harness.event(egui::Event::PointerMoved(from - vec2(10.0 * step as f32, 0.0)));
            harness.step();
        }
        harness.run_steps(2);
        assert!(crop(&harness) < column - 100.0, "one column still");
        let held = end(&harness);
        harness.event(press(false));
        harness.run_steps(3);
        let released = end(&harness);
        assert!((held - released).abs() < 2.0, "held {held}, let go of {released}");
        // no further down than in one column, as they now fit
        assert!(released <= at_end + 2.0, "{released} under {at_end}");
    }

    #[test]
    fn puts_the_line_under_the_preview_back_on_a_double_click() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        square(&a, [255, 0, 0, 255]);
        let mut harness = Harness::builder()
            .with_size([1280.0, 820.0])
            .with_step_dt(1.0 / 60.0)
            .build_eframe(|_| App::with(Config::default(), None, Presets::builtin()));
        harness.state_mut().add(vec![a]);
        finish(&mut harness);
        harness.state_mut().preview_share = 0.4;
        harness.run_steps(2);
        let line = || fl!("preview-divider");
        let at = harness.get_by_label(&line()).rect().center();
        harness.event(egui::Event::PointerMoved(at));
        harness.step();
        for _ in 0..2 {
            for pressed in [true, false] {
                let (button, modifiers) = (egui::PointerButton::Primary, egui::Modifiers::NONE);
                harness.event(egui::Event::PointerButton { pos: at, button, pressed, modifiers });
                harness.step();
            }
        }
        harness.run_steps(2);
        assert_eq!(harness.state().preview_share, crate::app::PREVIEW_SHARE);
        // where it was, then, on the line, which stays where it was
        let moved = harness.get_by_label(&line()).rect();
        assert!(moved.center().y > at.y, "{moved:?}");
    }
}
