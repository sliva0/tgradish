//! The sprites, each drawn for a case real pixel art has.

use std::f32::consts::TAU;

use crate::canvas::{Canvas, Colour, Random};

const OUTLINE: Colour = [28, 22, 40, 255];
const SKIN: Colour = [240, 196, 160, 255];
const SKIN_SHADE: Colour = [196, 140, 112, 255];
const HAIR: Colour = [92, 58, 128, 255];
const HAIR_LIGHT: Colour = [138, 96, 176, 255];
const SHIRT: Colour = [60, 124, 200, 255];
const SHIRT_SHADE: Colour = [40, 84, 150, 255];
const PANTS: Colour = [66, 64, 80, 255];
const SHOE: Colour = [120, 74, 44, 255];
const WHITE: Colour = [250, 250, 250, 255];
const GOLD: Colour = [250, 204, 70, 255];
const GOLD_SHADE: Colour = [200, 140, 40, 255];
const IRON: Colour = [150, 156, 170, 255];
const IRON_LIGHT: Colour = [210, 216, 228, 255];
const WOOD: Colour = [140, 90, 50, 255];
const STONE: Colour = [110, 104, 100, 255];
const STONE_DARK: Colour = [80, 74, 72, 255];
const SLIME: Colour = [90, 200, 110, 255];
const SLIME_SHADE: Colour = [50, 150, 80, 255];
const SLIME_LIGHT: Colour = [180, 240, 170, 255];

/// A person walking in place, 32 by 32, at `phase` of a step cycle from 0
/// to 1, with an outline.
pub fn person(phase: f32, blink: bool, shirt: (Colour, Colour)) -> Canvas {
    let mut canvas = Canvas::new(32, 32);
    let swing = (phase * TAU).sin();
    // the body is lower while the legs are apart
    let bob = i32::from(swing.abs() > 0.5);
    let hip = (16, 20 + bob);
    for (side, sign) in [(-2, 1.0), (2, -1.0)] {
        let foot = (hip.0 + side + (swing * sign * 4.0).round() as i32, 28);
        canvas.line((hip.0 + side, hip.1), foot, 3, PANTS);
        canvas.rect(foot.0 - 1, foot.1, 3, 2, SHOE);
    }
    let (lit, shade) = shirt;
    canvas.ellipse(16.0, 15.5 + bob as f32, 5.5, 6.0, shade);
    canvas.ellipse(15.5, 15.0 + bob as f32, 4.5, 5.5, lit);
    for (shoulder, sign) in [(11, -1.0), (21, 1.0)] {
        let hand = (shoulder + (swing * sign * 3.0).round() as i32, 19 + bob);
        canvas.line((shoulder, 12 + bob), hand, 2, SKIN);
        canvas.set(hand.0, hand.1 + 1, SKIN_SHADE);
    }
    canvas.ellipse(16.0, 8.0 + bob as f32, 5.5, 5.5, SKIN_SHADE);
    canvas.ellipse(15.5, 7.5 + bob as f32, 5.0, 5.0, SKIN);
    canvas.ellipse(16.0, 4.5 + bob as f32, 6.0, 3.5, HAIR);
    canvas.rect(10, 4 + bob, 3, 4, HAIR);
    for x in [13, 14, 17] {
        canvas.set(x, 3 + bob, HAIR_LIGHT);
    }
    for x in [14, 18] {
        if blink {
            canvas.set(x, 9 + bob, OUTLINE);
        } else {
            canvas.rect(x, 8 + bob, 1, 2, OUTLINE);
        }
    }
    canvas.outline(OUTLINE);
    canvas
}

/// Eight steps of a walk, 100 ms each, blinking once.
pub fn walker() -> (Vec<Canvas>, Vec<u32>) {
    let frames = (0..8).map(|i| person(i as f32 / 8.0, i == 5, (SHIRT, SHIRT_SHADE))).collect();
    (frames, vec![100; 8])
}

/// A hammer swung down onto stone, 104 by 69 art pixels, with sparks where
/// it lands.
pub fn hammer() -> (Vec<Canvas>, Vec<u32>) {
    let angles: [f32; 7] = [-110.0, -75.0, -40.0, -5.0, 30.0, 55.0, 55.0];
    let frames = angles
        .iter()
        .enumerate()
        .map(|(index, &angle)| {
            let mut scene = Canvas::new(104, 69);
            // the stone the hammer lands on, with a crack after it does
            scene.rect(0, 61, 104, 8, STONE);
            for x in (2..104).step_by(9) {
                scene.rect(x, 63 + (x / 9) % 3, 3, 2, STONE_DARK);
            }
            let striker = person(0.0, false, (GOLD, GOLD_SHADE));
            scene.paste(&striker, 18, 30);
            let pivot = (40.0f32, 44.0f32);
            let radians = angle.to_radians();
            let head = (pivot.0 + 22.0 * radians.cos(), pivot.1 + 22.0 * radians.sin());
            let to = |(x, y): (f32, f32)| (x.round() as i32, y.round() as i32);
            scene.line(to(pivot), to(head), 2, WOOD);
            let across = (-radians.sin() * 6.0, radians.cos() * 6.0);
            let ends =
                [(head.0 - across.0, head.1 - across.1), (head.0 + across.0, head.1 + across.1)];
            scene.line(to(ends[0]), to(ends[1]), 5, IRON);
            scene.line(to(ends[0]), to(ends[1]), 1, IRON_LIGHT);
            if index >= 5 {
                let mut random = Random::new(index as u64);
                for _ in 0..14 {
                    let (x, y) = (
                        head.0 as i32 + random.below(21) as i32 - 10,
                        50 + random.below(11) as i32,
                    );
                    scene.set(x, y, if random.below(2) == 0 { GOLD } else { WHITE });
                }
                scene.line((60, 62), (66, 66), 1, OUTLINE);
            }
            scene
        })
        .collect();
    (frames, vec![80, 80, 80, 80, 80, 80, 100])
}

/// A slime squashing and stretching, 30 by 24.
pub fn slime() -> (Vec<Canvas>, Vec<u32>) {
    let frames = (0..6)
        .map(|i| {
            let squash = (i as f32 / 6.0 * TAU).sin() * 2.5;
            let mut canvas = Canvas::new(30, 24);
            let (rx, ry) = (11.0 + squash, 8.0 - squash);
            let floor = 22.0;
            canvas.ellipse(15.0, floor - ry, rx, ry, SLIME_SHADE);
            canvas.ellipse(14.0, floor - ry - 1.0, rx - 1.5, ry - 1.5, SLIME);
            canvas.rect(9, (floor - 2.0 * ry + 3.0) as i32, 2, 2, SLIME_LIGHT);
            let eyes = (floor - ry) as i32 - 1;
            canvas.rect(11, eyes, 2, 3, OUTLINE);
            canvas.rect(17, eyes, 2, 3, OUTLINE);
            canvas.outline(OUTLINE);
            canvas
        })
        .collect();
    (frames, vec![120; 6])
}

/// The slime at twice the size, with a few pixels per frame off the grid
/// of 2, as art scaled up and touched up by hand.
pub fn slime_jittered() -> (Vec<Canvas>, Vec<u32>) {
    let (frames, delays) = slime();
    let mut random = Random::new(7);
    let frames = frames
        .iter()
        .map(|frame| {
            let mut big = frame.scaled(2);
            let mut touched = 0;
            while touched < 4 {
                let (x, y) = (random.below(big.width) as i32, random.below(big.height) as i32);
                // one pixel, alone in its block, inside the slime
                if x % 2 == 1 && big.get(x, y) == SLIME {
                    big.set(x, y, SLIME_LIGHT);
                    touched += 1;
                }
            }
            big
        })
        .collect();
    (frames, delays)
}

/// A king bowing and rising, 44 by 83, over almost 10 seconds.
pub fn bow() -> (Vec<Canvas>, Vec<u32>) {
    let count = 43;
    let frames = (0..count)
        .map(|i| {
            let mut canvas = Canvas::new(44, 83);
            let lean = ((i as f32 / count as f32) * TAU).sin().max(0.0) * 14.0;
            // robe, then the upper body leaning forward from the waist
            canvas.ellipse(22.0, 62.0, 13.0, 19.0, [150, 40, 60, 255]);
            canvas.ellipse(21.0, 61.0, 11.0, 18.0, [196, 60, 84, 255]);
            let neck = (22 + lean.round() as i32, 30 + (lean / 3.0).round() as i32);
            canvas.line((22, 48), neck, 9, [196, 60, 84, 255]);
            canvas.ellipse(neck.0 as f32, neck.1 as f32 - 8.0, 7.0, 7.0, SKIN_SHADE);
            canvas.ellipse(neck.0 as f32 - 0.5, neck.1 as f32 - 8.5, 6.5, 6.5, SKIN);
            // the crown, and a beard
            for (dx, height) in [(-5, 5), (-2, 7), (1, 5), (4, 7)] {
                canvas.rect(neck.0 + dx, neck.1 - 15 - height + 4, 2, height, GOLD);
            }
            canvas.rect(neck.0 - 5, neck.1 - 13, 11, 2, GOLD_SHADE);
            canvas.ellipse(neck.0 as f32, neck.1 as f32 - 2.0, 5.0, 4.0, WHITE);
            canvas.outline(OUTLINE);
            canvas
        })
        .collect();
    let mut delays = vec![230; count - 1];
    delays.push(190);
    (frames, delays)
}

/// A glowing orb, 48 by 48, its halo partly transparent and pulsing.
pub fn glow() -> (Vec<Canvas>, Vec<u32>) {
    let frames = (0..5)
        .map(|i| {
            let mut canvas = Canvas::new(48, 48);
            let pulse = (i as f32 / 5.0 * TAU).sin();
            for (radius, alpha) in [(20.0 + 2.0 * pulse, 40), (16.0 + pulse, 90), (12.0, 160)] {
                canvas.ellipse(24.0, 24.0, radius, radius, [120, 200, 255, alpha]);
            }
            canvas.ellipse(24.0, 24.0, 8.0, 8.0, [90, 170, 240, 255]);
            canvas.ellipse(23.0, 23.0, 6.0, 6.0, [160, 220, 255, 255]);
            canvas.rect(20, 19, 3, 2, WHITE);
            canvas
        })
        .collect();
    (frames, vec![170; 5])
}

/// A dancer with sparkles and lit floor tiles, 40 by 42, changing a lot
/// from frame to frame.
pub fn dance() -> (Vec<Canvas>, Vec<u32>) {
    let tiles: [Colour; 4] =
        [[230, 80, 160, 255], [80, 200, 230, 255], [250, 210, 80, 255], [140, 90, 220, 255]];
    let mut random = Random::new(3);
    let frames = (0..46)
        .map(|i| {
            let mut canvas = Canvas::new(40, 42);
            for tile in 0..8 {
                let colour = tiles[(tile + i / 3) % tiles.len()];
                canvas.rect(tile as i32 * 5, 36, 5, 6, colour);
            }
            let shirt = if i % 6 < 3 { (SHIRT, SHIRT_SHADE) } else { (GOLD, GOLD_SHADE) };
            let dancer = person(i as f32 / 5.0, i % 9 == 4, shirt);
            canvas.paste(&dancer, 4 + ((i as f32 / 7.0 * TAU).sin() * 3.0).round() as i32, 4);
            for _ in 0..30 {
                let (x, y) = (random.below(40) as i32, random.below(34) as i32);
                let colour = tiles[random.below(4) as usize];
                canvas.set(x, y, colour);
                if random.below(3) == 0 {
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        canvas.set(x + dx, y + dy, WHITE);
                    }
                }
            }
            canvas
        })
        .collect();
    (frames, vec![60; 46])
}

/// A sunrise over hills in a dithered sky, 64 by 48: bands of colour mixed
/// in a 4 by 4 Bayer pattern.
pub fn sunrise() -> (Vec<Canvas>, Vec<u32>) {
    const BAYER: [[u32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let sky: [Colour; 5] = [
        [40, 30, 90, 255],
        [110, 50, 120, 255],
        [210, 90, 110, 255],
        [250, 160, 90, 255],
        [250, 220, 140, 255],
    ];
    let frames = (0..12)
        .map(|i| {
            let mut canvas = Canvas::new(64, 48);
            for y in 0..48 {
                // how far down the sky, in sixteenths of a band
                let at = (y as u32 * (sky.len() as u32 - 1) * 16) / 47;
                for x in 0..64 {
                    let (band, part) = ((at / 16) as usize, at % 16);
                    let upper = part > BAYER[y as usize % 4][x as usize % 4];
                    canvas.set(x, y, sky[(band + usize::from(upper)).min(sky.len() - 1)]);
                }
            }
            canvas.ellipse(32.0, 40.0 - i as f32 * 1.5, 7.0, 7.0, GOLD);
            canvas.ellipse(31.0, 39.0 - i as f32 * 1.5, 5.0, 5.0, [255, 240, 180, 255]);
            for (x, height) in [(0, 10), (20, 14), (44, 9)] {
                canvas.ellipse(x as f32 + 10.0, 48.0, 16.0, height as f32, [40, 70, 50, 255]);
            }
            canvas
        })
        .collect();
    (frames, vec![100; 12])
}

/// A waterfall by palette cycling, 40 by 48: the shape stays, its four
/// blues move down it.
pub fn waterfall() -> (Vec<Canvas>, Vec<u32>) {
    let water: [Colour; 4] =
        [[40, 90, 200, 255], [70, 140, 230, 255], [140, 200, 250, 255], [230, 245, 255, 255]];
    let frames = (0..8)
        .map(|i| {
            let mut canvas = Canvas::new(40, 48);
            canvas.rect(0, 0, 10, 48, STONE);
            canvas.rect(30, 0, 10, 48, STONE);
            for y in 0..48 {
                for x in 10..30 {
                    let index = (y + 8 - i + (x * 7 % 3)) as usize / 2 % water.len();
                    canvas.set(x, y, water[index]);
                }
            }
            canvas.rect(4, 10, 3, 2, STONE_DARK);
            canvas.rect(33, 30, 4, 2, STONE_DARK);
            canvas
        })
        .collect();
    (frames, vec![80; 8])
}

/// A ball bouncing, 32 by 32, its shadow partly transparent.
pub fn bounce() -> (Vec<Canvas>, Vec<u32>) {
    let frames = (0..10)
        .map(|i| {
            let mut canvas = Canvas::new(32, 32);
            let height = (i as f32 / 10.0 * TAU / 2.0).sin() * 16.0;
            let squash = if i == 0 { 2.0 } else { 0.0 };
            canvas.ellipse(16.0, 29.0, 9.0 - height / 4.0, 2.0, [0, 0, 0, 96]);
            let y = 22.0 - height + squash / 2.0;
            canvas.ellipse(16.0, y, 6.0 + squash, 6.0 - squash, [220, 60, 60, 255]);
            canvas.ellipse(15.0, y - 1.0, 4.0, 4.0 - squash / 2.0, [250, 110, 100, 255]);
            canvas.set(13, y as i32 - 3, WHITE);
            canvas
        })
        .collect();
    (frames, vec![50; 10])
}

/// Four steps, 27 by 31, the second and fourth the same: the frames of an
/// image sequence.
pub fn steps() -> Vec<Canvas> {
    [0.0, 0.25, 0.5, 0.25]
        .iter()
        .map(|&phase| {
            person(phase, false, ([200, 80, 80, 255], [150, 50, 60, 255])).cropped(3, 1, 27, 31)
        })
        .collect()
}

/// Eight frames of a wave, 28 by 42: another image sequence, of another
/// size.
pub fn wave() -> Vec<Canvas> {
    (0..8)
        .map(|i| {
            let mut canvas = Canvas::new(28, 42);
            let person = person(0.0, false, ([80, 160, 90, 255], [50, 110, 60, 255]));
            canvas.paste(&person, -2, 10);
            let raised = (i as f32 / 8.0 * TAU).sin();
            let hand = (23 + (raised * 3.0).round() as i32, 8);
            canvas.line((20, 22), hand, 2, SKIN);
            canvas
        })
        .collect()
}
