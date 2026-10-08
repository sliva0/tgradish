//! What a `.tgs` file has to stay within to be accepted by Telegram and
//! drawn by every client. Output is checked against all of these.

use serde::Serialize;

/// Telegram's rules for animated stickers, from
/// <https://core.telegram.org/stickers> and
/// <https://core.telegram.org/animated_stickers> (checked 2026-10-08).
pub mod telegram {
    /// The canvas is exactly this many pixels wide and high.
    pub const CANVAS: u32 = 512;
    pub const FPS: u32 = 60;
    /// At most 3 seconds at 60 fps: `op - ip <= 180`.
    pub const MAX_FRAMES: u32 = 180;
    /// Size of the gzipped file.
    pub const MAX_BYTES: usize = 64 * 1024;
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
};
