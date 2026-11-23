//! Limits that a `.tgs` file must stay within to be accepted by Telegram and
//! rendered by every client. Output is checked against all of these, except
//! where a stricter limit already covers the same bound (tlottie's layer,
//! painted-layer, and shapes-per-layer limits sit above Telegram's server
//! limits, and its input-size limit sits above Telegram Desktop's JSON limit
//! and [`crate::file::MAX_UNPACKED`]).

/// Telegram's rules for animated stickers (from
/// <https://core.telegram.org/stickers> and
/// <https://core.telegram.org/animated_stickers>, checked 2026-10-08) plus
/// the limits its server has beyond them.
pub mod telegram {
    /// Required canvas width and height in pixels.
    pub const CANVAS: u32 = 512;
    /// Required frame rate (30 fps is also accepted, against the rules).
    pub const FPS: u32 = 60;
    /// Maximum frame count: 3 seconds at 60 fps (`op - ip <= 180`).
    pub const MAX_FRAMES: u32 = 180;
    /// Maximum compressed file size: 64 KiB passes, one byte more is rejected.
    pub const MAX_BYTES: usize = 64 * 1024;

    // Measured by uploading `tgs-lab probes` outputs to @Stickers. Above
    // these limits, the server treats the `.tgs` as a plain document instead
    // of a sticker and @Stickers replies "File type is invalid".

    /// Maximum combined animation complexity counted by [`cost`]. Raw JSON
    /// size does not affect this: the same shapes passed in both 1 MB and
    /// 1.24 MB. Probes measured each layer at 8.2–8.9 shapes and the ceiling
    /// at 24 300–24 580; this constant and [`LAYER_COST`] round conservatively.
    pub const MAX_COST: usize = 24_000;
    /// Layer weight in [`cost`], measured in shapes.
    pub const LAYER_COST: usize = 9;
    /// Maximum layer count: 1 500 passed, 1 750 failed, even below
    /// [`MAX_COST`].
    pub const MAX_LAYERS: usize = 1_500;
    /// Maximum shape items in one layer, counted like tlottie (groups, fills,
    /// transforms, and geometry): 4 093 passed, 4 103 failed. No probe tested
    /// values in between, so this is the highest value confirmed to pass,
    /// rounding towards refusing like the limits above.
    pub const MAX_SHAPES_PER_LAYER: usize = 4_093;
    /// Maximum path vertices painted by one fill (all paths before it in its
    /// group): 8 000 passed, 12 000 failed, while 12 000 across three fills
    /// (one layer or three) and 40 000 across ten fills passed. Path vertices
    /// add little or nothing to [`MAX_COST`]: those 40 000 vertices would have
    /// come to 24 000 ([`MAX_COST`]) at 0.6 shapes per point.
    pub const MAX_PAINT_POINTS: usize = 8_000;

    /// Computes Telegram's server complexity from total `shapes` (counted like
    /// [`MAX_SHAPES_PER_LAYER`]) and `layers`, for comparison with
    /// [`MAX_COST`].
    pub const fn cost(shapes: usize, layers: usize) -> usize {
        shapes + LAYER_COST * layers
    }
}

/// Telegram Desktop's raw Lottie JSON limit (`kMaxFileSize` in
/// `desktop-app/lib_lottie`). Output stays well below this.
pub const MAX_RAW_JSON: usize = 2 * 1024 * 1024;

/// Default parser limits of tlottie, the renderer in current Telegram clients
/// (Telegram Android does not override them), plus its fixed `MAX_GROUP_DEPTH`
/// constant. Taken from tlottie's `src/composition/limits.rs`, keeping only
/// the limits tgradish output can approach.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct RendererLimits {
    /// Maximum uncompressed JSON bytes.
    pub max_input_bytes: usize,
    /// Maximum JSON array/object nesting depth.
    pub max_nesting_depth: usize,
    /// Maximum total layers, including precomp layers.
    pub max_layers: usize,
    /// Maximum painted shape layers.
    pub max_painted_shape_layers: usize,
    /// Maximum shape items per layer (groups, fills, transforms, and geometry).
    pub max_shapes_per_layer: usize,
    /// Maximum fills and strokes per layer.
    pub max_paints_per_layer: usize,
    /// Maximum geometry items (rectangles, paths) painted in one layer.
    pub max_paint_source_items_per_layer: usize,
    /// Maximum vertices in one path or tangent array.
    pub max_path_points: usize,
    /// Maximum absolute path coordinate value.
    pub max_path_coordinate_abs: f32,
    /// Maximum keyframes on one property.
    pub max_keyframes: usize,
    /// Maximum assets (such as precomps).
    pub max_assets: usize,
    /// Maximum layers after expanding precomps at each use.
    pub max_precomp_expansion: usize,
    /// Maximum nested shape group depth (`MAX_GROUP_DEPTH` in tlottie).
    pub max_group_depth: usize,
}

/// tlottie's limits at the version `tgs-lab` checks renders against.
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
