//! Validates Lottie JSON against Telegram's sticker rules, server limits,
//! Telegram Desktop's raw JSON limit, and the parser limits of `tlottie`
//! (Telegram's renderer on Android, Desktop and web). Shapes, paints, and
//! paint sources are counted the same way as tlottie's parser
//! (`src/composition/parse.rs`).

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::limits::{MAX_RAW_JSON, TLOTTIE, telegram};

/// Summary statistics of a Lottie document relevant to Telegram's limits.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Stats {
    /// Animation name (`nm`).
    pub name: Option<String>,
    /// Canvas width (`w`).
    pub width: f64,
    /// Canvas height (`h`).
    pub height: f64,
    /// Frame rate in frames per second (`fr`).
    pub fps: f64,
    /// Frame count (`op - ip`).
    pub frames: f64,
    /// Uncompressed Lottie JSON size in bytes.
    pub json_bytes: usize,
    /// Compressed `.tgs` size in bytes, when checking a `.tgs` file.
    pub tgs_bytes: Option<usize>,
    /// Total layer count, including layers inside precomps.
    pub layers: usize,
    /// Number of shape layers that paint at least one shape.
    pub painted_shape_layers: usize,
    /// Total shape items across all layers, counted like `max_shapes_per_layer`.
    pub shapes: usize,
    /// Maximum shape items in a single layer, counted as tlottie counts them:
    /// groups, fills, transforms, and geometry items.
    pub max_shapes_per_layer: usize,
    /// Maximum fills and strokes in a single layer.
    pub max_paints_per_layer: usize,
    /// Maximum geometry items (rectangles, paths) painted in a single layer.
    pub max_paint_sources_per_layer: usize,
    /// Maximum vertex count in a single path or tangent array.
    pub max_path_points: usize,
    /// Maximum path vertices painted by a single fill or stroke (all paths
    /// preceding it in its group, including nested groups).
    pub max_paint_points: usize,
    /// Largest absolute coordinate value in any path.
    pub max_path_coordinate: f64,
    /// Number of paths with empty tangents (`[]`), as written by pixelart2tgs 1.x.
    pub empty_tangent_paths: usize,
    /// Maximum keyframe count on any animated property.
    pub max_keyframes: usize,
    /// Total asset count (precomps, images, etc.).
    pub assets: usize,
    /// Total layer count after expanding precomps at each call site.
    pub expanded_layers: usize,
    /// Maximum JSON array/object nesting depth.
    pub depth: usize,
    /// Maximum shape group nesting depth.
    pub group_depth: usize,
    /// Lottie features detected in the document, by name.
    pub features: BTreeSet<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// Severity of a validation [`Issue`].
#[non_exhaustive]
pub enum Severity {
    /// Telegram rejects the sticker or a client cannot play it.
    Error,
    /// Against Telegram's rules or ignored by Telegram's apps, though the
    /// server accepts it.
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
/// A problem found in a sticker by [`check`].
#[non_exhaustive]
pub struct Issue {
    /// Whether this causes rejection or is only a warning.
    pub severity: Severity,
    /// Category of the check that triggered this issue.
    pub kind: IssueKind,
    /// Plain-English description including the measured numbers.
    pub message: String,
    /// What was measured, for wording the issue other than `message`
    /// does, as a translated front-end does. Not serialised.
    #[serde(skip)]
    pub finding: Finding,
}

/// What a check found that makes an [`Issue`], with the numbers it
/// measured and the limit they are held to. [`Issue::kind`] says what
/// was checked, which tells apart the issues of the same shape here.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Finding {
    /// The canvas is `width` × `height` pixels, where Telegram requires
    /// `side` × `side`.
    Canvas {
        /// Canvas width.
        width: f64,
        /// Canvas height.
        height: f64,
        /// The side Telegram requires.
        side: u32,
    },
    /// `fps` frames a second, where Telegram requires `required`: an
    /// error, or a warning at 30 fps, which Telegram accepts against its
    /// rules.
    FrameRate {
        /// The animation's frame rate.
        fps: f64,
        /// The frame rate Telegram requires.
        required: u32,
    },
    /// `frames` frames at `fps` frames a second, longer than `max`
    /// seconds.
    Length {
        /// Frame count.
        frames: f64,
        /// Frame rate.
        fps: f64,
        /// Longest allowed, in seconds.
        max: f64,
    },
    /// The animation has no frames.
    NoFrames,
    /// `count` of what [`Issue::kind`] counts, more than `limit`: bytes of
    /// the file ([`IssueKind::FileSize`]) or of its JSON
    /// ([`IssueKind::JsonSize`]), layers, shapes in a layer, path points
    /// under one fill, fills and strokes in a layer, shapes painted in a
    /// layer, points in a path, keyframes in a property, assets, layers
    /// with precomps expanded, or levels of nesting or of group nesting.
    Count {
        /// What was counted.
        count: usize,
        /// The most allowed.
        limit: usize,
    },
    /// `shapes` shapes in `layers` layers come to a complexity of `cost`,
    /// each layer counting as `layer_cost` shapes, more than `limit`.
    Cost {
        /// Shape count.
        shapes: usize,
        /// Layer count.
        layers: usize,
        /// Their complexity.
        cost: usize,
        /// What each layer counts as.
        layer_cost: usize,
        /// The most Telegram allows.
        limit: usize,
    },
    /// `paths` paths use empty tangents (`[]`), which Telegram rejects.
    EmptyTangents {
        /// How many.
        paths: usize,
    },
    /// A path coordinate of `coordinate`, more than `limit`.
    PathCoordinate {
        /// The largest absolute coordinate.
        coordinate: f64,
        /// The most Telegram's renderer allows.
        limit: f64,
    },
    /// Precomps reference each other in a loop.
    PrecompLoop,
    /// The Lottie feature `name` (as [`Stats::features`] names it): against
    /// Telegram's rules where `against_rules`, though Telegram accepts it,
    /// and drawn by Telegram's apps where `drawn`.
    Feature {
        /// The feature's English name.
        name: &'static str,
        /// It is against Telegram's rules.
        against_rules: bool,
        /// Telegram's apps draw it.
        drawn: bool,
    },
}

/// Category of a validation [`Issue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum IssueKind {
    /// Canvas is not 512×512.
    Canvas,
    /// Frame rate is not 60 fps (or 30 fps, which produces a warning).
    FrameRate,
    /// Animation is longer than 3 seconds.
    Length,
    /// Animation has zero or negative frame count.
    NoFrames,
    /// Compressed `.tgs` file exceeds 64 KiB.
    FileSize,
    /// Combined shape and layer complexity exceeds Telegram's server limit.
    Cost,
    /// Layer count exceeds Telegram's server limit.
    Layers,
    /// Shapes in a single layer exceed Telegram's server limit.
    ShapesPerLayer,
    /// Path points under one fill exceed Telegram's server limit.
    PaintPoints,
    /// Paths use empty tangents (`[]`), which Telegram rejects.
    EmptyTangents,
    /// Uncompressed JSON exceeds Telegram Desktop's limit.
    JsonSize,
    /// Fills and strokes in a layer exceed the renderer limit.
    PaintsPerLayer,
    /// Painted geometry items in a layer exceed the renderer limit.
    PaintSources,
    /// Vertices in a single path exceed the renderer limit.
    PathPoints,
    /// Keyframes in a property exceed the renderer limit.
    Keyframes,
    /// Asset count exceeds the renderer limit.
    Assets,
    /// Expanded layer count exceeds the renderer limit.
    ExpandedLayers,
    /// JSON nesting depth exceeds the renderer limit.
    Nesting,
    /// Shape group nesting depth exceeds the renderer limit.
    GroupDepth,
    /// A path coordinate exceeds the renderer limit.
    PathCoordinate,
    /// Precomps reference each other in a cycle.
    PrecompLoop,
    /// A Lottie feature against Telegram's rules or ignored by Telegram's apps,
    /// named in the message.
    Feature,
}

/// Error returned when Lottie JSON cannot be parsed for validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// JSON nesting exceeds tlottie's limit and could overflow our parser's
    /// stack.
    #[error("JSON nesting depth is {depth}; Telegram's renderer allows up to {limit}")]
    TooDeep {
        /// Measured nesting depth.
        depth: usize,
        /// Renderer nesting limit.
        limit: usize,
    },
    /// Invalid JSON reported by `serde_json`.
    #[error("{0}")]
    Json(String),
}

/// Lottie features against Telegram's rules or ignored by its apps:
/// `(feature_name, against_rules, drawn_by_tlottie)`, where `tlottie` is
/// Telegram's renderer on Android, Desktop and web.
/// Telegram's server accepted probe stickers using each of these.
const FEATURES: &[(&str, bool, bool)] = &[
    ("expressions", true, false),
    ("masks", true, true),
    ("mattes", true, true),
    ("effects", true, false),
    ("images", true, false),
    ("solids", true, true),
    ("texts", true, false),
    ("3D layers", true, false),
    ("merge paths", true, false),
    ("stars", true, true),
    ("gradient strokes", true, true),
    ("repeaters", true, true),
    ("time stretching", true, false),
    ("time remapping", true, true),
    ("auto-orient", true, true),
    ("blend modes", false, false),
    ("offset paths", false, false),
    ("zig-zags", false, false),
    ("pucker and bloat", false, false),
    ("twists", false, false),
];

#[derive(Default)]
struct ShapeCounts {
    items: usize,
    paints: usize,
    paint_sources: usize,
}

struct Walker<'a> {
    stats: Stats,
    assets: HashMap<&'a str, &'a Value>,
}

/// Measures JSON nesting depth without parsing.
fn nesting(json: &[u8]) -> usize {
    let (mut depth, mut deepest, mut in_string, mut escaped) = (0usize, 0, false, false);
    for &byte in json {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

/// Validates Lottie JSON against Telegram and renderer limits. `tgs_bytes` is
/// the compressed `.tgs` file size, if known. Fails on invalid JSON or nesting
/// deeper than Telegram's renderer accepts.
pub fn check(json: &[u8], tgs_bytes: Option<usize>) -> Result<(Stats, Vec<Issue>), CheckError> {
    let depth = nesting(json);
    if depth > TLOTTIE.max_nesting_depth {
        return Err(CheckError::TooDeep { depth, limit: TLOTTIE.max_nesting_depth });
    }
    // serde_json defaults to a 128-level recursion limit; depth is already
    // bounded above.
    let mut parser = serde_json::Deserializer::from_slice(json);
    parser.disable_recursion_limit();
    let json_error = |err: serde_json::Error| CheckError::Json(err.to_string());
    let root = Value::deserialize(&mut parser).map_err(json_error)?;
    parser.end().map_err(json_error)?;
    let number = |key: &str| root.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let mut walker = Walker {
        stats: Stats {
            name: root.get("nm").and_then(Value::as_str).map(str::to_owned),
            width: number("w"),
            height: number("h"),
            fps: number("fr"),
            frames: number("op") - number("ip"),
            json_bytes: json.len(),
            tgs_bytes,
            ..Stats::default()
        },
        assets: HashMap::new(),
    };
    walker.walk_values(&root, 1);
    let mut issues = Vec::new();
    for asset in root.get("assets").and_then(Value::as_array).into_iter().flatten() {
        walker.stats.assets += 1;
        if let Some(id) = asset.get("id").and_then(Value::as_str) {
            walker.assets.insert(id, asset);
        }
        match asset.get("layers").and_then(Value::as_array) {
            Some(layers) => {
                walker.stats.features.insert("precomps");
                layers.iter().for_each(|layer| walker.layer(layer));
            }
            None => _ = walker.stats.features.insert("images"),
        }
    }
    let layers = root.get("layers").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    for layer in layers {
        walker.layer(layer);
    }
    // expanding recurses once per nested precomp, at most once per asset;
    // past tlottie's limit on assets the file fails anyway
    if walker.stats.assets <= TLOTTIE.max_assets {
        match walker.expand(layers, &mut HashMap::new(), &mut Vec::new()) {
            Some(count) => walker.stats.expanded_layers = count,
            None => issues.push(error(
                IssueKind::PrecompLoop,
                "precomps reference each other in a loop".into(),
                Finding::PrecompLoop,
            )),
        }
    }
    let stats = walker.stats;
    issues.extend(rules(&stats));
    Ok((stats, issues))
}

fn error(kind: IssueKind, message: String, finding: Finding) -> Issue {
    Issue { severity: Severity::Error, kind, message, finding }
}

fn rules(stats: &Stats) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut fail = |kind: IssueKind, message: String, finding: Finding| {
        issues.push(error(kind, message, finding));
    };
    let canvas = f64::from(telegram::CANVAS);
    if (stats.width, stats.height) != (canvas, canvas) {
        let (width, height, side) = (stats.width, stats.height, telegram::CANVAS);
        fail(
            IssueKind::Canvas,
            format!("canvas is {width}×{height}; Telegram requires {side}×{side}"),
            Finding::Canvas { width, height, side },
        );
    }
    // 30 fps was accepted too, and the limit is in seconds
    let required = telegram::FPS;
    if stats.fps != f64::from(required) && stats.fps != 30.0 {
        fail(
            IssueKind::FrameRate,
            format!("{} fps; Telegram requires {required} fps", stats.fps),
            Finding::FrameRate { fps: stats.fps, required },
        );
    }
    let max_seconds = f64::from(telegram::MAX_FRAMES) / f64::from(telegram::FPS);
    if stats.fps > 0.0 && stats.frames / stats.fps > max_seconds + 1e-9 {
        fail(
            IssueKind::Length,
            format!(
                "{} frames at {} fps ({:.2} s); maximum is {max_seconds} s",
                stats.frames,
                stats.fps,
                stats.frames / stats.fps
            ),
            Finding::Length { frames: stats.frames, fps: stats.fps, max: max_seconds },
        );
    }
    if stats.frames <= 0.0 {
        fail(IssueKind::NoFrames, "animation has no frames".into(), Finding::NoFrames);
    }
    if let Some(bytes) = stats.tgs_bytes.filter(|&bytes| bytes > telegram::MAX_BYTES) {
        fail(
            IssueKind::FileSize,
            format!("file is {bytes} bytes; maximum is {} bytes", telegram::MAX_BYTES),
            Finding::Count { count: bytes, limit: telegram::MAX_BYTES },
        );
    }
    let cost = telegram::cost(stats.shapes, stats.layers);
    if cost > telegram::MAX_COST {
        fail(
            IssueKind::Cost,
            format!(
                "{} shapes in {} layers have complexity {cost} (each layer counts as {}); \
                 Telegram allows at most {}",
                stats.shapes,
                stats.layers,
                telegram::LAYER_COST,
                telegram::MAX_COST
            ),
            Finding::Cost {
                shapes: stats.shapes,
                layers: stats.layers,
                cost,
                layer_cost: telegram::LAYER_COST,
                limit: telegram::MAX_COST,
            },
        );
    }
    // Telegram's own limits are below tlottie's for these
    let server = [
        (IssueKind::Layers, "layers", stats.layers, telegram::MAX_LAYERS),
        (
            IssueKind::ShapesPerLayer,
            "shapes in a layer",
            stats.max_shapes_per_layer,
            telegram::MAX_SHAPES_PER_LAYER,
        ),
        (
            IssueKind::PaintPoints,
            "path points under one fill",
            stats.max_paint_points,
            telegram::MAX_PAINT_POINTS,
        ),
    ];
    for (kind, what, count, limit) in server {
        if count > limit {
            let message = format!("{count} {what}; Telegram allows at most {limit}");
            fail(kind, message, Finding::Count { count, limit });
        }
    }
    if stats.empty_tangent_paths > 0 {
        fail(
            IssueKind::EmptyTangents,
            format!(
                "{} paths use empty tangents (`[]`, as pixelart2tgs 1.x wrote them), which \
                 Telegram now rejects; `[0,0]` works",
                stats.empty_tangent_paths
            ),
            Finding::EmptyTangents { paths: stats.empty_tangent_paths },
        );
    }
    if stats.json_bytes > MAX_RAW_JSON {
        fail(
            IssueKind::JsonSize,
            format!(
                "{} bytes of JSON; Telegram Desktop allows at most {MAX_RAW_JSON}",
                stats.json_bytes
            ),
            Finding::Count { count: stats.json_bytes, limit: MAX_RAW_JSON },
        );
    }
    let limits = [
        (
            IssueKind::PaintsPerLayer,
            "fills and strokes in a layer",
            stats.max_paints_per_layer,
            TLOTTIE.max_paints_per_layer,
        ),
        (
            IssueKind::PaintSources,
            "shapes painted in a layer",
            stats.max_paint_sources_per_layer,
            TLOTTIE.max_paint_source_items_per_layer,
        ),
        (IssueKind::PathPoints, "points in a path", stats.max_path_points, TLOTTIE.max_path_points),
        (
            IssueKind::Keyframes,
            "keyframes in a property",
            stats.max_keyframes,
            TLOTTIE.max_keyframes,
        ),
        (IssueKind::Assets, "assets", stats.assets, TLOTTIE.max_assets),
        (
            IssueKind::ExpandedLayers,
            "layers with precomps expanded",
            stats.expanded_layers,
            TLOTTIE.max_precomp_expansion,
        ),
        (IssueKind::Nesting, "levels of nesting", stats.depth, TLOTTIE.max_nesting_depth),
        (
            IssueKind::GroupDepth,
            "levels of group nesting",
            stats.group_depth,
            TLOTTIE.max_group_depth,
        ),
    ];
    for (kind, what, count, limit) in limits {
        if count > limit {
            let message = format!("{count} {what}; Telegram's renderer allows at most {limit}");
            fail(kind, message, Finding::Count { count, limit });
        }
    }
    let coordinate = stats.max_path_coordinate;
    if coordinate > f64::from(TLOTTIE.max_path_coordinate_abs) {
        let limit = TLOTTIE.max_path_coordinate_abs;
        fail(
            IssueKind::PathCoordinate,
            format!(
                "a path coordinate of {coordinate}; Telegram's renderer allows at most {limit}"
            ),
            Finding::PathCoordinate { coordinate, limit: f64::from(limit) },
        );
    }
    if stats.fps == 30.0 {
        issues.push(Issue {
            severity: Severity::Warning,
            kind: IssueKind::FrameRate,
            message: "30 fps: against Telegram's rules, but Telegram accepts it".into(),
            finding: Finding::FrameRate { fps: stats.fps, required },
        });
    }
    for &(feature, forbidden, drawn) in FEATURES {
        if !stats.features.contains(feature) {
            continue;
        }
        let message = match (forbidden, drawn) {
            (true, true) => {
                format!("{feature}: against Telegram's rules, but Telegram accepts it")
            }
            (true, false) => format!(
                "{feature}: against Telegram's rules; Telegram accepts it, but its apps ignore it"
            ),
            (false, _) => format!("{feature}: Telegram's apps ignore this"),
        };
        let finding = Finding::Feature { name: feature, against_rules: forbidden, drawn };
        issues.push(Issue {
            severity: Severity::Warning,
            kind: IssueKind::Feature,
            message,
            finding,
        });
    }
    issues
}

impl<'a> Walker<'a> {
    /// Nesting depth, keyframes and expressions, anywhere in the JSON.
    fn walk_values(&mut self, value: &Value, depth: usize) {
        self.stats.depth = self.stats.depth.max(depth);
        match value {
            Value::Array(items) => items.iter().for_each(|item| self.walk_values(item, depth + 1)),
            Value::Object(object) => {
                if object.get("x").is_some_and(Value::is_string) {
                    self.stats.features.insert("expressions");
                }
                if object.get("ddd").and_then(Value::as_f64) == Some(1.0) {
                    self.stats.features.insert("3D layers");
                }
                if let Some(Value::Array(keys)) = object.get("k")
                    && keys.first().is_some_and(|key| key.get("t").is_some())
                {
                    self.stats.features.insert("keyframes");
                    self.stats.max_keyframes = self.stats.max_keyframes.max(keys.len());
                }
                object.values().for_each(|item| self.walk_values(item, depth + 1));
            }
            _ => {}
        }
    }

    fn layer(&mut self, layer: &Value) {
        self.stats.layers += 1;
        let features = &mut self.stats.features;
        let ty = layer.get("ty").and_then(Value::as_f64).unwrap_or(-1.0) as i64;
        match ty {
            0 => _ = features.insert("precomps"),
            1 => _ = features.insert("solids"),
            2 => _ = features.insert("images"),
            3 => _ = features.insert("null layers"),
            5 => _ = features.insert("texts"),
            _ => {}
        }
        let present = |key: &str| layer.get(key).is_some_and(|value| !value.is_null());
        if layer.get("masksProperties").and_then(Value::as_array).is_some_and(|m| !m.is_empty())
            || layer.get("hasMask").and_then(Value::as_bool) == Some(true)
        {
            features.insert("masks");
        }
        if present("tt") || present("td") || present("tp") {
            features.insert("mattes");
        }
        if layer.get("ef").and_then(Value::as_array).is_some_and(|e| !e.is_empty()) {
            features.insert("effects");
        }
        if layer.get("sr").and_then(Value::as_f64).is_some_and(|sr| sr != 1.0) {
            features.insert("time stretching");
        }
        if present("tm") {
            features.insert("time remapping");
        }
        if layer.get("ao").and_then(Value::as_f64) == Some(1.0) {
            features.insert("auto-orient");
        }
        if present("parent") {
            features.insert("parenting");
        }
        if layer.get("bm").and_then(Value::as_f64).is_some_and(|bm| bm != 0.0) {
            features.insert("blend modes");
        }
        if ty == 4 {
            let mut counts = ShapeCounts::default();
            if let Some(shapes) = layer.get("shapes").and_then(Value::as_array) {
                self.shapes(shapes, &mut counts, 0);
            }
            let stats = &mut self.stats;
            stats.shapes += counts.items;
            stats.max_shapes_per_layer = stats.max_shapes_per_layer.max(counts.items);
            stats.max_paints_per_layer = stats.max_paints_per_layer.max(counts.paints);
            stats.max_paint_sources_per_layer =
                stats.max_paint_sources_per_layer.max(counts.paint_sources);
            if counts.paints > 0 {
                stats.painted_shape_layers += 1;
            }
        }
    }

    /// Walks one shape list. Each paint covers all geometry items before it in
    /// the list, and a group counts as one geometry item in its parent.
    /// Returns the total path vertices in the list (including nested groups)
    /// so a paint after the enclosing group counts them too.
    fn shapes(&mut self, items: &[Value], counts: &mut ShapeCounts, depth: usize) -> usize {
        self.stats.group_depth = self.stats.group_depth.max(depth);
        let (mut sources, mut points) = (0, 0);
        for item in items {
            counts.items += 1;
            let ty = item.get("ty").and_then(Value::as_str).unwrap_or("");
            let feature = match ty {
                "gr" => {
                    let children = item.get("it").and_then(Value::as_array);
                    points +=
                        self.shapes(children.map_or(&[][..], Vec::as_slice), counts, depth + 1);
                    sources += 1;
                    None
                }
                "rc" | "el" | "sh" | "sr" => {
                    sources += 1;
                    if ty == "sh" {
                        points += self.path(item);
                    }
                    match ty {
                        "el" => Some("ellipses"),
                        "sr" => Some("stars"),
                        _ => None,
                    }
                }
                "fl" | "st" | "gf" | "gs" => {
                    counts.paints += 1;
                    counts.paint_sources += sources;
                    self.stats.max_paint_points = self.stats.max_paint_points.max(points);
                    if item.get("r").and_then(Value::as_f64) == Some(2.0) {
                        self.stats.features.insert("even-odd fills");
                    }
                    match ty {
                        "st" => Some("strokes"),
                        "gf" => Some("gradient fills"),
                        "gs" => Some("gradient strokes"),
                        _ => None,
                    }
                }
                "mm" => Some("merge paths"),
                "rp" => Some("repeaters"),
                "tm" => Some("trim paths"),
                "rd" => Some("rounded corners"),
                "op" => Some("offset paths"),
                "zz" => Some("zig-zags"),
                "pb" => Some("pucker and bloat"),
                "tw" => Some("twists"),
                _ => None,
            };
            if let Some(feature) = feature {
                self.stats.features.insert(feature);
            }
        }
        points
    }

    /// Counts path vertices (maximum across keyframes if animated).
    fn path(&mut self, item: &Value) -> usize {
        let shape = item.get("ks").and_then(|ks| ks.get("k"));
        // a static shape, or the start and end shapes of keyframes
        let shapes: Vec<&Value> = match shape {
            Some(Value::Array(keys)) => keys
                .iter()
                .flat_map(|key| ["s", "e"].map(|end| key.get(end).and_then(Value::as_array)))
                .flatten()
                .flatten()
                .collect(),
            Some(shape) => vec![shape],
            None => Vec::new(),
        };
        let stats = &mut self.stats;
        let empty_tangent = shapes.iter().any(|shape| {
            ["i", "o"].iter().any(|name| {
                let tangents = shape.get(*name).and_then(Value::as_array);
                tangents.into_iter().flatten().any(|t| t.as_array().is_some_and(Vec::is_empty))
            })
        });
        stats.empty_tangent_paths += usize::from(empty_tangent);
        let vertices =
            |shape: &&Value| shape.get("v").and_then(Value::as_array).map_or(0, Vec::len);
        let points = shapes.iter().map(vertices).max().unwrap_or(0);
        for shape in shapes {
            // vertices and both tangents
            for name in ["v", "i", "o"] {
                let Some(points) = shape.get(name).and_then(Value::as_array) else { continue };
                stats.max_path_points = stats.max_path_points.max(points.len());
                for coordinate in points.iter().filter_map(Value::as_array).flatten() {
                    let value = coordinate.as_f64().unwrap_or(0.0).abs();
                    stats.max_path_coordinate = stats.max_path_coordinate.max(value);
                }
            }
        }
        points
    }

    /// Counts layers after expanding precomps at each call site, capped just
    /// past tlottie's limit. Returns `None` on cyclic precomps.
    fn expand(
        &self,
        layers: &'a [Value],
        sizes: &mut HashMap<&'a str, usize>,
        stack: &mut Vec<&'a str>,
    ) -> Option<usize> {
        let cap = TLOTTIE.max_precomp_expansion + 1;
        let mut total = 0usize;
        for layer in layers {
            total = total.saturating_add(1);
            let reference = layer.get("refId").and_then(Value::as_str);
            let Some((&id, asset)) = reference.and_then(|id| self.assets.get_key_value(id)) else {
                continue;
            };
            let Some(inner) = asset.get("layers").and_then(Value::as_array) else { continue };
            if stack.contains(&id) {
                return None;
            }
            let size = match sizes.get(id) {
                Some(&size) => size,
                None => {
                    stack.push(id);
                    let size = self.expand(inner, sizes, stack)?;
                    stack.pop();
                    sizes.insert(id, size);
                    size
                }
            };
            total = total.saturating_add(size);
            if total >= cap {
                return Some(cap);
            }
        }
        Some(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lottie::{Animation, Item, Layer, Style, Transform};
    use crate::scene::FillRule;

    fn group(rects: usize) -> Item {
        let mut items: Vec<Item> = (0..rects)
            .map(|i| Item::Rect { centre: [i as f64 + 0.5, 0.5], size: [1.0, 1.0] })
            .collect();
        items.push(Item::Fill { colour: [1, 2, 3, 255], rule: FillRule::NonZero });
        items.push(Item::GroupTransform { opacity: Vec::new() });
        Item::Group(items)
    }

    #[test]
    fn counts_like_tlottie() {
        let layer = |groups: Vec<Item>| Layer {
            id: None,
            from: 0,
            to: 60,
            transform: Transform { position: [0.0, 0.0], scale: [1.0, 1.0] },
            items: groups,
            hidden: Vec::new(),
        };
        let animation = Animation {
            name: None,
            ticks: 60,
            layers: vec![layer(vec![group(3), group(2)]), layer(vec![group(1)]), layer(vec![])],
        };
        let json = animation.to_json(Style::default());
        let (stats, issues) = check(json.as_bytes(), Some(1000)).unwrap();
        assert_eq!(issues, []);
        assert_eq!((stats.layers, stats.painted_shape_layers), (3, 2));
        // 2 groups + 5 rectangles + 2 fills + 2 transforms
        assert_eq!(stats.max_shapes_per_layer, 11);
        assert_eq!(stats.shapes, 11 + 4);
        assert_eq!((stats.max_paints_per_layer, stats.max_paint_sources_per_layer), (2, 5));
        assert_eq!(
            (stats.width, stats.height, stats.fps, stats.frames),
            (512.0, 512.0, 60.0, 60.0)
        );
        assert!(stats.features.is_empty(), "{:?}", stats.features);
    }

    #[test]
    fn finds_broken_rules() {
        let json = br#"{"fr":25,"ip":0,"op":200,"w":512,"h":256,"layers":[
            {"ty":4,"masksProperties":[{}],"shapes":[{"ty":"gr","it":[
                {"ty":"sh","ks":{"k":{"v":[[0,0],[200000,1]]}}},{"ty":"mm"},
                {"ty":"st"},{"ty":"fl"},{"ty":"tr"}]}]},
            {"ty":1,"ks":{"p":{"x":"wiggle(1,2)","k":[0,0]},"o":{"a":1,"k":[{"t":0,"s":[0]},{"t":9,"s":[100]}]}}}
        ]}"#;
        let (stats, issues) = check(json, Some(70_000)).unwrap();
        let errors: Vec<&str> = issues
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .map(|issue| issue.message.as_str())
            .collect();
        for expected in
            ["512×256", "25 fps", "200 frames at 25 fps (8.00 s)", "70000 bytes", "165389"]
        {
            assert!(errors.iter().any(|e| e.contains(expected)), "{expected} in {errors:?}");
        }
        let warnings: Vec<&str> = issues
            .iter()
            .filter(|issue| issue.severity == Severity::Warning)
            .map(|issue| issue.message.as_str())
            .collect();
        assert_eq!(
            warnings,
            [
                "expressions: against Telegram's rules; Telegram accepts it, but its apps ignore it",
                "masks: against Telegram's rules, but Telegram accepts it",
                "solids: against Telegram's rules, but Telegram accepts it",
                "merge paths: against Telegram's rules; Telegram accepts it, but its apps ignore it",
            ]
        );
        assert_eq!(stats.max_keyframes, 2);
        assert_eq!((stats.max_paints_per_layer, stats.max_paint_sources_per_layer), (2, 2));
        assert!(stats.features.contains("strokes") && stats.features.contains("keyframes"));
    }

    #[test]
    fn says_what_it_measured() {
        let json = br#"{"fr":25,"ip":0,"op":200,"w":512,"h":256,"layers":[
            {"ty":4,"masksProperties":[{}],"shapes":[]}]}"#;
        let (_, issues) = check(json, Some(70_000)).unwrap();
        let findings: Vec<&Finding> = issues.iter().map(|issue| &issue.finding).collect();
        assert_eq!(
            findings,
            [
                &Finding::Canvas { width: 512.0, height: 256.0, side: 512 },
                &Finding::FrameRate { fps: 25.0, required: 60 },
                &Finding::Length { frames: 200.0, fps: 25.0, max: 3.0 },
                &Finding::Count { count: 70_000, limit: 65_536 },
                &Finding::Feature { name: "masks", against_rules: true, drawn: true },
            ]
        );
    }

    #[test]
    fn takes_30_fps_for_3_seconds() {
        let sticker = |op: u32| {
            format!(
                r#"{{"fr":30,"ip":0,"op":{op},"w":512,"h":512,"layers":[{{"ty":4,"shapes":[]}}]}}"#
            )
        };
        let (_, issues) = check(sticker(90).as_bytes(), None).unwrap();
        assert_eq!(
            issues,
            [Issue {
                severity: Severity::Warning,
                kind: IssueKind::FrameRate,
                message: "30 fps: against Telegram's rules, but Telegram accepts it".into(),
                finding: Finding::FrameRate { fps: 30.0, required: 60 },
            }]
        );
        let (_, issues) = check(sticker(180).as_bytes(), None).unwrap();
        assert!(
            issues
                .iter()
                .any(|issue| issue.severity == Severity::Error && issue.kind == IssueKind::Length)
        );
    }

    #[test]
    fn finds_the_limits_of_telegrams_server() {
        let errors = |animation: &Animation| -> Vec<String> {
            let json = animation.to_json(Style::default());
            let (_, issues) = check(json.as_bytes(), None).unwrap();
            issues.into_iter().map(|issue| issue.message).collect()
        };
        let layer = |rects| Layer {
            id: None,
            from: 0,
            to: 60,
            transform: Transform { position: [0.0, 0.0], scale: [1.0, 1.0] },
            items: vec![group(rects)],
            hidden: Vec::new(),
        };
        let animation = |layers, rects| Animation {
            name: None,
            ticks: 60,
            layers: (0..layers).map(|_| layer(rects)).collect(),
        };
        // 6 layers of 4003 shapes: 24 018, with the layers 24 072
        assert_eq!(
            errors(&animation(6, 4000)),
            [
                "24018 shapes in 6 layers have complexity 24072 (each layer counts as 9); Telegram allows at most 24000"
            ]
        );
        let json = animation(6, 4000).to_json(Style::default());
        let (_, issues) = check(json.as_bytes(), None).unwrap();
        let (shapes, layers, cost, layer_cost, limit) = (24_018, 6, 24_072, 9, 24_000);
        assert_eq!(issues[0].finding, Finding::Cost { shapes, layers, cost, layer_cost, limit });
        assert_eq!(errors(&animation(6, 3980)), Vec::<String>::new());
        // a layer of as many shapes as were accepted, and of one more
        assert_eq!(errors(&animation(1, telegram::MAX_SHAPES_PER_LAYER - 3)), Vec::<String>::new());
        assert_eq!(
            errors(&animation(1, telegram::MAX_SHAPES_PER_LAYER - 2)),
            ["4094 shapes in a layer; Telegram allows at most 4093"]
        );
        let named = Animation { name: Some("x".repeat(MAX_RAW_JSON)), ..animation(1, 1) };
        assert!(errors(&named)[0].contains("Telegram Desktop allows at most"));

        let path = |tangent: &str| {
            format!(
                r#"{{"fr":60,"ip":0,"op":10,"w":512,"h":512,"layers":[{{"ty":4,"shapes":[{{"ty":"sh","ks":{{"k":{{"v":[[0,0],[1,0],[1,1]],"i":[{tangent},{tangent},{tangent}],"o":[[0,0],[0,0],[0,0]]}}}}}}]}}]}}"#
            )
        };
        let (stats, issues) = check(path("[]").as_bytes(), None).unwrap();
        assert_eq!(stats.empty_tangent_paths, 1);
        assert!(issues.iter().any(|issue| issue.message.contains("empty tangents")), "{issues:?}");
        let (stats, issues) = check(path("[0,0]").as_bytes(), None).unwrap();
        assert_eq!((stats.empty_tangent_paths, issues), (0, Vec::new()));

        // 6 paths of 1334 points, 1335 as written: one fill paints 8010
        // of them, or each of three paints 2670
        let points: Vec<[f64; 2]> =
            (0..1334).map(|i| [f64::from(i % 2), f64::from(i / 2)]).collect();
        let group = |paths: usize| {
            let mut items = vec![Item::Path(points.clone()); paths];
            items.push(Item::Fill { colour: [1, 2, 3, 255], rule: FillRule::NonZero });
            items.push(Item::GroupTransform { opacity: Vec::new() });
            Item::Group(items)
        };
        let one_fill = Animation {
            name: None,
            ticks: 60,
            layers: vec![Layer { items: vec![group(6)], ..layer(0) }],
        };
        assert_eq!(
            errors(&one_fill),
            ["8010 path points under one fill; Telegram allows at most 8000"]
        );
        let three_fills = Animation {
            name: None,
            ticks: 60,
            layers: vec![Layer { items: vec![group(2), group(2), group(2)], ..layer(0) }],
        };
        assert_eq!(errors(&three_fills), Vec::<String>::new());
    }

    #[test]
    fn finds_precomp_loops() {
        let json = br#"{"fr":60,"ip":0,"op":10,"w":512,"h":512,
            "assets":[{"id":"a","layers":[{"ty":0,"refId":"b"}]},{"id":"b","layers":[{"ty":0,"refId":"a"}]}],
            "layers":[{"ty":0,"refId":"a"},{"ty":0,"refId":"b"}]}"#;
        let (stats, issues) = check(json, None).unwrap();
        assert!(issues.iter().any(|issue| issue.kind == IssueKind::PrecompLoop));
        assert_eq!(stats.assets, 2);

        let json = br#"{"fr":60,"ip":0,"op":10,"w":512,"h":512,
            "assets":[{"id":"a","layers":[{"ty":4},{"ty":4}]}],
            "layers":[{"ty":0,"refId":"a"},{"ty":0,"refId":"a"},{"ty":4}]}"#;
        let (stats, issues) = check(json, None).unwrap();
        assert_eq!(issues, []);
        // 3 top layers and 2 for each use of the precomp
        assert_eq!((stats.layers, stats.expanded_layers), (5, 7));
    }

    #[test]
    fn expands_precomps_once() {
        // each asset uses the one before twice: 2^40 layers if expanded
        // naively, but only 41 assets to look at
        let mut assets = vec![r#"{"id":"a0","layers":[{"ty":4}]}"#.to_owned()];
        for n in 1..=40 {
            let previous = n - 1;
            assets.push(format!(
                r#"{{"id":"a{n}","layers":[{{"ty":0,"refId":"a{previous}"}},{{"ty":0,"refId":"a{previous}"}}]}}"#
            ));
        }
        let json = format!(
            r#"{{"fr":60,"ip":0,"op":10,"w":512,"h":512,"assets":[{}],"layers":[{{"ty":0,"refId":"a40"}}]}}"#,
            assets.join(",")
        );
        let started = std::time::Instant::now();
        let (stats, issues) = check(json.as_bytes(), None).unwrap();
        assert!(started.elapsed().as_secs() < 2);
        assert!(stats.expanded_layers > TLOTTIE.max_precomp_expansion);
        assert!(issues.iter().any(|issue| issue.kind == IssueKind::ExpandedLayers));
    }

    #[test]
    fn reads_tangents_and_end_shapes() {
        let path = |shape: &str| {
            format!(
                r#"{{"fr":60,"ip":0,"op":10,"w":512,"h":512,"layers":[{{"ty":4,"shapes":[{{"ty":"sh","ks":{shape}}}]}}]}}"#
            )
        };
        let far_tangent =
            path(r#"{"k":{"v":[[0,0],[1,1]],"i":[[200000,0],[0,0]],"o":[[0,0],[0,0]]}}"#);
        let far_end =
            path(r#"{"a":1,"k":[{"t":0,"s":[{"v":[[0,0]]}],"e":[{"v":[[0,-200000]]}]},{"t":9}]}"#);
        for json in [far_tangent, far_end] {
            let (stats, issues) = check(json.as_bytes(), None).unwrap();
            assert_eq!(stats.max_path_coordinate, 200_000.0);
            assert!(
                issues.iter().any(|issue| issue.message.contains("path coordinate")),
                "{issues:?}"
            );
        }
    }

    #[test]
    fn reads_deep_nesting_like_tlottie() {
        let groups = |depth: usize| {
            let open = r#"{"ty":"gr","it":["#.repeat(depth);
            let close = "]}".repeat(depth);
            format!(
                r#"{{"fr":60,"ip":0,"op":10,"w":512,"h":512,"layers":[{{"ty":4,"shapes":[{open}{close}]}}]}}"#
            )
        };
        // past serde_json's own limit of 128, within tlottie's: two levels
        // per group, four around them
        let (stats, issues) = check(groups(64).as_bytes(), None).unwrap();
        assert_eq!((stats.group_depth, stats.depth), (64, 132));
        assert_eq!(issues, []);
        // more groups inside groups than tlottie takes
        let (_, issues) = check(groups(70).as_bytes(), None).unwrap();
        assert!(issues.iter().any(|issue| issue.kind == IssueKind::GroupDepth));
        // nesting past what tlottie reads at all
        assert!(matches!(
            check(groups(100).as_bytes(), None),
            Err(CheckError::TooDeep { depth: 204, limit: 175 })
        ));
        // brackets in strings don't count
        assert_eq!(nesting(br#"{"a":"[[[{{{\"]]]","b":[1]}"#), 2);
    }

    #[test]
    fn stops_at_long_precomp_chains() {
        // each asset uses the next; deep enough to overflow a stack
        let assets: Vec<String> = (0..20_000)
            .map(|n| format!(r#"{{"id":"a{n}","layers":[{{"ty":0,"refId":"a{}"}}]}}"#, n + 1))
            .collect();
        let json = format!(
            r#"{{"fr":60,"ip":0,"op":10,"w":512,"h":512,"assets":[{}],"layers":[{{"ty":0,"refId":"a0"}}]}}"#,
            assets.join(",")
        );
        let (stats, issues) = check(json.as_bytes(), None).unwrap();
        assert_eq!(stats.assets, 20_000);
        assert!(issues.iter().any(|issue| issue.kind == IssueKind::Assets));
    }
}
