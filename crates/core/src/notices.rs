// What each notice a build carries is, and the file in this crate it is
// read from, shared by build.rs, which packs those files, and `licenses`,
// which says what each one it unpacks is.

/// What a notice is the licence of, or about: front-ends word its title
/// from it in their own language, keeping the names it holds as they are.
/// Its `Display` is the title in English, as [`Notice::title`] gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// tgradish's own licence.
    Tgradish,
    /// The licence of the work named.
    Licence {
        /// The work's name.
        of: &'static str,
    },
    /// A licence of the work named, given by its own name, in full.
    NamedLicence {
        /// The work's name.
        of: &'static str,
        /// The licence's name.
        licence: &'static str,
    },
    /// Notes on how the work named is licensed.
    LicenceNotes {
        /// The work's name.
        of: &'static str,
    },
    /// The patent grant of the work named.
    PatentGrant {
        /// The work's name.
        of: &'static str,
    },
    /// What of the work named is built in, and where its sources are.
    Sources {
        /// The work's name.
        of: &'static str,
    },
    /// The licences of the Rust crates, the longest notice.
    Crates,
    /// The licence of the font named, of which a front-end has subsets
    /// built in, as the GUI does.
    Font {
        /// The font's name, or its styles' names.
        name: &'static str,
        /// How many subsets of it there are.
        subsets: u8,
    },
}

/// Each notice's file, whether only builds with ffmpeg built in carry it,
/// and what it is, in the order they are listed: tgradish's own first.
pub(crate) const FILES: &[(&str, bool, Kind)] = &[
    ("LICENSE.txt", false, Kind::Tgradish),
    // only builds with the release's ffmpeg in them carry its notices:
    // one linked to a distribution's libraries holds no copy of ffmpeg, and
    // those aren't the build these files describe
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
