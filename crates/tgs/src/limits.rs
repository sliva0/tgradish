//! What a `.tgs` file has to stay within to be accepted by Telegram and
//! drawn by every client. Output is checked against all of these.

use serde::Serialize;

/// Telegram's rules for animated stickers, from
/// <https://core.telegram.org/stickers> and
/// <https://core.telegram.org/animated_stickers> (checked 2026-10-08), and
/// the limits its server has beyond them.
pub mod telegram {
    /// The canvas is exactly this many pixels wide and high.
    pub const CANVAS: u32 = 512;
    pub const FPS: u32 = 60;
    /// At most 3 seconds at 60 fps: `op - ip <= 180`.
    pub const MAX_FRAMES: u32 = 180;
    /// Size of the gzipped file: exactly 64 KiB was accepted, a byte more
    /// refused.
    pub const MAX_BYTES: usize = 64 * 1024;

    // Found by uploading probes (`docs/probes.md`): past these, the server
    // keeps a `.tgs` as a plain file instead of making it a sticker, and
    // @Stickers answers "File type is invalid".

    /// The size of the whole animation, as [`cost`] counts it. Bytes of
    /// JSON don't count: the same shapes were accepted in 1.24 MB as in 1
    /// MB. Probes put a layer at 8.2 to 8.9 shapes and the limit between
    /// 24 300 and 24 580; this and [`LAYER_COST`] round towards refusing.
    pub const MAX_COST: usize = 24_000;
    /// What a layer counts for in [`cost`], in shapes.
    pub const LAYER_COST: usize = 9;
    /// 1500 layers were accepted, 1750 refused, though both cost less than
    /// [`MAX_COST`].
    pub const MAX_LAYERS: usize = 1_500;
    /// Shapes in one layer, counted like tlottie counts them (groups, their
    /// fills and transforms, and what they draw): 4093 were accepted, 4103
    /// refused.
    pub const MAX_SHAPES_PER_LAYER: usize = 4_096;
    /// Path vertices one fill paints (the paths before it in its group):
    /// 8000 were accepted, 12 000 refused, while 12 000 under three fills
    /// passed, in one layer or three, and so did 40 000 under ten. Points
    /// add little or nothing to [`MAX_COST`]: those 40 000 would have come
    /// to 24 000 at 0.6 shapes a point.
    pub const MAX_PAINT_POINTS: usize = 8_000;

    /// `shapes` in all layers, counted like [`MAX_SHAPES_PER_LAYER`], and
    /// `layers`, as Telegram's server weighs them against [`MAX_COST`].
    pub const fn cost(shapes: usize, layers: usize) -> usize {
        shapes + LAYER_COST * layers
    }
}

/// Telegram Desktop refuses Lottie JSON larger than this (`kMaxFileSize` in
/// desktop-app/lib_lottie). Output stays well below it.
pub const MAX_RAW_JSON: usize = 2 * 1024 * 1024;

/// Default parse limits of tlottie, the renderer in Telegram's current
/// clients; Telegram Android passes no limits of its own. From tlottie's
/// `src/composition/limits.rs`, only the ones tgradish's output can get
/// near.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RendererLimits {
    pub max_input_bytes: usize,
    /// Arrays and objects inside each other in the JSON.
    pub max_nesting_depth: usize,
    pub max_layers: usize,
    /// Shape layers that paint something.
    pub max_painted_shape_layers: usize,
    pub max_shapes_per_layer: usize,
    /// Fills and strokes per layer.
    pub max_paints_per_layer: usize,
    /// Geometry items (rectangles, paths) that paints in one layer draw.
    pub max_paint_source_items_per_layer: usize,
    pub max_path_points: usize,
    pub max_path_coordinate_abs: f32,
    pub max_keyframes: usize,
    pub max_assets: usize,
    pub max_precomp_expansion: usize,
    /// Groups inside groups; a constant in tlottie (`MAX_GROUP_DEPTH`).
    pub max_group_depth: usize,
}

pub const TLOTTIE: RendererLimits = RendererLimits {
    max_input_bytes: 16 << 20,
    max_nesting_depth: 175,
    max_layers: 2_720,
    max_painted_shape_layers: 2_715,
    max_shapes_per_layer: 20_480,
    max_paints_per_layer: 5_120,
    max_paint_source_items_per_layer: 5_120,
    max_path_points: 4_355,
    max_path_coordinate_abs: 165_389.0,
    max_keyframes: 2_048,
    max_assets: 256,
    max_precomp_expansion: 18_005,
    max_group_depth: 65,
};
