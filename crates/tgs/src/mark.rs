//! Encodes a byte payload in the ordering of rectangles within shape groups.
//! A group's fill paints all its rectangles together, so reordering them does
//! not affect rendering and survives JSON re-encoding.
//!
//! Within each group, rectangles are sorted by
//! `(2*top, 2*left, height, width)`.
//! Each disjoint consecutive pair (`[0, 1]`, `[2, 3]`, ...) carries one bit:
//! `1` if written swapped, `0` if kept in sorted order. Groups are read in the
//! order they are written. Pairs of identical rectangles carry no bit.
//!
//! Stickers with too few rectangles store the hex-encoded payload in the first
//! layer's `ln` field instead (adding only a few bytes, formatted like an
//! ordinary layer name).

use crate::lottie::{Animation, Item};

/// Rectangle sort key: `[2*top, 2*left, height, width]` in millionths.
///
/// Both writer and reader compute the key from numbers rounded to 6 decimal
/// places (matching JSON output) in whole millionths, so they always agree:
/// computed from unrounded floats, two rectangles sharing a top edge (such as
/// art pixels clipped to one-third by a crop) could round apart at 6 decimals
/// and sort in different orders. Uses `i128` so inputs bounded by `i64` in
/// [`micro`] cannot overflow.
type Key = [i128; 4];

fn key(centre: [i64; 2], size: [i64; 2]) -> Key {
    let ([x, y], [width, height]) = (centre.map(i128::from), size.map(i128::from));
    [2 * y - height, 2 * x - width, height, width]
}

/// Converts a JSON number to whole millionths (exact for reader inputs, which
/// are written with at most 6 decimal places).
fn micro(value: f64) -> i64 {
    (value * 1e6).round() as i64
}

/// Rounds `value` to 6 decimal places as [`crate::lottie`] writes it, then
/// converts to whole millionths.
fn written(value: f64) -> i64 {
    micro(format!("{value:.6}").parse().expect("Rust prints numbers it can read"))
}

/// Sort key of a rectangle after JSON rounding.
fn written_key(centre: [f64; 2], size: [f64; 2]) -> Key {
    key(centre.map(written), size.map(written))
}

internal! {
/// Embeds `payload` once in the rectangle order of `animation`, or in the
/// first layer's `ln` field if capacity is insufficient. Returns `false` if
/// `animation` has no layers. Only one copy is embedded because swapping pairs
/// breaks deflate matches (~0.6% size overhead per copy).
pub fn embed(animation: &mut Animation, payload: &[u8]) -> bool {
    if capacity(animation) < payload.len() * 8 {
        let Some(first) = animation.layers.first_mut() else { return false };
        first.id = Some(payload.iter().map(|byte| format!("{byte:02x}")).collect());
        return true;
    }
    let mut bits =
        payload.iter().flat_map(|&byte| (0..8).rev().map(move |bit| byte >> bit & 1 == 1));
    for layer in &mut animation.layers {
        for item in &mut layer.items {
            if let Item::Group(items) = item {
                write_group(items, &mut bits);
            }
        }
    }
    true
}
}

/// Decodes a hex payload from the first layer's `ln` field, if present and
/// valid.
pub fn layer_name(json: &[u8]) -> Option<Vec<u8>> {
    let value = serde_json::from_slice::<serde_json::Value>(json).ok()?;
    let name = value.get("layers")?.get(0)?.get("ln")?.as_str()?;
    if name.len() % 2 != 0 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..name.len()).step_by(2).map(|i| u8::from_str_radix(&name[i..i + 2], 16).ok()).collect()
}

internal! {
/// Returns how many bits `animation`'s rectangles can encode.
pub fn capacity(animation: &Animation) -> usize {
    let groups = animation.layers.iter().flat_map(|layer| &layer.items);
    groups
        .filter_map(|item| match item {
            Item::Group(items) => Some(pairs(&keys(items))),
            _ => None,
        })
        .sum()
}
}

fn keys(items: &[Item]) -> Vec<Key> {
    let mut keys: Vec<Key> = items
        .iter()
        .filter_map(|item| match *item {
            Item::Rect { centre, size } => Some(written_key(centre, size)),
            _ => None,
        })
        .collect();
    keys.sort_unstable();
    keys
}

/// Counts distinct consecutive pairs in `sorted` that can carry a bit.
fn pairs(sorted: &[Key]) -> usize {
    sorted.as_chunks::<2>().0.iter().filter(|[a, b]| a != b).count()
}

/// Sorts a group's rectangles and swaps pairs according to `bits`.
fn write_group(items: &mut [Item], bits: &mut impl Iterator<Item = bool>) {
    let places: Vec<usize> =
        (0..items.len()).filter(|&i| matches!(items[i], Item::Rect { .. })).collect();
    let mut rects: Vec<(Key, Item)> = places
        .iter()
        .map(|&i| match items[i] {
            Item::Rect { centre, size } => (written_key(centre, size), items[i].clone()),
            _ => unreachable!("only rectangles were picked"),
        })
        .collect();
    rects.sort_by_key(|(key, _)| *key);
    for pair in rects.as_chunks_mut::<2>().0 {
        if pair[0].0 != pair[1].0 && bits.next() == Some(true) {
            pair.swap(0, 1);
        }
    }
    for (place, (_, rect)) in places.into_iter().zip(rects) {
        items[place] = rect;
    }
}

/// Extracts the bits encoded by rectangle order across all groups in `json`.
/// Returns an empty vector if `json` is not valid JSON.
pub fn order_bits(json: &[u8]) -> Vec<bool> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(json) else { return Vec::new() };
    let mut bits = Vec::new();
    let layers = value.get("layers").and_then(|layers| layers.as_array());
    for layer in layers.into_iter().flatten() {
        let shapes = layer.get("shapes").and_then(|shapes| shapes.as_array());
        for group in shapes.into_iter().flatten() {
            if group.get("ty").and_then(|ty| ty.as_str()) != Some("gr") {
                continue;
            }
            let items = group.get("it").and_then(|items| items.as_array());
            read_group(items.map_or(&[][..], Vec::as_slice), &mut bits);
        }
    }
    bits
}

fn read_group(items: &[serde_json::Value], bits: &mut Vec<bool>) {
    let pair = |value: Option<&serde_json::Value>| -> Option<[i64; 2]> {
        let values = value?.get("k")?.as_array()?;
        Some([micro(values.first()?.as_f64()?), micro(values.get(1)?.as_f64()?)])
    };
    // (key, place in the file)
    let mut rects: Vec<(Key, usize)> = items
        .iter()
        .filter(|item| item.get("ty").and_then(|ty| ty.as_str()) == Some("rc"))
        .enumerate()
        .filter_map(|(place, item)| Some((key(pair(item.get("p"))?, pair(item.get("s"))?), place)))
        .collect();
    rects.sort_unstable();
    for pair in rects.as_chunks::<2>().0 {
        if pair[0].0 != pair[1].0 {
            bits.push(pair[0].1 > pair[1].1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lottie::{Layer, Style, Transform};
    use crate::scene::FillRule;

    fn rect(x: f64, y: f64) -> Item {
        Item::Rect { centre: [x, y], size: [1.0, 1.0] }
    }

    fn animation(rects: usize) -> Animation {
        let group = |offset: usize| {
            let mut items: Vec<Item> =
                (0..rects).map(|i| rect((i % 7) as f64, (i / 7 + offset) as f64)).collect();
            items.push(Item::Fill { colour: [255, 0, 0, 255], rule: FillRule::NonZero });
            items.push(Item::GroupTransform { opacity: Vec::new() });
            Item::Group(items)
        };
        Animation {
            name: None,
            ticks: 60,
            layers: vec![Layer {
                id: None,
                from: 0,
                to: 60,
                hidden: Vec::new(),
                transform: Transform { position: [0.0, 0.0], scale: [1.0, 1.0] },
                items: vec![group(0), group(100)],
            }],
        }
    }

    #[test]
    fn reads_rectangles_however_far_off_they_are() {
        let rect = |x: &str| format!(r#"{{"ty":"rc","p":{{"k":[{x},{x}]}},"s":{{"k":[1,{x}]}}}}"#);
        let far = ["1e300", "-1e20", "2", "0"].map(rect).join(",");
        let json = format!(r#"{{"layers":[{{"shapes":[{{"ty":"gr","it":[{far}]}}]}}]}}"#);
        assert_eq!(order_bits(json.as_bytes()).len(), 2);
    }

    #[test]
    fn reads_back_bytes_from_the_order() {
        let mut lottie = animation(100);
        assert_eq!(capacity(&lottie), 100);
        let payload = [0b1011_0001, 0x5a, 0xff, 0x00];
        assert!(embed(&mut lottie, &payload));
        let json = lottie.to_json(Style::default());
        let bits = order_bits(json.as_bytes());
        let expected: Vec<bool> = payload
            .iter()
            .flat_map(|&byte| (0..8).rev().map(move |bit| byte >> bit & 1 == 1))
            .collect();
        assert_eq!(bits[..32], expected[..]);
        // the rest of the pairs stay in order
        assert!(bits[32..].iter().all(|&bit| !bit));
        // the same rectangles, in another order
        let mut sorted = animation(100);
        embed(&mut sorted, &[]);
        assert_ne!(json, sorted.to_json(Style::default()));
        assert_eq!(json.len(), sorted.to_json(Style::default()).len());
    }

    #[test]
    fn reads_back_bits_among_art_pixels_cut_to_thirds() {
        // edges of art pixels 3 input pixels wide, the first cut to 1 and 2,
        // as fractions of an art pixel: rectangles of one top have centres
        // and sizes that round apart at 6 decimals
        for cut in [1.0, 2.0] {
            let edges: Vec<f64> = (0..8)
                .map(|i| if i == 0 { 0.0 } else { (cut + f64::from(i - 1) * 3.0) / 3.0 - 0.5 })
                .collect();
            let mut items = Vec::new();
            for rows in [(0, 1), (0, 2), (1, 2), (0, 3), (1, 3), (2, 5), (0, 4), (3, 4), (1, 6)] {
                for columns in [(0, 1), (1, 2), (0, 2), (2, 4), (0, 5)] {
                    let (x0, x1, y0, y1) =
                        (edges[columns.0], edges[columns.1], edges[rows.0], edges[rows.1]);
                    items.push(Item::Rect {
                        centre: [(x0 + x1) / 2.0, (y0 + y1) / 2.0],
                        size: [x1 - x0, y1 - y0],
                    });
                }
            }
            items.push(Item::Fill { colour: [255, 0, 0, 255], rule: FillRule::NonZero });
            items.push(Item::GroupTransform { opacity: Vec::new() });
            let mut lottie = animation(0);
            lottie.layers[0].items = vec![Item::Group(items)];
            assert_eq!(capacity(&lottie), 22);
            let payload = [0b1011_0001, 0x5a];
            assert!(embed(&mut lottie, &payload));
            let bits = order_bits(lottie.to_json(Style::default()).as_bytes());
            let expected: Vec<bool> = payload
                .iter()
                .flat_map(|&byte| (0..8).rev().map(move |bit| byte >> bit & 1 == 1))
                .collect();
            assert_eq!(bits[..16], expected[..], "cut to {cut}");
            assert!(bits[16..].iter().all(|&bit| !bit), "cut to {cut}");
        }
    }

    #[test]
    fn uses_the_first_layers_name_where_there_is_no_room() {
        let mut lottie = animation(3);
        assert_eq!(capacity(&lottie), 2);
        // the first layer's name holds it instead
        assert!(embed(&mut lottie, &[1, 0xab]));
        let json = lottie.to_json(Style::default());
        assert_eq!(order_bits(json.as_bytes()), [false, false]);
        assert!(json.starts_with(r#"{"v""#) && json.contains(r#"{"ln":"01ab","ty":4,"#), "{json}");
        assert_eq!(layer_name(json.as_bytes()), Some(vec![1, 0xab]));
        assert!(order_bits(b"not json").is_empty() && layer_name(b"{}").is_none());
    }
}
