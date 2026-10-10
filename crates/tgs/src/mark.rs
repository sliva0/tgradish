//! Hides bits in the order of rectangles within groups. A group's fill
//! paints all of its rectangles together, so their order changes nothing
//! that is drawn, and a JSON re-encoding keeps it.
//!
//! In each group the rectangles are sorted by their top, left, height and
//! width; then each pair in that order, first and second, then third and
//! fourth, and so on, holds a bit: 1 when the pair is written swapped.
//! Groups are read in the order they are written. Pairs of equal
//! rectangles hold nothing.
//!
//! Stickers with too few rectangles for a copy carry the payload as the
//! first layer's name (`ln`) instead, in hex: a few bytes more, and a
//! name like any other.

use crate::lottie::{Animation, Item};

/// Where a rectangle is, as the sort key: top, left, height, width in
/// millionths, so written and read numbers sort alike.
type Key = [i64; 4];

fn key(centre: [f64; 2], size: [f64; 2]) -> Key {
    let micro = |value: f64| (value * 1e6).round() as i64;
    [
        micro(centre[1] - size[1] / 2.0),
        micro(centre[0] - size[0] / 2.0),
        micro(size[1]),
        micro(size[0]),
    ]
}

/// How many copies of a payload [`embed`] writes at most: each costs
/// about 0.6% of a sticker's size, as swapped pairs break up repeats.
const COPIES: usize = 1;

/// Hides `payload` in the order of `animation`'s rectangles, or as its first
/// layer's name when they have no room for it. False when it has no layers
/// either.
pub fn embed(animation: &mut Animation, payload: &[u8]) -> bool {
    let room = capacity(animation);
    let copies = (room / (payload.len() * 8).max(1)).min(COPIES);
    if copies == 0 {
        let Some(first) = animation.layers.first_mut() else { return false };
        first.id = Some(payload.iter().map(|byte| format!("{byte:02x}")).collect());
        return true;
    }
    let bits: Vec<bool> = std::iter::repeat_n(payload, copies)
        .flatten()
        .flat_map(|&byte| (0..8).rev().map(move |bit| byte >> bit & 1 == 1))
        .collect();
    let mut bits = bits.into_iter();
    for layer in &mut animation.layers {
        for item in &mut layer.items {
            if let Item::Group(items) = item {
                write_group(items, &mut bits);
            }
        }
    }
    true
}

/// The payload a sticker carries as its first layer's name, if it does.
pub fn layer_name(json: &[u8]) -> Option<Vec<u8>> {
    let value = serde_json::from_slice::<serde_json::Value>(json).ok()?;
    let name = value.get("layers")?.get(0)?.get("ln")?.as_str()?;
    if name.len() % 2 != 0 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..name.len()).step_by(2).map(|i| u8::from_str_radix(&name[i..i + 2], 16).ok()).collect()
}

/// How many bits `animation`'s rectangles can hold.
pub fn capacity(animation: &Animation) -> usize {
    let groups = animation.layers.iter().flat_map(|layer| &layer.items);
    groups
        .filter_map(|item| match item {
            Item::Group(items) => Some(pairs(&keys(items))),
            _ => None,
        })
        .sum()
}

fn keys(items: &[Item]) -> Vec<Key> {
    let mut keys: Vec<Key> = items
        .iter()
        .filter_map(|item| match *item {
            Item::Rect { centre, size } => Some(key(centre, size)),
            _ => None,
        })
        .collect();
    keys.sort_unstable();
    keys
}

/// Pairs of sorted keys that can hold a bit.
fn pairs(sorted: &[Key]) -> usize {
    sorted.as_chunks::<2>().0.iter().filter(|[a, b]| a != b).count()
}

/// Sorts a group's rectangles, swapping pairs for the bits taken.
fn write_group(items: &mut [Item], bits: &mut impl Iterator<Item = bool>) {
    let places: Vec<usize> =
        (0..items.len()).filter(|&i| matches!(items[i], Item::Rect { .. })).collect();
    let mut rects: Vec<(Key, Item)> = places
        .iter()
        .map(|&i| match items[i] {
            Item::Rect { centre, size } => (key(centre, size), items[i].clone()),
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

/// The bits hidden in a Lottie document's rectangles, in order, or none
/// when it doesn't parse.
pub fn hidden_bits(json: &[u8]) -> Vec<bool> {
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
    let pair = |value: Option<&serde_json::Value>| -> Option<[f64; 2]> {
        let values = value?.get("k")?.as_array()?;
        Some([values.first()?.as_f64()?, values.get(1)?.as_f64()?])
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
                transform: Transform { position: [0.0, 0.0], scale: 1.0 },
                items: vec![group(0), group(100)],
            }],
        }
    }

    #[test]
    fn reads_back_hidden_bytes() {
        let mut lottie = animation(100);
        assert_eq!(capacity(&lottie), 100);
        let payload = [0b1011_0001, 0x5a, 0xff, 0x00];
        assert!(embed(&mut lottie, &payload));
        let json = lottie.to_json(Style::default());
        let bits = hidden_bits(json.as_bytes());
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
    fn hides_nothing_where_there_is_no_room() {
        let mut lottie = animation(3);
        assert_eq!(capacity(&lottie), 2);
        // the first layer's name holds it instead
        assert!(embed(&mut lottie, &[1, 0xab]));
        let json = lottie.to_json(Style::default());
        assert_eq!(hidden_bits(json.as_bytes()), [false, false]);
        assert!(json.starts_with(r#"{"v""#) && json.contains(r#"{"ln":"01ab","ty":4,"#), "{json}");
        assert_eq!(layer_name(json.as_bytes()), Some(vec![1, 0xab]));
        assert!(hidden_bits(b"not json").is_empty() && layer_name(b"{}").is_none());
    }
}
