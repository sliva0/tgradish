//! What a sticker shows, as frames of straight RGBA for previews: drawn
//! from the art it is made of ([`of_art`]), or from the Lottie of a
//! `.tgs` tgradish wrote ([`of_lottie`]), which give the same picture, so
//! that a sticker made earlier shows as it did when it was made.
//!
//! A picture's pixel is a square of the art's lattice: the largest block
//! of input pixels every cell is a whole number of across and down. That
//! is an art pixel, unless the canvas or a crop cut art pixels at the
//! sides, which then show only in part. Where the lattice has more
//! squares along its longer side than a picture may, or than [`MAX_BYTES`]
//! hold in every frame, every so many show, the middle one of each run;
//! frames that then come out the same as the one before are joined.
//!
//! [`of_lottie`] reads the Lottie tgradish writes: shape layers of groups
//! of rectangles, each group one solid fill, layers and groups shown and
//! hidden over whole frames, all placed alike on the 512×512 canvas, the
//! rectangles' edges on a pixel grid. Lottie with more than that, as paths,
//! gradients, masks or fading, is refused rather than drawn wrong, as
//! [`DrawError::Unsupported`]: other programs' stickers mostly.
//!
//! Where a pixel grid's edges are all where nothing is drawn, the file
//! can't tell art cut at the sides from art that isn't: a square sticker
//! cut from wider art through transparent columns reads as art as wide as
//! what shows. The picture is then the same on the canvas, whose margins
//! are wider by the transparent part.

use serde_json::{Map, Value};
use thiserror::Error;

use crate::grid::gcd;
use crate::limits::{MAX_RAW_JSON, TLOTTIE, telegram};
use crate::normalise::PixelAnim;

/// The most bytes the frames of a picture take together: a long
/// animation of large art shows fewer of its pixels to stay within it.
pub const MAX_BYTES: usize = 128 << 20;

/// The most squares a lattice read from Lottie may have along a side.
const MAX_LATTICE: i64 = 1 << 16;
/// The most squares of the lattice an art pixel of a file may be cut
/// into, as by a crop through it.
const MAX_DIVISIONS: i64 = 4096;
/// How much drawing [`of_lottie`] does at most: pixels painted, in all
/// frames. A sticker within Telegram's limits needs far less.
const MAX_WORK: u64 = 1 << 30;
/// How far from where tgradish put them the edges of a rectangle may be,
/// in art pixels: rectangles are written with 6 decimals.
const EDGE: f64 = 2e-6;

/// What a sticker shows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Picture {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Each frame, `width * height * 4` bytes of straight RGBA, and how
    /// many 60 fps frames it shows for. No two in a row are the same.
    pub frames: Vec<(Vec<u8>, u32)>,
}

/// Why [`of_lottie`] couldn't draw a sticker.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DrawError {
    /// It isn't Lottie JSON, or not as Lottie has it: what is wrong.
    #[error("{0}")]
    Invalid(String),
    /// It is Lottie with more than tgradish draws, which this names.
    #[error("it has {0}, which tgradish doesn't draw")]
    Unsupported(String),
}

fn invalid(message: impl Into<String>) -> DrawError {
    DrawError::Invalid(message.into())
}

fn unsupported(what: impl Into<String>) -> DrawError {
    DrawError::Unsupported(what.into())
}

type Result<T> = std::result::Result<T, DrawError>;

/// The squares of a lattice `wide` by `high` a picture shows, across and
/// down.
#[derive(Debug, PartialEq, Eq)]
struct Sampling {
    columns: Vec<u32>,
    rows: Vec<u32>,
}

/// Which squares of a lattice `wide` by `high` show in a picture of
/// `frames` frames: every one, or every `step`th, at most `max_side`
/// along the longer side and [`MAX_BYTES`] in all. `stretch`ed, the
/// picture is square, the shorter side's squares shown more than once.
fn sampling(wide: u32, high: u32, stretch: bool, frames: usize, max_side: u32) -> Sampling {
    let longest = wide.max(high).max(1);
    let stretch = stretch && wide != high;
    let sizes = |step: u32| match stretch {
        true => (longest.div_ceil(step), longest.div_ceil(step)),
        false => (wide.div_ceil(step), high.div_ceil(step)),
    };
    let fits = |step: u32| {
        let (across, down) = sizes(step);
        u64::from(across) * u64::from(down) * 4 * frames.max(1) as u64 <= MAX_BYTES as u64
    };
    let mut step = longest.div_ceil(max_side.max(1)).max(1);
    while step < longest && !fits(step) {
        step += 1;
    }
    let pick = |size: u32| -> Vec<u32> {
        let size = size.max(1);
        if stretch {
            let most = u64::from(sizes(step).0);
            (0..most).map(|i| ((2 * i + 1) * u64::from(size) / (2 * most)) as u32).collect()
        } else {
            (0..size.div_ceil(step)).map(|i| (i * step + step / 2).min(size - 1)).collect()
        }
    };
    Sampling { columns: pick(wide), rows: pick(high) }
}

/// Adds a frame, joined with the one before where they are the same.
fn push(frames: &mut Vec<(Vec<u8>, u32)>, rgba: Vec<u8>, ticks: u32) {
    match frames.last_mut() {
        Some((last, total)) if *last == rgba => *total += ticks,
        _ => frames.push((rgba, ticks)),
    }
}

/// What `anim` shows, `stretch`ed square as the sticker is, or not, at
/// most `max_side` pixels along its longer side.
pub fn of_art(anim: &PixelAnim, stretch: bool, max_side: u32) -> Picture {
    let grid = anim.grid();
    let (columns, rows) = (grid.columns(), grid.rows());
    let sizes = columns.windows(2).chain(rows.windows(2)).map(|edges| edges[1] - edges[0]);
    let unit = sizes.fold(0, gcd).max(1);
    let (width, height) = (*columns.last().unwrap_or(&0), *rows.last().unwrap_or(&0));
    let sampling = sampling(width / unit, height / unit, stretch, anim.frames().len(), max_side);
    // the cell each square shown is in
    let cells = |edges: &[u32], squares: &[u32]| -> Vec<usize> {
        let cell = |square: u32| edges.partition_point(|&edge| edge <= square * unit).max(1) - 1;
        squares.iter().map(|&square| cell(square)).collect()
    };
    let (xs, ys) = (cells(columns, &sampling.columns), cells(rows, &sampling.rows));
    let mut frames = Vec::new();
    for frame in anim.frames() {
        let mut rgba = Vec::with_capacity(xs.len() * ys.len() * 4);
        for &y in &ys {
            let row = &frame.pixels[y * anim.width() as usize..];
            for &x in &xs {
                rgba.extend(anim.palette()[row[x] as usize]);
            }
        }
        push(&mut frames, rgba, frame.ticks);
    }
    Picture { width: xs.len() as u32, height: ys.len() as u32, frames }
}

/// What the Lottie `json` of a `.tgs` tgradish wrote shows, as [`of_art`]
/// draws the art it was made of, at most `max_side` pixels along its
/// longer side. Lottie with more than tgradish writes is refused, as is
/// more JSON than Telegram's apps play
/// ([`MAX_RAW_JSON`]) and a sticker longer
/// than the 3 seconds Telegram allows.
pub fn of_lottie(json: &[u8], max_side: u32) -> Result<Picture> {
    if json.len() > MAX_RAW_JSON {
        return Err(unsupported("more JSON than Telegram's apps play"));
    }
    // serde_json stops at 128 levels of nesting, far more than is drawn
    let root: Value = serde_json::from_slice(json).map_err(|err| invalid(err.to_string()))?;
    Sticker::read(&root)?.draw(max_side)
}

/// When a layer or a group shows: as hold keyframes on its opacity,
/// `(frame, shown)` in order, or always where there are none.
#[derive(Debug, Default)]
struct Shown(Vec<(f64, bool)>);

impl Shown {
    fn at(&self, frame: f64) -> bool {
        match self.0.iter().rposition(|&(time, _)| time <= frame) {
            Some(index) => self.0[index].1,
            None => self.0.first().is_none_or(|&(_, shown)| shown),
        }
    }

    /// The frames it changes at.
    fn changes(&self) -> impl Iterator<Item = f64> + '_ {
        self.0.iter().map(|&(time, _)| time).filter(|time| time.is_finite())
    }
}

/// Where a layer's coordinates land on the canvas: `position + scale *
/// point`, in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Transform {
    position: [f64; 2],
    scale: [f64; 2],
}

/// A group: rectangles, `[left, top, right, bottom]` in layer coordinates,
/// filled with one colour.
#[derive(Debug)]
struct Group {
    rects: Vec<[f64; 4]>,
    colour: [u8; 4],
    shown: Shown,
}

#[derive(Debug)]
struct Layer {
    /// Shown for frames `from..to`.
    from: f64,
    to: f64,
    shown: Shown,
    /// Bottom first.
    groups: Vec<Group>,
}

/// Lottie as tgradish writes it.
#[derive(Debug)]
struct Sticker {
    /// Its frames, `start..end`, whole numbers.
    start: f64,
    end: f64,
    /// 60 fps frames each of its frames shows for.
    ticks: u32,
    transform: Transform,
    /// Bottom first.
    layers: Vec<Layer>,
}

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| invalid(format!("{what} isn't an object")))
}

fn field<'a>(object: &'a Map<String, Value>, key: &str, what: &str) -> Result<&'a Value> {
    object.get(key).ok_or_else(|| invalid(format!("{what} has no \"{key}\"")))
}

fn number(value: &Value, what: &str) -> Result<f64> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid(format!("{what} isn't a number")))
}

fn array<'a>(value: &'a Value, what: &str) -> Result<&'a [Value]> {
    value.as_array().map(Vec::as_slice).ok_or_else(|| invalid(format!("{what} isn't a list")))
}

/// Whether `value` is the number `n`.
fn is(value: &Value, n: f64) -> bool {
    value.as_f64() == Some(n)
}

/// Whether `value` is an empty list.
fn empty(value: &Value) -> bool {
    value.as_array().is_some_and(Vec::is_empty)
}

/// Fails on a key of `object` none of `known` takes as it is: what
/// would draw something tgradish doesn't.
fn only(
    object: &Map<String, Value>,
    what: &str,
    known: impl Fn(&str, &Value) -> bool,
) -> Result<()> {
    // names, indices and the like, which change nothing that is drawn
    const NAMES: &[&str] = &["nm", "mn", "ln", "cl", "ix", "cix", "np", "ind"];
    match object.iter().find(|(key, value)| !NAMES.contains(&key.as_str()) && !known(key, value)) {
        Some((key, _)) => Err(unsupported(format!("{what} with \"{key}\""))),
        None => Ok(()),
    }
}

/// The value of a property that doesn't change.
fn fixed<'a>(property: &'a Value, what: &str) -> Result<&'a Value> {
    let object = object(property, what)?;
    if object.get("a").is_some_and(|animated| !is(animated, 0.0)) {
        return Err(unsupported(format!("animation of {what}")));
    }
    only(object, what, |key, _| matches!(key, "a" | "k"))?;
    field(object, "k", what)
}

/// The two numbers of a fixed property, and a third, which must be
/// `third`, where there is one.
fn pair(property: &Value, what: &str, third: f64) -> Result<[f64; 2]> {
    let value = array(fixed(property, what)?, what)?;
    let numbers = value.iter().map(|n| number(n, what)).collect::<Result<Vec<f64>>>()?;
    match numbers[..] {
        [x, y] => Ok([x, y]),
        [x, y, z] if z == third => Ok([x, y]),
        [_, _, _] => Err(unsupported(format!("{what} in 3D"))),
        _ => Err(invalid(format!("{what} isn't two numbers"))),
    }
}

/// Fails, as having `what`, on a property at `key` that isn't the fixed
/// `value`, which changes nothing; missing, it is.
fn neutral(object: &Map<String, Value>, key: &str, what: &str, value: &[f64]) -> Result<()> {
    let Some(property) = object.get(key) else { return Ok(()) };
    let numbers: Option<Vec<f64>> = match fixed(property, what) {
        Ok(Value::Array(items)) => items.iter().map(Value::as_f64).collect(),
        Ok(other) => other.as_f64().map(|n| vec![n]),
        Err(_) => None,
    };
    match numbers {
        Some(numbers)
            if numbers.len() >= value.len() && numbers.iter().zip(value).all(|(a, b)| a == b) =>
        {
            Ok(())
        }
        _ => Err(unsupported(what)),
    }
}

/// Shown or hidden, as an opacity of 100 or 0.
fn opacity(value: &Value, what: &str) -> Result<bool> {
    let value = match value {
        Value::Array(items) if items.len() == 1 => &items[0],
        value => value,
    };
    let percent = number(value, what)?;
    if percent == 100.0 {
        Ok(true)
    } else if percent == 0.0 {
        Ok(false)
    } else {
        Err(unsupported("partly transparent layers or groups"))
    }
}

/// When what has the opacity `property` shows: always, never, or by hold
/// keyframes.
fn shown(property: Option<&Value>) -> Result<Shown> {
    let what = "opacity";
    let Some(property) = property else { return Ok(Shown::default()) };
    let property = object(property, what)?;
    only(property, what, |key, value| match key {
        "k" => true,
        "a" => is(value, 0.0) || is(value, 1.0),
        _ => false,
    })?;
    let value = field(property, "k", what)?;
    if !property.get("a").is_some_and(|animated| is(animated, 1.0)) {
        return Ok(Shown(vec![(f64::NEG_INFINITY, opacity(value, what)?)]));
    }
    let keys = array(value, what)?;
    if keys.is_empty() {
        return Err(invalid("an opacity has no keyframes"));
    }
    if keys.len() > TLOTTIE.max_keyframes {
        return Err(unsupported("more keyframes than Telegram's apps play"));
    }
    let mut read = Vec::with_capacity(keys.len());
    for key in keys {
        let key = object(key, "a keyframe")?;
        // easing, which held keys don't use
        only(key, "a keyframe", |key, _| matches!(key, "t" | "s" | "e" | "h" | "i" | "o"))?;
        let time = number(field(key, "t", "a keyframe")?, "a keyframe's time")?;
        let shown = opacity(field(key, "s", "a keyframe")?, what)?;
        let held = key.get("h").is_some_and(|held| is(held, 1.0));
        read.push((time, shown, held));
    }
    for pair in read.windows(2) {
        let ((time, shown, held), (next, after, _)) = (pair[0], pair[1]);
        if next < time {
            return Err(invalid("keyframes out of order"));
        }
        if !held && shown != after {
            return Err(unsupported("fading"));
        }
    }
    Ok(Shown(read.into_iter().map(|(time, shown, _)| (time, shown)).collect()))
}

/// A frame number, which must be whole.
fn frame(value: &Value, what: &str) -> Result<f64> {
    let frame = number(value, what)?;
    if frame.fract() != 0.0 {
        return Err(unsupported("frames that start between frames"));
    }
    Ok(frame)
}

/// The name of a kind of shape, for saying it isn't drawn.
fn shape_name(kind: &str) -> String {
    let name = match kind {
        "sh" => "paths",
        "el" => "ellipses",
        "sr" => "stars",
        "st" => "strokes",
        "gf" => "gradients",
        "gs" => "gradient strokes",
        "mm" => "merged paths",
        "tm" => "trimmed paths",
        "rp" => "repeaters",
        "rd" => "rounded corners",
        "op" => "offset paths",
        "pb" => "pucker and bloat",
        "tw" => "twists",
        "zz" => "zig-zags",
        "gr" => "groups in groups",
        _ => return format!("shapes of the kind \"{kind}\""),
    };
    name.to_owned()
}

impl Sticker {
    fn read(root: &Value) -> Result<Sticker> {
        let root = object(root, "the JSON")?;
        only(root, "the animation", |key, value| match key {
            "v" | "fr" | "ip" | "op" | "w" | "h" | "layers" | "tgs" | "markers" | "meta" => true,
            "ddd" => is(value, 0.0),
            "assets" | "fonts" | "chars" => empty(value) || value.as_object().is_some(),
            _ => false,
        })?;
        // fonts and characters only draw in text layers, which aren't drawn
        if root.get("assets").is_some_and(|assets| !empty(assets)) {
            return Err(unsupported("assets"));
        }
        let size = |key| number(field(root, key, "the animation")?, "the canvas's size");
        let (width, height) = (size("w")?, size("h")?);
        let canvas = f64::from(telegram::CANVAS);
        if (width, height) != (canvas, canvas) {
            return Err(unsupported(format!("a canvas of {width}×{height}")));
        }
        let rate = number(field(root, "fr", "the animation")?, "the frame rate")?;
        let fps = f64::from(telegram::FPS);
        if !(rate >= 1.0 && rate.fract() == 0.0 && (fps % rate) == 0.0) {
            return Err(unsupported(format!("a frame rate of {rate}")));
        }
        let ticks = (fps / rate) as u32;
        let start = frame(field(root, "ip", "the animation")?, "its first frame")?;
        let end = frame(field(root, "op", "the animation")?, "its last frame")?;
        if end <= start {
            return Err(invalid("it has no frames"));
        }
        if (end - start) * f64::from(ticks) > f64::from(telegram::MAX_FRAMES) {
            return Err(unsupported("more than 3 seconds of frames"));
        }
        let mut layers = Vec::new();
        let mut transform = None;
        // listed top first
        for layer in array(field(root, "layers", "the animation")?, "the layers")?.iter().rev() {
            let Some((placed, layer)) = Layer::read(layer)? else { continue };
            if *transform.get_or_insert(placed) != placed {
                return Err(unsupported("layers placed differently"));
            }
            layers.push(layer);
        }
        let transform = transform.ok_or_else(|| unsupported("no layers"))?;
        Ok(Sticker { start, end, ticks, transform, layers })
    }
}

impl Layer {
    /// The layer, and where it is placed; `None` for a hidden one.
    fn read(layer: &Value) -> Result<Option<(Transform, Layer)>> {
        let what = "a layer";
        let layer = object(layer, what)?;
        if layer.get("hd").is_some_and(|hidden| hidden == &Value::Bool(true)) {
            return Ok(None);
        }
        if !layer.get("ty").is_some_and(|kind| is(kind, 4.0)) {
            return Err(unsupported("layers other than shape layers"));
        }
        only(layer, what, |key, value| match key {
            "ty" | "ks" | "ip" | "op" | "shapes" => true,
            "st" | "ddd" | "ao" | "bm" => is(value, 0.0),
            "sr" => is(value, 1.0),
            "hd" | "hasMask" => value == &Value::Bool(false),
            "masksProperties" | "ef" => empty(value),
            _ => false,
        })?;
        let ks = object(field(layer, "ks", what)?, "a layer's transform")?;
        only(ks, "a layer's transform", |key, _| {
            matches!(key, "p" | "s" | "a" | "r" | "o" | "sk" | "sa" | "rx" | "ry" | "rz" | "or")
        })?;
        let position =
            ks.get("p").map_or(Ok([0.0, 0.0]), |p| pair(p, "a layer's position", 0.0))?;
        let scale =
            ks.get("s").map_or(Ok([100.0, 100.0]), |s| pair(s, "a layer's scale", 100.0))?;
        neutral(ks, "a", "a layer's anchor away from its origin", &[0.0, 0.0])?;
        for key in ["r", "sk", "rx", "ry", "rz"] {
            neutral(ks, key, "rotated or skewed layers", &[0.0])?;
        }
        neutral(ks, "or", "rotated or skewed layers", &[0.0, 0.0, 0.0])?;
        let transform = Transform { position, scale: scale.map(|percent| percent / 100.0) };
        let from = frame(field(layer, "ip", what)?, "a layer's first frame")?;
        let to = frame(field(layer, "op", what)?, "a layer's last frame")?;
        let mut groups = Vec::new();
        let shapes = layer.get("shapes").map_or(Ok(&[][..]), |shapes| array(shapes, "shapes"))?;
        for item in shapes.iter().rev() {
            let item = object(item, "a shape")?;
            if item.get("hd").is_some_and(|hidden| hidden == &Value::Bool(true)) {
                continue;
            }
            match item.get("ty").and_then(Value::as_str) {
                Some("gr") => groups.push(Group::read(item)?),
                Some(kind @ ("rc" | "fl" | "tr")) => {
                    return Err(unsupported(format!("\"{kind}\" outside groups")));
                }
                Some(kind) => return Err(unsupported(shape_name(kind))),
                None => return Err(invalid("a shape has no kind")),
            }
        }
        let shown = shown(ks.get("o"))?;
        Ok(Some((transform, Layer { from, to, shown, groups })))
    }
}

impl Group {
    /// Rectangles, then their fill, then the group's transform, which
    /// tgradish writes: anything else is refused.
    fn read(group: &Map<String, Value>) -> Result<Group> {
        only(group, "a group", |key, value| match key {
            "ty" | "it" => true,
            "bm" => is(value, 0.0),
            "hd" => value == &Value::Bool(false),
            _ => false,
        })?;
        let items =
            group.get("it").map_or(Ok(&[][..]), |items| array(items, "a group's shapes"))?;
        let (mut rects, mut colour, mut shown) = (Vec::new(), None, Shown::default());
        let refused = || unsupported("groups other than a fill over rectangles");
        for (index, item) in items.iter().enumerate() {
            let item = object(item, "a shape")?;
            if item.get("hd").is_some_and(|hidden| hidden == &Value::Bool(true)) {
                continue;
            }
            match item.get("ty").and_then(Value::as_str) {
                Some("rc") if colour.is_none() => {
                    if let Some(rect) = rect(item)? {
                        rects.push(rect);
                    }
                }
                Some("fl") if colour.is_none() => colour = Some(fill(item)?),
                // last, as Lottie has it
                Some("tr") if colour.is_some() && index == items.len() - 1 => {
                    shown = transform(item)?;
                }
                Some("rc" | "fl" | "tr") => return Err(refused()),
                Some(kind) => return Err(unsupported(shape_name(kind))),
                None => return Err(invalid("a shape has no kind")),
            }
        }
        // nothing is filled without a fill
        let colour = colour.unwrap_or([0; 4]);
        Ok(Group { rects, colour, shown })
    }
}

/// A rectangle's edges, `None` where it has no area.
fn rect(item: &Map<String, Value>) -> Result<Option<[f64; 4]>> {
    only(item, "a rectangle", |key, value| match key {
        "ty" | "p" | "s" | "r" => true,
        // drawn clockwise, which only paths would tell from the other way
        "d" => is(value, 1.0) || is(value, 2.0),
        "hd" => value == &Value::Bool(false),
        _ => false,
    })?;
    neutral(item, "r", "rounded rectangles", &[0.0])?;
    let [x, y] = pair(field(item, "p", "a rectangle")?, "a rectangle's position", 0.0)?;
    let [width, height] = pair(field(item, "s", "a rectangle")?, "a rectangle's size", 0.0)?;
    if width < 0.0 || height < 0.0 {
        return Err(unsupported("rectangles of a negative size"));
    }
    if width == 0.0 || height == 0.0 {
        return Ok(None);
    }
    let edges = [x - width / 2.0, y - height / 2.0, x + width / 2.0, y + height / 2.0];
    if edges.iter().any(|edge| !edge.is_finite()) {
        return Err(invalid("a rectangle is too large"));
    }
    Ok(Some(edges))
}

/// A fill's colour: renderers truncate `255 * fraction`, as tgradish
/// writes them for.
fn fill(item: &Map<String, Value>) -> Result<[u8; 4]> {
    only(item, "a fill", |key, value| match key {
        "ty" | "c" | "o" => true,
        "r" => is(value, 1.0),
        "bm" => is(value, 0.0),
        "fillEnabled" => value.is_boolean(),
        "hd" => value == &Value::Bool(false),
        _ => false,
    })?;
    if item.get("fillEnabled") == Some(&Value::Bool(false)) {
        return Ok([0; 4]);
    }
    let what = "a fill's colour";
    let channels = array(fixed(field(item, "c", "a fill")?, what)?, what)?;
    let channels = channels.iter().map(|n| number(n, what)).collect::<Result<Vec<f64>>>()?;
    let truncated = |fraction: f64| (fraction.clamp(0.0, 1.0) * 255.0) as u8;
    let [r, g, b] = match channels[..] {
        [r, g, b] | [r, g, b, _] => [r, g, b].map(truncated),
        _ => return Err(invalid("a fill's colour isn't three numbers")),
    };
    let alpha = match item.get("o") {
        Some(o) => number(fixed(o, "a fill's opacity")?, "a fill's opacity")?,
        None => 100.0,
    };
    Ok([r, g, b, truncated(alpha / 100.0)])
}

/// When a group shows, from its transform, which must place it as it is.
fn transform(item: &Map<String, Value>) -> Result<Shown> {
    let what = "a group's transform";
    only(item, what, |key, _| {
        matches!(key, "ty" | "p" | "a" | "s" | "r" | "o" | "sk" | "sa" | "hd")
    })?;
    neutral(item, "p", "groups moved", &[0.0, 0.0])?;
    neutral(item, "a", "groups moved", &[0.0, 0.0])?;
    neutral(item, "s", "groups scaled", &[100.0, 100.0])?;
    neutral(item, "r", "groups rotated", &[0.0])?;
    neutral(item, "sk", "groups skewed", &[0.0])?;
    shown(item.get("o"))
}

/// Where the art is along one side of the canvas, in layer coordinates:
/// from `low` to `high`, each within `tolerance` of where it was;
/// `guessed` where the file doesn't say.
#[derive(Debug, Clone, Copy)]
struct Extent {
    low: f64,
    high: f64,
    tolerance: f64,
    guessed: bool,
}

/// How far off what the transform's numbers give may be, written with 4
/// decimals: the middle of the canvas and half its length, in layer
/// coordinates, along a side laid out at `position + scale * point`.
fn canvas(position: f64, scale: f64) -> ((f64, f64), (f64, f64)) {
    let half = f64::from(telegram::CANVAS) / 2.0;
    let middle = (half - position) / scale;
    let (written, percent) = (5e-5, 5e-7);
    let off = 2.0 * (written / scale + middle.abs() * percent / scale) + 1e-9;
    let off_half = 2.0 * half / scale * percent / scale + 1e-9;
    ((middle, off), (half / scale, off_half))
}

impl Extent {
    /// Where the art is along a side, laid out at `position + scale *
    /// point` and centred, and `stretched` to fill the canvas or not, from
    /// `edges`, the first and last rectangle edge along it, if any.
    ///
    /// tgradish puts the art pixels' inner edges at halves (layout.rs),
    /// so an edge that isn't is where the art starts or ends, cut at the
    /// side, and the art's middle is the canvas's. Where neither end
    /// shows, the art is taken as filling the canvas when stretched, and
    /// otherwise as not cut, where its middle allows, or else as cut at
    /// its end only, as canvases and crops mostly cut art: guesses
    /// [`Extent::fill`] corrects for the longer side.
    fn of(position: f64, scale: f64, edges: Option<(f64, f64)>, stretched: bool) -> Extent {
        let ((middle, off), _) = canvas(position, scale);
        let on_half = |edge: f64| ((edge + 0.5) - (edge + 0.5).round()).abs() <= EDGE;
        let (first, last) = edges.map_or((None, None), |(first, last)| (Some(first), Some(last)));
        let shown = |low, high, tolerance| Extent { low, high, tolerance, guessed: false };
        match (first.filter(|&e| !on_half(e)), last.filter(|&e| !on_half(e))) {
            (Some(low), Some(high)) => shown(low, high, EDGE),
            (Some(low), None) => shown(low, 2.0 * middle - low, 2.0 * off),
            (None, Some(high)) => shown(2.0 * middle - high, high, 2.0 * off),
            (None, None) if stretched => Extent::filling(position, scale),
            (None, None) => {
                let whole = 2.0 * middle + 1.0;
                let (low, high, tolerance) = if (whole - whole.round()).abs() <= 2.0 * off {
                    (-0.5, whole.round() - 0.5, 0.0)
                } else {
                    (-0.5, whole - 0.5, 2.0 * off)
                };
                Extent { low, high, tolerance, guessed: true }
            }
        }
    }

    /// The art filling the canvas along a side, as it does along the
    /// longer side and both stretched.
    fn filling(position: f64, scale: f64) -> Extent {
        let ((middle, off), (half, off_half)) = canvas(position, scale);
        let tolerance = off + off_half;
        Extent { low: middle - half, high: middle + half, tolerance, guessed: true }
    }

    fn length(&self) -> f64 {
        self.high - self.low
    }

    /// The extents of a sticker that isn't stretched, its longer side
    /// filling the canvas: guessed shorter, the longer guessed one fills.
    fn fill(mut extents: [Extent; 2], position: [f64; 2], scale: [f64; 2]) -> [Extent; 2] {
        let (_, (half, off_half)) = canvas(position[0], scale[0]);
        let longest = extents.iter().map(Extent::length).fold(0.0, f64::max);
        let tolerance = extents.iter().map(|e| e.tolerance).fold(0.0, f64::max) + 2.0 * off_half;
        if longest >= 2.0 * half - 2.0 * tolerance - EDGE {
            return extents;
        }
        let guessed = (0..2).filter(|&side| extents[side].guessed);
        if let Some(side) =
            guessed.max_by(|&a, &b| extents[a].length().total_cmp(&extents[b].length()))
        {
            let filling = Extent::filling(position[side], scale[side]);
            // cut by less than an art pixel
            let cut = -0.5 - filling.low;
            if (-filling.tolerance..1.0 + filling.tolerance).contains(&cut) {
                extents[side] = filling;
            }
        }
        extents
    }

    /// How far the art starts before its first whole art pixel, and how
    /// far it ends past its last's start, in art pixels, with how far off
    /// they may be.
    fn offsets(&self) -> [(f64, f64); 2] {
        [(-0.5 - self.low, self.tolerance), (self.high + 0.5, self.tolerance)]
    }
}

/// The smallest number of squares an art pixel is cut into that puts
/// both sides' ends on the lattice.
fn divisions(extents: &[Extent; 2]) -> Result<i64> {
    let offsets: Vec<(f64, f64)> = extents.iter().flat_map(Extent::offsets).collect();
    let near = |n: i64| {
        offsets.iter().all(|&(offset, tolerance)| {
            let at = offset * n as f64;
            (at - at.round()).abs() <= tolerance * n as f64 + 1e-9
        })
    };
    let most = offsets.iter().map(|&(_, tolerance)| tolerance).fold(0.0, f64::max);
    (1..=MAX_DIVISIONS)
        .take_while(|&n| most * n as f64 <= 0.05)
        .find(|&n| near(n))
        .ok_or_else(|| unsupported("shapes off a pixel grid"))
}

/// A side of the lattice: its squares, how many of them come before the
/// first whole art pixel, and how many make an art pixel.
#[derive(Debug, Clone, Copy)]
struct Side {
    squares: i64,
    before: i64,
    divisions: i64,
}

impl Side {
    fn of(extent: &Extent, divisions: i64) -> Result<Side> {
        let [(before, _), (after, _)] = extent.offsets();
        let (before, after) =
            ((before * divisions as f64).round(), (after * divisions as f64).round());
        let squares = before + after;
        if !(1.0..=MAX_LATTICE as f64).contains(&squares) || before < 0.0 {
            return Err(unsupported("shapes off a pixel grid"));
        }
        Ok(Side { squares: squares as i64, before: before as i64, divisions })
    }

    /// Where `edge`, in layer coordinates, is on the lattice, if it is on
    /// a line of it within the art.
    fn line(&self, edge: f64) -> Option<i64> {
        let at = (edge + 0.5) * self.divisions as f64 + self.before as f64;
        let line = at.round();
        let near = (at - line).abs() <= EDGE * self.divisions as f64 + 1e-6;
        (near && (0.0..=self.squares as f64).contains(&line)).then_some(line as i64)
    }

    /// The middles of `squares`, in layer coordinates.
    fn middles(&self, squares: &[u32]) -> Vec<f64> {
        let at =
            |square: u32| (f64::from(square) + 0.5 - self.before as f64) / self.divisions as f64;
        squares.iter().map(|&square| at(square) - 0.5).collect()
    }
}

/// `colour` painted over `pixel`, straight RGBA.
fn paint(pixel: &mut [u8], colour: [u8; 4]) {
    if colour[3] == 255 || pixel[3] == 0 {
        pixel.copy_from_slice(&colour);
        return;
    }
    if colour[3] == 0 {
        return;
    }
    // what tgradish never writes: translucent colours over others
    let (over, under) = (f32::from(colour[3]) / 255.0, f32::from(pixel[3]) / 255.0);
    let alpha = over + under * (1.0 - over);
    for (channel, &painted) in pixel.iter_mut().zip(&colour).take(3) {
        let mixed = f32::from(painted) * over + f32::from(*channel) * under * (1.0 - over);
        *channel = (mixed / alpha).round() as u8;
    }
    pixel[3] = (alpha * 255.0).round() as u8;
}

impl Sticker {
    fn draw(&self, max_side: u32) -> Result<Picture> {
        let Transform { position, scale } = self.transform;
        if !(scale[0] > 0.0 && scale[1] > 0.0) {
            return Err(unsupported("layers flipped or scaled to nothing"));
        }
        let stretched = (scale[0] - scale[1]).abs() > 1e-6 * scale[0].max(scale[1]) + 2e-6;
        let rects = || self.layers.iter().flat_map(|layer| &layer.groups).flat_map(|g| &g.rects);
        let span = |low: usize, high: usize| {
            rects().fold(None, |span: Option<(f64, f64)>, rect| {
                let (a, b) = span.unwrap_or((rect[low], rect[high]));
                Some((a.min(rect[low]), b.max(rect[high])))
            })
        };
        let mut extents = [
            Extent::of(position[0], scale[0], span(0, 2), stretched),
            Extent::of(position[1], scale[1], span(1, 3), stretched),
        ];
        if !stretched {
            extents = Extent::fill(extents, position, scale);
        }
        // within the canvas, where players show it
        for (extent, (position, scale)) in extents.iter().zip(position.into_iter().zip(scale)) {
            // as far off as the transform's numbers, written with 4 decimals
            let canvas = f64::from(telegram::CANVAS);
            let margin = extent.tolerance + EDGE + 2.0 * (5e-5 + canvas * 5e-7 / scale) / scale;
            if extent.low < -position / scale - margin
                || extent.high > (canvas - position) / scale + margin
            {
                return Err(unsupported("shapes outside the canvas"));
            }
        }
        let divisions = divisions(&extents)?;
        let (across, down) = (Side::of(&extents[0], divisions)?, Side::of(&extents[1], divisions)?);
        let off_grid = || unsupported("shapes off a pixel grid");
        for rect in rects() {
            let lines = [
                across.line(rect[0]),
                down.line(rect[1]),
                across.line(rect[2]),
                down.line(rect[3]),
            ];
            if lines.contains(&None) {
                return Err(off_grid());
            }
        }

        // where what shows changes, and so the frames of the picture
        let mut changes = vec![self.start, self.end];
        for layer in &self.layers {
            changes.extend([layer.from, layer.to]);
            changes.extend(layer.shown.changes().map(f64::ceil));
            for group in &layer.groups {
                changes.extend(group.shown.changes().map(f64::ceil));
            }
        }
        changes.retain(|&change| (self.start..=self.end).contains(&change));
        changes.sort_by(f64::total_cmp);
        changes.dedup();
        let sampling = sampling(
            across.squares as u32,
            down.squares as u32,
            stretched,
            changes.len() - 1,
            max_side,
        );
        let (xs, ys) = (across.middles(&sampling.columns), down.middles(&sampling.rows));
        let (width, height) = (xs.len(), ys.len());
        // the pixels each rectangle covers, as ranges of columns and rows
        let covered = |middles: &[f64], low: f64, high: f64| {
            (middles.partition_point(|&m| m < low), middles.partition_point(|&m| m < high))
        };
        let groups: Vec<Vec<Vec<[usize; 4]>>> = self
            .layers
            .iter()
            .map(|layer| {
                let rects = |group: &Group| {
                    let covers = |r: &[f64; 4]| {
                        let ((left, right), (top, bottom)) =
                            (covered(&xs, r[0], r[2]), covered(&ys, r[1], r[3]));
                        [left, top, right, bottom]
                    };
                    let rects = group.rects.iter().map(covers);
                    rects
                        .filter(|[left, top, right, bottom]| left < right && top < bottom)
                        .collect()
                };
                layer.groups.iter().map(rects).collect()
            })
            .collect();

        let mut frames = Vec::new();
        let mut work = 0u64;
        // which group last painted each pixel, so none paints one twice
        let mut painted = vec![usize::MAX; width * height];
        let mut group_index = 0;
        for span in changes.windows(2) {
            let (at, ticks) = (span[0], ((span[1] - span[0]) as u32) * self.ticks);
            let mut rgba = vec![0u8; width * height * 4];
            for (layer, rects) in self.layers.iter().zip(&groups) {
                if !(layer.from <= at && at < layer.to && layer.shown.at(at)) {
                    continue;
                }
                for (group, rects) in layer.groups.iter().zip(rects) {
                    if !group.shown.at(at) || group.colour[3] == 0 {
                        continue;
                    }
                    group_index += 1;
                    for &[left, top, right, bottom] in rects {
                        work += ((right - left) * (bottom - top)) as u64;
                        if work > MAX_WORK {
                            return Err(unsupported("more to draw than a sticker has"));
                        }
                        for y in top..bottom {
                            for x in left..right {
                                let index = y * width + x;
                                if painted[index] != group_index {
                                    painted[index] = group_index;
                                    paint(&mut rgba[index * 4..index * 4 + 4], group.colour);
                                }
                            }
                        }
                    }
                }
            }
            push(&mut frames, rgba, ticks);
        }
        Ok(Picture { width: width as u32, height: height as u32, frames })
    }
}

#[cfg(test)]
mod tests {
    use tgradish_frames::{Animation, DecodeOptions, Limits, decode};

    use super::*;
    use crate::encode::{Effort, Settings, painter};
    use crate::layout::lay_out;
    use crate::lottie::Style;
    use crate::normalise::{self, Rect, Square, normalise};
    use crate::reduce::Reduction;

    /// The Lottie tgradish writes of `anim`, marked as core marks it.
    fn lottie(anim: &PixelAnim, stretch: bool) -> String {
        let fast = Settings { effort: Effort::Fast, ..Settings::default() };
        let scene = painter(anim, &fast, None, &|| false).unwrap();
        let mut lottie = lay_out(&scene, anim, Some("tgradish".into()), stretch);
        crate::mark::embed(&mut lottie, b"a mark of a few bytes");
        lottie.to_json(Style::default())
    }

    fn inputs() -> Vec<(String, Animation)> {
        let options = (DecodeOptions::default(), Limits::default());
        tgradish_corpus::files()
            .into_iter()
            .map(|file| (file.name, decode(&file.bytes, &options.0, &options.1).unwrap()))
            .collect()
    }

    /// Ways of reading art: whole, squared three ways, on its canvas,
    /// on a forced grid, and cropped through its art pixels.
    fn readings(width: u32, height: u32) -> Vec<normalise::Options> {
        let cut = Rect { x: 1, y: 3, width: width - 2, height: height - 5 };
        let square = |square| normalise::Options { square, ..Default::default() };
        let cropped = |square| normalise::Options { crop: Some(cut), square, ..Default::default() };
        vec![
            normalise::Options::default(),
            square(Square::Crop),
            square(Square::Stretch),
            normalise::Options { keep_canvas: true, ..Default::default() },
            normalise::Options { pixel_scale: Some(3), ..Default::default() },
            cropped(Square::Pad),
            cropped(Square::Crop),
            cropped(Square::Stretch),
            normalise::Options { keep_canvas: true, ..cropped(Square::Pad) },
        ]
    }

    #[test]
    fn reads_back_what_it_wrote() {
        let reductions = [
            None,
            Some(Reduction::Downscale { factor: 0.5 }),
            Some(Reduction::Downscale { factor: 0.7 }),
            Some(Reduction::SnapToGrid { scale: 2 }),
            Some(Reduction::MergeColours { distance: 0.1 }),
            Some(Reduction::MergeFrames { changed: 0.2 }),
            Some(Reduction::DropFrames { share: 0.5 }),
            Some(Reduction::Despeckle),
        ];
        let (mut cases, mut ambiguous) = (0, 0);
        for (name, input) in inputs() {
            for options in readings(input.width(), input.height()) {
                let (anim, _) = normalise(input.clone(), &options).unwrap();
                let stretch = options.square == Square::Stretch;
                for reduction in &reductions {
                    let anim = match reduction {
                        Some(reduction) => reduction.apply(&anim).unwrap(),
                        None => anim.clone(),
                    };
                    let json = lottie(&anim, stretch);
                    let case = format!("{name}, {options:?}, {reduction:?}");
                    let read = of_lottie(json.as_bytes(), 1024).unwrap();
                    let drawn = of_art(&anim, stretch, 1024);
                    cases += 1;
                    if read != drawn {
                        // art cut where nothing shows, which the file
                        // can't tell from art that isn't cut: the same on
                        // the canvas
                        assert!(anim.grid().cut_at_sides(), "{case}");
                        same_on_canvas(&read, &drawn, stretch, &case);
                        ambiguous += 1;
                        continue;
                    }
                    // every so many squares
                    for max_side in [37, 5] {
                        let read = of_lottie(json.as_bytes(), max_side);
                        let drawn = of_art(&anim, stretch, max_side);
                        assert_eq!(read.as_ref(), Ok(&drawn), "{case}, {max_side}");
                    }
                }
            }
        }
        assert!(cases > 500 && ambiguous * 20 < cases, "{ambiguous} of {cases}");
    }

    #[test]
    fn reads_back_fitted_stickers() {
        // made as tgradish makes them, fitted by giving up some of the art
        let mut fitted = 0;
        for (name, input) in inputs() {
            for square in [Square::Pad, Square::Crop, Square::Stretch] {
                let options = crate::Options {
                    normalise: normalise::Options { square, ..Default::default() },
                    effort: Effort::Fast,
                    max_bytes: 2500,
                    mark: Some(b"marked".to_vec()),
                    name: Some("tgradish".into()),
                    ..Default::default()
                };
                let sticker = crate::make(input.clone(), &options, &mut |_| {}, &|| false).unwrap();
                fitted += usize::from(!sticker.steps.is_empty());
                let json = crate::file::unpack(&sticker.tgs).unwrap();
                let drawn = of_art(&sticker.anim, square == Square::Stretch, 1024);
                assert_eq!(of_lottie(&json, 1024), Ok(drawn), "{name}, {square:?}");
            }
        }
        assert!(fitted > 5, "{fitted}");
    }

    /// A small sticker's Lottie, as JSON to change.
    fn sticker() -> Value {
        let walker = inputs().into_iter().find(|(name, _)| name == "walker.gif").unwrap().1;
        let (anim, _) = normalise(walker, &Default::default()).unwrap();
        serde_json::from_str(&lottie(&anim, false)).unwrap()
    }

    fn drawn(json: &Value) -> Result<Picture> {
        of_lottie(json.to_string().as_bytes(), 1024)
    }

    #[test]
    fn refuses_what_tgradish_doesnt_draw() {
        use serde_json::json;
        let unchanged = sticker();
        let original = drawn(&unchanged).unwrap();
        type Change = fn(&mut Value);
        let changes: &[(&str, Change)] = &[
            ("a path", |sticker| sticker["layers"][0]["shapes"][0]["it"][0]["ty"] = "sh".into()),
            ("an ellipse", |sticker| {
                sticker["layers"][0]["shapes"][0]["it"][0]["ty"] = "el".into()
            }),
            ("a gradient", |sticker| {
                let items = &mut sticker["layers"][0]["shapes"][0]["it"];
                let fill = items.as_array().unwrap().len() - 2;
                items[fill]["ty"] = "gf".into();
            }),
            ("a stroke", |sticker| {
                let items = sticker["layers"][0]["shapes"][0]["it"].as_array_mut().unwrap();
                items.insert(0, json!({"ty": "st", "c": {"k": [1, 0, 0]}}));
            }),
            ("a mask", |sticker| sticker["layers"][0]["masksProperties"] = json!([{"mode": "a"}])),
            ("a matte", |sticker| sticker["layers"][0]["tt"] = 1.into()),
            ("a parent", |sticker| sticker["layers"][0]["parent"] = 2.into()),
            ("an image layer", |sticker| sticker["layers"][0]["ty"] = 2.into()),
            ("an effect", |sticker| sticker["layers"][0]["ef"] = json!([{"ty": 5}])),
            ("another canvas", |sticker| sticker["w"] = 100.into()),
            ("another frame rate", |sticker| sticker["fr"] = 24.into()),
            ("too long", |sticker| sticker["op"] = 600.into()),
            ("half shown", |sticker| sticker["layers"][0]["ks"]["o"] = json!({"k": 50})),
            ("fading", |sticker| {
                let keys = json!([{"t": 0, "s": [100]}, {"t": 10, "s": [0]}]);
                sticker["layers"][0]["ks"]["o"] = json!({"a": 1, "k": keys});
            }),
            ("rotated", |sticker| sticker["layers"][0]["ks"]["r"] = json!({"k": 45})),
            ("moving", |sticker| {
                let keys = json!([{"t": 0, "s": [0, 0], "h": 1}, {"t": 10, "s": [5, 5], "h": 1}]);
                sticker["layers"][0]["ks"]["p"] = json!({"a": 1, "k": keys});
            }),
            ("placed apart", |sticker| {
                let layers = sticker["layers"].as_array_mut().unwrap();
                let mut moved = layers[0].clone();
                moved["ks"]["p"] = json!({"k": [3, 7]});
                layers.push(moved);
            }),
            ("moved groups", |sticker| {
                let items = sticker["layers"][0]["shapes"][0]["it"].as_array_mut().unwrap();
                *items.last_mut().unwrap() = json!({"ty": "tr", "p": {"k": [1, 0]}});
            }),
            ("rounded", |sticker| {
                sticker["layers"][0]["shapes"][0]["it"][0]["r"] = json!({"k": 2});
            }),
            ("off the grid", |sticker| {
                sticker["layers"][0]["shapes"][0]["it"][0]["p"] = json!({"k": [0.3, 0.25]});
            }),
            ("an expression", |sticker| {
                sticker["layers"][0]["ks"]["p"]["x"] = "time * 10".into();
            }),
            ("assets", |sticker| sticker["assets"] = json!([{"id": "a", "layers": []}])),
            ("no layers", |sticker| sticker["layers"] = json!([])),
        ];
        for (what, change) in changes {
            let mut changed = unchanged.clone();
            change(&mut changed);
            let refused = drawn(&changed);
            assert!(matches!(refused, Err(DrawError::Unsupported(_))), "{what}: {refused:?}");
        }
        // what changes nothing drawn is taken as it is
        let mut named = unchanged.clone();
        named["nm"] = "another name".into();
        named["layers"][0]["nm"] = "a layer".into();
        named["layers"][0]["ind"] = 1.into();
        named["layers"][0]["shapes"][0]["it"][0]["nm"] = "a rectangle".into();
        named["meta"] = json!({"g": "some exporter"});
        assert_eq!(drawn(&named), Ok(original.clone()));
        // and hidden layers aren't drawn, whatever they are
        let mut hidden = unchanged.clone();
        hidden["layers"].as_array_mut().unwrap().insert(0, json!({"ty": 2, "hd": true}));
        assert_eq!(drawn(&hidden), Ok(original));
        // nor is what isn't Lottie
        for broken in [&b""[..], b"{}", b"[]", b"null", b"\x1f\x8b\x08", b"{\"layers\":7}"] {
            assert!(of_lottie(broken, 1024).is_err(), "{broken:?}");
        }
        let large = format!("{{\"nm\":\"{}\"}}", "x".repeat(MAX_RAW_JSON));
        assert!(matches!(of_lottie(large.as_bytes(), 1024), Err(DrawError::Unsupported(_))));
    }

    #[test]
    fn survives_broken_files() {
        use serde_json::json;
        let bytes = sticker().to_string().into_bytes();
        // cut short anywhere
        for end in (0..bytes.len()).step_by(7) {
            let _ = of_lottie(&bytes[..end], 1024);
        }
        // bytes changed here and there, the same on every machine
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..400 {
            let mut changed = bytes.clone();
            for _ in 0..3 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let at = (seed % changed.len() as u64) as usize;
                changed[at] = b"0123456789-.,:[]{}\"e "[(seed >> 32) as usize % 21];
            }
            let _ = of_lottie(&changed, 1024);
        }
        // nested past what serde reads, and numbers past what is drawn
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(matches!(of_lottie(deep.as_bytes(), 1024), Err(DrawError::Invalid(_))));
        type Change = fn(&mut Value);
        let changes: &[Change] = &[
            |sticker| sticker["layers"][0]["ks"]["s"] = json!({"k": [1e-300, 1e-300]}),
            |sticker| sticker["layers"][0]["ks"]["s"] = json!({"k": [0, 0]}),
            |sticker| sticker["layers"][0]["ks"]["s"] = json!({"k": [-100, 100]}),
            |sticker| sticker["layers"][0]["ks"]["p"] = json!({"k": [1e300, 0]}),
            |sticker| sticker["layers"][0]["shapes"][0]["it"][0]["s"] = json!({"k": [1e300, 1]}),
            |sticker| sticker["layers"][0]["shapes"][0]["it"][0]["s"] = json!({"k": [-2, 1]}),
            |sticker| sticker["layers"][0]["shapes"][0]["it"][0]["p"] = json!({"k": [1]}),
            |sticker| sticker["op"] = json!(1e300),
            |sticker| sticker["layers"][0]["ks"]["o"] = json!({"a": 1, "k": []}),
            |sticker| sticker["layers"][0]["ks"]["o"] = json!({"a": 1, "k": [{"t": 5}]}),
        ];
        for change in changes {
            let mut changed = sticker();
            change(&mut changed);
            assert!(drawn(&changed).is_err(), "{changed}");
        }
        // a layer from long before, which shows from the start
        let mut early = sticker();
        early["layers"][0]["ip"] = json!(-1e300);
        assert!(drawn(&early).is_ok());
    }

    #[test]
    fn stays_within_its_memory() {
        use serde_json::json;
        // art 4000 pixels wide, a pixel of it changing every frame for 3
        // seconds: more than a picture may hold of it, a pixel a pixel
        let layers: Vec<Value> = (0..180)
            .map(|frame| {
                let at = frame * 20;
                let rect = json!({"ty": "rc", "p": {"k": [at, at]}, "s": {"k": [1, 1]}});
                let fill = json!({"ty": "fl", "c": {"k": [1, 0, 0]}});
                let ks = json!({"p": {"k": [0.064, 0.064]}, "s": {"k": [12.8, 12.8]}});
                let group = json!({"ty": "gr", "it": [rect, fill, {"ty": "tr"}]});
                json!({"ty": 4, "ip": frame, "op": frame + 1, "ks": ks, "shapes": [group]})
            })
            .collect();
        let sticker = json!({"fr": 60, "ip": 0, "op": 180, "w": 512, "h": 512, "layers": layers});
        let picture = drawn(&sticker).unwrap();
        let bytes: usize = picture.frames.iter().map(|(rgba, _)| rgba.len()).sum();
        assert!(bytes <= MAX_BYTES && picture.width < 1024, "{} {bytes}", picture.width);
        assert_eq!(picture.frames.iter().map(|&(_, ticks)| ticks).sum::<u32>(), 180);
        // as the art it would be made of is drawn
        let shown = sampling(4000, 4000, false, 180, 1024);
        assert_eq!(
            (picture.width, picture.height),
            (shown.columns.len() as u32, shown.rows.len() as u32)
        );
        assert_eq!(shown, sampling(4000, 4000, true, 180, 1024));
        let few = sampling(4000, 4000, false, 1, 1024);
        assert_eq!((few.columns.len(), few.columns[..2].to_vec()), (1000, vec![2, 6]));
    }

    /// Asserts that `a` and `b` show the same on the canvas, as a sticker
    /// places them: in each 60 fps frame, at the middle of each pixel.
    fn same_on_canvas(a: &Picture, b: &Picture, stretch: bool, case: &str) {
        const CANVAS: i64 = telegram::CANVAS as i64;
        // the pixel of `picture` at the middle of canvas pixel `at`, along
        // a side it is `size` pixels long
        let at = |picture: &Picture, size: u32, at: i64| -> Option<usize> {
            let longest = i64::from(picture.width.max(picture.height));
            let (size, most) = (i64::from(size), if stretch { i64::from(size) } else { longest });
            let twice = (2 * at + 1) * most - CANVAS * (most - size);
            (twice >= 0 && twice < 2 * CANVAS * size).then_some((twice / (2 * CANVAS)) as usize)
        };
        let ticks = |picture: &Picture| -> Vec<usize> {
            let frames = picture.frames.iter().enumerate();
            frames
                .flat_map(|(index, &(_, ticks))| std::iter::repeat_n(index, ticks as usize))
                .collect()
        };
        let (a_ticks, b_ticks) = (ticks(a), ticks(b));
        assert_eq!(a_ticks.len(), b_ticks.len(), "{case}");
        let mut pairs: Vec<(usize, usize)> = a_ticks.into_iter().zip(b_ticks).collect();
        pairs.dedup();
        for (a_frame, b_frame) in pairs {
            for y in 0..CANVAS {
                for x in 0..CANVAS {
                    let pixel = |picture: &Picture, frame: usize| {
                        let column = at(picture, picture.width, x);
                        let row = at(picture, picture.height, y);
                        match (column, row) {
                            (Some(column), Some(row)) => {
                                let index = (row * picture.width as usize + column) * 4;
                                picture.frames[frame].0[index..index + 4].try_into().unwrap()
                            }
                            _ => [0u8; 4],
                        }
                    };
                    assert_eq!(pixel(a, a_frame), pixel(b, b_frame), "{case}: {x}, {y}");
                }
            }
        }
    }
}
