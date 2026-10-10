//! Checks any Lottie JSON against Telegram's sticker rules, the limits
//! its server has beyond them, Telegram Desktop's limit on JSON and the
//! parse limits of tlottie, Telegram's renderer. Shapes, paints and their sources are counted the way tlottie's
//! parser counts them (`src/composition/parse.rs`).

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::limits::{MAX_RAW_JSON, TLOTTIE, telegram};

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Stats {
    pub width: f64,
    pub height: f64,
    pub fps: f64,
    /// `op - ip`.
    pub frames: f64,
    pub json_bytes: usize,
    /// Size of the `.tgs`, when checking one.
    pub tgs_bytes: Option<usize>,
    /// All layers, including those of precomps.
    pub layers: usize,
    pub painted_shape_layers: usize,
    /// In all layers, counted like `max_shapes_per_layer`.
    pub shapes: usize,
    pub max_shapes_per_layer: usize,
    pub max_paints_per_layer: usize,
    pub max_paint_sources_per_layer: usize,
    pub max_path_points: usize,
    /// Path vertices one fill or stroke paints: those of the paths before
    /// it in its group, nested groups included.
    pub max_paint_points: usize,
    pub max_path_coordinate: f64,
    /// Paths with an empty tangent (`[]`), as pixelart2tgs 1.x wrote them.
    pub empty_tangent_paths: usize,
    pub max_keyframes: usize,
    pub assets: usize,
    /// Layers once precomps are expanded where they are used.
    pub expanded_layers: usize,
    /// Arrays and objects inside each other in the JSON.
    pub depth: usize,
    /// Shape groups inside each other.
    pub group_depth: usize,
    /// Lottie features used, by name.
    pub features: BTreeSet<&'static str>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Telegram refuses the sticker, or some client can't play it.
    Error,
    /// Against the rules, or ignored by Telegram's apps, though Telegram
    /// accepts it.
    Warning,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub message: String,
}

/// Lottie features Telegram's rules forbid or its apps ignore, by the names
/// [`Stats::features`] uses: whether the rules forbid each, and whether
/// tlottie (the renderer of Android, Desktop and web) draws it. Telegram's
/// server accepted a sticker with each of them (T9, `docs/probes.md`).
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

/// How deeply arrays and objects nest, read without parsing.
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

/// Checks Lottie JSON; `tgs_bytes` is the size of the `.tgs` it came from.
/// Fails on JSON that doesn't parse, or nests deeper than Telegram's
/// renderer reads.
pub fn check(json: &[u8], tgs_bytes: Option<usize>) -> Result<(Stats, Vec<Issue>), String> {
    let depth = nesting(json);
    if depth > TLOTTIE.max_nesting_depth {
        return Err(format!(
            "the JSON nests {depth} levels deep; Telegram's renderer reads up to {}",
            TLOTTIE.max_nesting_depth
        ));
    }
    // serde_json stops at 128 levels on its own; the depth is bounded above
    let mut parser = serde_json::Deserializer::from_slice(json);
    parser.disable_recursion_limit();
    let root = Value::deserialize(&mut parser).map_err(|err| err.to_string())?;
    parser.end().map_err(|err| err.to_string())?;
    let number = |key: &str| root.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let mut walker = Walker {
        stats: Stats {
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
            None => issues.push(error("precomps reference each other in a loop".into())),
        }
    }
    let stats = walker.stats;
    issues.extend(rules(&stats));
    Ok((stats, issues))
}

fn error(message: String) -> Issue {
    Issue { severity: Severity::Error, message }
}

fn rules(stats: &Stats) -> Vec<Issue> {
    let mut issues = Vec::new();
    let mut fail = |message: String| issues.push(error(message));
    let canvas = f64::from(telegram::CANVAS);
    if (stats.width, stats.height) != (canvas, canvas) {
        fail(format!("the canvas is {}x{}, stickers are 512x512", stats.width, stats.height));
    }
    // T9: 30 fps was accepted too, and the limit is in seconds
    if stats.fps != f64::from(telegram::FPS) && stats.fps != 30.0 {
        fail(format!("{} fps, stickers are 60 fps", stats.fps));
    }
    let max_seconds = f64::from(telegram::MAX_FRAMES) / f64::from(telegram::FPS);
    if stats.fps > 0.0 && stats.frames / stats.fps > max_seconds + 1e-9 {
        fail(format!(
            "{} frames at {} fps last {:.2} s, at most {max_seconds} s are allowed",
            stats.frames,
            stats.fps,
            stats.frames / stats.fps
        ));
    }
    if stats.frames <= 0.0 {
        fail("the animation has no frames".into());
    }
    if let Some(bytes) = stats.tgs_bytes.filter(|&bytes| bytes > telegram::MAX_BYTES) {
        fail(format!("the file is {bytes} bytes, at most {} are allowed", telegram::MAX_BYTES));
    }
    let cost = telegram::cost(stats.shapes, stats.layers);
    if cost > telegram::MAX_COST {
        fail(format!(
            "{} shapes in {} layers count {cost}, a layer as {}; Telegram takes at most {}",
            stats.shapes,
            stats.layers,
            telegram::LAYER_COST,
            telegram::MAX_COST
        ));
    }
    // Telegram's own limits are below tlottie's for these
    let server = [
        ("layers", stats.layers, telegram::MAX_LAYERS),
        ("shapes in a layer", stats.max_shapes_per_layer, telegram::MAX_SHAPES_PER_LAYER),
        ("path points under one fill", stats.max_paint_points, telegram::MAX_PAINT_POINTS),
    ];
    for (what, count, limit) in server {
        if count > limit {
            fail(format!("{count} {what}; Telegram takes at most {limit}"));
        }
    }
    if stats.empty_tangent_paths > 0 {
        fail(format!(
            "{} paths have empty tangents (`[]`, as pixelart2tgs 1.x wrote them); Telegram \
             refuses those now, `[0,0]` works",
            stats.empty_tangent_paths
        ));
    }
    if stats.json_bytes > MAX_RAW_JSON {
        fail(format!(
            "{} bytes of JSON; Telegram Desktop plays at most {MAX_RAW_JSON}",
            stats.json_bytes
        ));
    }
    let limits = [
        ("fills and strokes in a layer", stats.max_paints_per_layer, TLOTTIE.max_paints_per_layer),
        (
            "shapes painted in a layer",
            stats.max_paint_sources_per_layer,
            TLOTTIE.max_paint_source_items_per_layer,
        ),
        ("points in a path", stats.max_path_points, TLOTTIE.max_path_points),
        ("keyframes in a property", stats.max_keyframes, TLOTTIE.max_keyframes),
        ("assets", stats.assets, TLOTTIE.max_assets),
        ("layers with precomps expanded", stats.expanded_layers, TLOTTIE.max_precomp_expansion),
        ("levels of nesting", stats.depth, TLOTTIE.max_nesting_depth),
        ("groups inside each other", stats.group_depth, TLOTTIE.max_group_depth),
    ];
    for (what, count, limit) in limits {
        if count > limit {
            fail(format!("{count} {what}; Telegram's renderer allows {limit}"));
        }
    }
    if stats.max_path_coordinate > f64::from(TLOTTIE.max_path_coordinate_abs) {
        fail(format!(
            "a path point at {}; Telegram's renderer allows up to {}",
            stats.max_path_coordinate, TLOTTIE.max_path_coordinate_abs
        ));
    }
    if stats.fps == 30.0 {
        issues.push(Issue {
            severity: Severity::Warning,
            message: "30 fps: against the rules, but Telegram accepts this".into(),
        });
    }
    for &(feature, forbidden, drawn) in FEATURES {
        if !stats.features.contains(feature) {
            continue;
        }
        let message = match (forbidden, drawn) {
            (true, true) => {
                format!("{feature}: against the rules, but Telegram accepts this")
            }
            (true, false) => format!(
                "{feature}: against the rules; Telegram accepts this, but its apps ignore it"
            ),
            (false, _) => format!("{feature}: Telegram's apps ignore this"),
        };
        issues.push(Issue { severity: Severity::Warning, message });
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

    /// One list of shape items. A paint covers the geometry before it in
    /// its list, and a group counts as one piece of geometry in its parent.
    /// Returns the path points in the list, groups inside it included,
    /// which a paint after the list's group paints too.
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

    /// Reads a path; returns its vertices, of its largest keyframe when
    /// it is animated.
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

    /// Layers counted once for every place a precomp is used, up to just
    /// past tlottie's limit; `None` for precomps that contain themselves.
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
        items.push(Item::GroupTransform);
        Item::Group(items)
    }

    #[test]
    fn counts_like_tlottie() {
        let layer = |groups: Vec<Item>| Layer {
            from: 0,
            to: 60,
            transform: Transform { position: [0.0, 0.0], scale: 1.0 },
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
            ["512x256", "25 fps", "200 frames at 25 fps last 8.00 s", "70000 bytes", "165389"]
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
                "expressions: against the rules; Telegram accepts this, but its apps ignore it",
                "masks: against the rules, but Telegram accepts this",
                "solids: against the rules, but Telegram accepts this",
                "merge paths: against the rules; Telegram accepts this, but its apps ignore it",
            ]
        );
        assert_eq!(stats.max_keyframes, 2);
        assert_eq!((stats.max_paints_per_layer, stats.max_paint_sources_per_layer), (2, 2));
        assert!(stats.features.contains("strokes") && stats.features.contains("keyframes"));
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
                message: "30 fps: against the rules, but Telegram accepts this".into()
            }]
        );
        let (_, issues) = check(sticker(180).as_bytes(), None).unwrap();
        assert!(issues.iter().any(
            |issue| issue.severity == Severity::Error && issue.message.contains("last 6.00 s")
        ));
    }

    #[test]
    fn finds_the_limits_of_telegrams_server() {
        let errors = |animation: &Animation| -> Vec<String> {
            let json = animation.to_json(Style::default());
            let (_, issues) = check(json.as_bytes(), None).unwrap();
            issues.into_iter().map(|issue| issue.message).collect()
        };
        let layer = |rects| Layer {
            from: 0,
            to: 60,
            transform: Transform { position: [0.0, 0.0], scale: 1.0 },
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
            ["24018 shapes in 6 layers count 24072, a layer as 9; Telegram takes at most 24000"]
        );
        assert_eq!(errors(&animation(6, 3980)), Vec::<String>::new());
        let named = Animation { name: Some("x".repeat(MAX_RAW_JSON)), ..animation(1, 1) };
        assert!(errors(&named)[0].contains("Telegram Desktop plays at most"));

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
            items.push(Item::GroupTransform);
            Item::Group(items)
        };
        let one_fill = Animation {
            name: None,
            ticks: 60,
            layers: vec![Layer { items: vec![group(6)], ..layer(0) }],
        };
        assert_eq!(
            errors(&one_fill),
            ["8010 path points under one fill; Telegram takes at most 8000"]
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
        assert!(issues.iter().any(|issue| issue.message.contains("loop")));
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
        assert!(issues.iter().any(|issue| issue.message.contains("precomps expanded")));
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
            assert!(issues.iter().any(|issue| issue.message.contains("path point")), "{issues:?}");
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
        assert!(issues.iter().any(|issue| issue.message.contains("groups inside")));
        // nesting past what tlottie reads at all
        assert!(check(groups(100).as_bytes(), None).unwrap_err().contains("nests"));
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
        assert!(issues.iter().any(|issue| issue.message.contains("assets")));
    }
}
