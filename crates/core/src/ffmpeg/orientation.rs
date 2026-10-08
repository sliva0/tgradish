use schemars::JsonSchema;
use serde::Serialize;

/// How frames are turned for display, from a video's display matrix. This
/// is what the ffmpeg command line applies when it autorotates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    #[default]
    Normal,
    /// A quarter turn clockwise.
    Rotate90,
    Rotate180,
    /// A quarter turn counterclockwise.
    Rotate270,
    FlipHorizontal,
    FlipVertical,
    /// Mirrored along the top-left to bottom-right diagonal.
    Transpose,
    /// Mirrored along the top-right to bottom-left diagonal.
    Transverse,
}

impl Orientation {
    /// Follows ffmpeg's `av_display_rotation_get` and the filter choice in
    /// `ffmpeg_filter.c`. Rotations other than quarter turns are ignored.
    pub fn from_display_matrix(m: &[i32; 9]) -> Orientation {
        let fixed = |i: usize| f64::from(m[i]) / 65536.0;
        let scale0 = fixed(0).hypot(fixed(3));
        let scale1 = fixed(1).hypot(fixed(4));
        if scale0 == 0.0 || scale1 == 0.0 {
            return Orientation::Normal;
        }
        let rotation = -(fixed(1) / scale1).atan2(fixed(0) / scale0).to_degrees();
        let mut theta = -rotation.round();
        theta -= 360.0 * (theta / 360.0 + 0.9 / 360.0).floor();

        let near = |angle: f64| (theta - angle).abs() < 1.0;
        if near(90.0) {
            if m[3] > 0 { Orientation::Transpose } else { Orientation::Rotate90 }
        } else if near(180.0) {
            match (m[0] < 0, m[4] < 0) {
                (true, true) => Orientation::Rotate180,
                (true, false) => Orientation::FlipHorizontal,
                (false, true) => Orientation::FlipVertical,
                (false, false) => Orientation::Normal,
            }
        } else if near(270.0) {
            if m[3] < 0 { Orientation::Transverse } else { Orientation::Rotate270 }
        } else if theta.abs() < 1.0 && m[4] < 0 {
            Orientation::FlipVertical
        } else {
            Orientation::Normal
        }
    }

    /// Whether width and height trade places.
    pub fn swaps_size(self) -> bool {
        matches!(
            self,
            Orientation::Rotate90
                | Orientation::Rotate270
                | Orientation::Transpose
                | Orientation::Transverse
        )
    }

    /// ffmpeg filters that apply it, each followed by a comma.
    pub fn filters(self) -> &'static str {
        match self {
            Orientation::Normal => "",
            Orientation::Rotate90 => "transpose=clock,",
            Orientation::Rotate180 => "hflip,vflip,",
            Orientation::Rotate270 => "transpose=cclock,",
            Orientation::FlipHorizontal => "hflip,",
            Orientation::FlipVertical => "vflip,",
            Orientation::Transpose => "transpose=cclock_flip,",
            Orientation::Transverse => "transpose=clock_flip,",
        }
    }
}

/// Parses ffprobe's `displaymatrix` text: three lines of three numbers,
/// each line prefixed with an offset like `00000000:`.
pub(crate) fn parse_display_matrix(text: &str) -> Option<[i32; 9]> {
    let numbers: Vec<i32> = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .flat_map(|(_, values)| values.split_whitespace().map(|v| v.parse::<i64>().ok()))
        .map(|value| value.map(|v| v as i32))
        .collect::<Option<_>>()?;
    numbers.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: i32 = 1 << 16;

    /// Matrix the way ffmpeg builds it: av_display_rotation_set, then
    /// av_display_matrix_flip.
    fn matrix(counterclockwise: f64, hflip: bool, vflip: bool) -> [i32; 9] {
        let radians = counterclockwise.to_radians();
        let (c, s) = (radians.cos(), radians.sin());
        let fixed = |v: f64| (v * f64::from(ONE)).round() as i32;
        let mut m = [fixed(c), fixed(-s), 0, fixed(s), fixed(c), 0, 0, 0, 1 << 30];
        let flip = [if hflip { -1 } else { 1 }, if vflip { -1 } else { 1 }, 1];
        for (i, value) in m.iter_mut().enumerate() {
            *value *= flip[i % 3];
        }
        m
    }

    #[test]
    fn reads_rotations_and_flips() {
        use Orientation::*;
        let cases = [
            (0.0, false, false, Normal),
            // ffmpeg's -display_rotation is counterclockwise
            (-90.0, false, false, Rotate90),
            (90.0, false, false, Rotate270),
            (180.0, false, false, Rotate180),
            (0.0, true, false, FlipHorizontal),
            (0.0, false, true, FlipVertical),
            (45.0, false, false, Normal),
        ];
        for (rotation, hflip, vflip, expected) in cases {
            let m = matrix(rotation, hflip, vflip);
            assert_eq!(
                Orientation::from_display_matrix(&m),
                expected,
                "{rotation} {hflip} {vflip}"
            );
        }
        assert_eq!(Orientation::from_display_matrix(&[0; 9]), Normal);
    }

    #[test]
    fn parses_ffprobe_text() {
        let text = "\n00000000:            0       65536           0\n\
                    00000001:       -65536           0           0\n\
                    00000002:            0           0  1073741824\n";
        let m = parse_display_matrix(text).unwrap();
        assert_eq!(m, [0, 65536, 0, -65536, 0, 0, 0, 0, 1 << 30]);
        assert_eq!(Orientation::from_display_matrix(&m), Orientation::Rotate90);
        assert_eq!(parse_display_matrix("garbage"), None);
    }
}
