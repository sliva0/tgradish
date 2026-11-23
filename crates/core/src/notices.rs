// Shared notice catalogue included by both `build.rs` (to pack the files) and
// `licenses.rs` (to label each unpacked notice).

/// Category of an embedded licence notice. Front-ends can localise titles from
/// this enum while keeping component names verbatim. `Display` produces the
/// English title returned by [`Notice::title`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// tgradish's own licence.
    Tgradish,
    /// Licence of the named component.
    Licence {
        /// Component name.
        of: &'static str,
    },
    /// Licence of the named component, with its full licence title.
    NamedLicence {
        /// Component name.
        of: &'static str,
        /// Full licence title.
        licence: &'static str,
    },
    /// Licensing notes for the named component.
    LicenceNotes {
        /// Component name.
        of: &'static str,
    },
    /// Patent grant for the named component.
    PatentGrant {
        /// Component name.
        of: &'static str,
    },
    /// Built-in component summary and source availability notice.
    Sources {
        /// Component name.
        of: &'static str,
    },
    /// Combined third-party Rust crate licences.
    Crates,
    /// Licence for an embedded font subset (used by the GUI).
    Font {
        /// Font family or style names.
        name: &'static str,
        /// Number of embedded subsets.
        subsets: u8,
    },
}

/// Bundled notice files in display order (`tgradish` first). Each tuple is
/// `(relative_path, linked_static_only, kind)`.
pub(crate) const FILES: &[(&str, bool, Kind)] = &[
    ("LICENSE.txt", false, Kind::Tgradish),
    // Only `linked-static` builds carry ffmpeg's notices: they describe the
    // ffmpeg built for releases, which a build linked to a distribution's
    // libraries holds no copy of.
    ("licenses/ffmpeg/SOURCES.txt", true, Kind::Sources { of: "ffmpeg" }),
    (
        "licenses/ffmpeg/ffmpeg-COPYING.LGPLv2.1",
        true,
        Kind::NamedLicence { of: "ffmpeg", licence: "GNU Lesser General Public License 2.1" },
    ),
    ("licenses/ffmpeg/ffmpeg-LICENSE.md", true, Kind::LicenceNotes { of: "ffmpeg" }),
    ("licenses/ffmpeg/libvpx-LICENSE", true, Kind::Licence { of: "libvpx" }),
    ("licenses/ffmpeg/libvpx-PATENTS", true, Kind::PatentGrant { of: "libvpx" }),
    ("licenses/ffmpeg/dav1d-COPYING", true, Kind::Licence { of: "dav1d" }),
    ("licenses/ffmpeg/zlib-LICENSE", true, Kind::Licence { of: "zlib" }),
    // libwebp, which every build has, from libwebp-sys
    ("licenses/libwebp/COPYING", false, Kind::Licence { of: "libwebp" }),
    ("licenses/libwebp/PATENTS", false, Kind::PatentGrant { of: "libwebp" }),
    ("licenses/THIRD-PARTY-CRATES.txt", false, Kind::Crates),
];
