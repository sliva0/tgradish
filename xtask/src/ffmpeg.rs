//! The minimal static ffmpeg that release builds link in, and the exact
//! sources it is made from. Everything comes from archives pinned by their
//! hashes, so a release can say what is in it and publish those very
//! sources, as the LGPL asks. Each build records its recipe, so one made
//! from other sources or flags is known to be out of date.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail, ensure};

use crate::{MUSL_GCC, Target, output, run, workspace_root};

/// An archive pinned by its hash: a library of the ffmpeg build, or a tool
/// a task downloads.
pub struct Source {
    pub name: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    /// License files to ship, relative to the source directory.
    pub licenses: &'static [&'static str],
}

// zlib and dav1d hashes match the ones their projects publish; ffmpeg and
// libvpx were pinned when first downloaded.
const ZLIB: Source = Source {
    name: "zlib",
    version: "1.3.2",
    // zlib.net sometimes serves other content to CI machines; the GitHub
    // release has the same bytes
    url: "https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz",
    sha256: "bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16",
    licenses: &["LICENSE"],
};
const LIBVPX: Source = Source {
    name: "libvpx",
    version: "1.17.0",
    url: "https://github.com/webmproject/libvpx/archive/refs/tags/v1.17.0.tar.gz",
    sha256: "1020f184046187baa2985dbde38e0691f49c44088bca7a1842b0236c6081dc0a",
    licenses: &["LICENSE", "PATENTS"],
};
const DAV1D: Source = Source {
    name: "dav1d",
    version: "1.5.4",
    url: "https://downloads.videolan.org/pub/videolan/dav1d/1.5.4/dav1d-1.5.4.tar.xz",
    sha256: "686616b7c69eb88d44459391ab25cac13b6647a3b288835c5784e71c1514a5c5",
    licenses: &["COPYING"],
};
pub const FFMPEG: Source = Source {
    name: "ffmpeg",
    version: "9.0.2",
    url: "https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz",
    sha256: "8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e",
    licenses: &["COPYING.LGPLv2.1", "LICENSE.md"],
};

/// Everything in the build, in the order it is built, since ffmpeg links
/// the other three. Recipes list them in this order too, so reordering them
/// makes every build out of date.
const LIBRARIES: [&Source; 4] = [&ZLIB, &LIBVPX, &DAV1D, &FFMPEG];

/// Everything tgradish asks ffmpeg to do, and nothing else.
const FFMPEG_COMPONENTS: &[(&str, &[&str])] = &[
    ("protocol", &["file", "pipe"]),
    (
        "demuxer",
        &[
            "mov",
            "matroska",
            "avi",
            "flv",
            "mpegts",
            "mpegps",
            "ogg",
            "ivf",
            "gif",
            "apng",
            "image2",
            "image2pipe",
            "image_png_pipe",
            "image_jpeg_pipe",
            "image_webp_pipe",
            "image_bmp_pipe",
            "image_tiff_pipe",
            "h264",
            "hevc",
            "m4v",
            "rawvideo",
        ],
    ),
    (
        "decoder",
        &[
            "h264",
            "hevc",
            "vp8",
            "vp9",
            "libvpx_vp8",
            "libvpx_vp9",
            "libdav1d",
            "mpeg4",
            "mpeg1video",
            "mpeg2video",
            "h263",
            "theora",
            "prores",
            "ffv1",
            "mjpeg",
            "png",
            "apng",
            "gif",
            "webp",
            "bmp",
            "tiff",
            "qtrle",
            "rawvideo",
        ],
    ),
    ("encoder", &["libvpx_vp9", "rawvideo", "wrapped_avframe"]),
    ("muxer", &["webm", "matroska", "null", "rawvideo"]),
    (
        "parser",
        &[
            "h264",
            "hevc",
            "vp8",
            "vp9",
            "av1",
            "mpeg4video",
            "mpegvideo",
            "mjpeg",
            "png",
            "gif",
            "webp",
        ],
    ),
    (
        "filter",
        &[
            "scale",
            // scaling in linear light, with premultiplied alpha
            "lutrgb",
            "premultiply",
            "unpremultiply",
            "fps",
            "format",
            "pad",
            "crop",
            "setsar",
            "settb",
            "setpts",
            "trim",
            "ssim",
            "null",
            "copy",
            "transpose",
            "hflip",
            "vflip",
        ],
    ),
];

/// Fails if configure silently left out a requested component, which it
/// does for misspelled names.
fn check_components(ffmpeg_dir: &Path) -> Result<()> {
    let config = std::fs::read_to_string(ffmpeg_dir.join("config_components.h"))?;
    let mut missing = Vec::new();
    for (kind, names) in FFMPEG_COMPONENTS {
        for name in *names {
            let define =
                format!("#define CONFIG_{}_{} 1", name.to_uppercase(), kind.to_uppercase());
            if !config.lines().any(|line| line.trim() == define) {
                missing.push(format!("{kind} {name}"));
            }
        }
    }
    ensure!(missing.is_empty(), "ffmpeg configure did not enable: {}", missing.join(", "));
    Ok(())
}

struct Build {
    target: Target,
    asm: bool,
    jobs: String,
    /// Downloads and extracted sources.
    sources: PathBuf,
    /// Install prefix for the libraries.
    prefix: PathBuf,
}

/// Downloads the archive of `source` into `dir`, unless it is there, and
/// verifies it.
pub fn download(source: &Source, dir: &Path) -> Result<PathBuf> {
    let file_name = source.url.rsplit('/').next().unwrap();
    let archive = dir.join(format!("{}-{file_name}", source.name));
    if !archive.exists() {
        std::fs::create_dir_all(dir)?;
        let partial = archive.with_extension("partial");
        run(Command::new("curl")
            .args(["-sSfL", "--retry", "3", "--retry-connrefused", "-o"])
            .arg(&partial)
            .arg(source.url))?;
        std::fs::rename(&partial, &archive)?;
    }
    let sum = output(Command::new("sha256sum").arg(&archive))?;
    let sum = sum.split_whitespace().next().unwrap_or_default();
    if sum != source.sha256 {
        std::fs::remove_file(&archive)?;
        bail!(
            "{} checksum mismatch: expected {}, got {sum}; the server may have sent an \
             error page instead of {}",
            source.name,
            source.sha256,
            source.url
        );
    }
    Ok(archive)
}

impl Build {
    /// Where `source` is extracted and built for this target.
    fn source_dir(&self, source: &Source) -> PathBuf {
        self.sources.join(format!("{}-{}-{}", source.name, source.version, self.target.triple()))
    }

    /// Downloads, verifies and extracts `source`. Returns the source directory.
    fn fetch(&self, source: &Source) -> Result<PathBuf> {
        let archive = download(source, &self.sources)?;

        // fresh copy per target, builds happen in the source tree
        let dir = self.source_dir(source);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        run(Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&dir)
            .arg("--strip-components=1"))?;
        Ok(dir)
    }

    fn make(&self, dir: &Path) -> Result<()> {
        run(Command::new("make").current_dir(dir).args(["-j", &self.jobs]))?;
        run(Command::new("make").current_dir(dir).arg("install"))
    }

    fn zlib(&self) -> Result<()> {
        let dir = self.fetch(&ZLIB)?;
        match self.target.cross_prefix() {
            None => {
                let mut configure = Command::new("./configure");
                configure
                    .current_dir(&dir)
                    .arg("--static")
                    .arg(format!("--prefix={}", self.prefix.display()))
                    .env("CFLAGS", "-O2 -fPIC");
                if self.target.is_musl() {
                    configure.env("CC", MUSL_GCC);
                }
                run(&mut configure)?;
                self.make(&dir)?;
            }
            Some(cross) => {
                let prefix = self.prefix.display();
                run(Command::new("make").current_dir(&dir).args([
                    "-f",
                    "win32/Makefile.gcc",
                    &format!("PREFIX={cross}"),
                    &format!("BINARY_PATH={prefix}/bin"),
                    &format!("INCLUDE_PATH={prefix}/include"),
                    &format!("LIBRARY_PATH={prefix}/lib"),
                    "-j",
                    &self.jobs,
                    "install",
                ]))?;
            }
        }
        Ok(())
    }

    fn libvpx(&self) -> Result<()> {
        let dir = self.fetch(&LIBVPX)?;
        let target = match (self.target, self.asm) {
            (_, false) => "generic-gnu",
            (Target::LinuxX64 | Target::LinuxMuslX64, true) => "x86_64-linux-gcc",
            (Target::LinuxArm64 | Target::LinuxMuslArm64, true) => "arm64-linux-gcc",
            (Target::WindowsX64, true) => "x86_64-win64-gcc",
        };
        let mut configure = Command::new("./configure");
        configure
            .current_dir(&dir)
            .arg(format!("--prefix={}", self.prefix.display()))
            .arg(format!("--target={target}"))
            .args(["--enable-static", "--disable-shared", "--enable-pic"])
            .args(["--disable-examples", "--disable-tools", "--disable-docs"])
            .args(["--disable-unit-tests", "--disable-vp8-encoder", "--enable-vp9"]);
        if let Some(cross) = self.target.cross_prefix() {
            configure.env("CROSS", cross);
        }
        // configure links its checks with gcc unless told
        if self.target.is_musl() {
            configure.env("CC", MUSL_GCC).env("LD", MUSL_GCC);
        }
        run(&mut configure)?;
        self.make(&dir)
    }

    fn dav1d(&self) -> Result<()> {
        let dir = self.fetch(&DAV1D)?;
        let mut setup = Command::new("meson");
        setup
            .current_dir(&dir)
            .args(["setup", "build", "--buildtype=release", "--default-library=static"])
            .arg(format!("--prefix={}", self.prefix.display()))
            .args(["--libdir=lib", "-Denable_tools=false", "-Denable_tests=false"])
            .arg(format!("-Denable_asm={}", self.asm));
        if let Some(cross) = self.target.cross_prefix() {
            let file = dir.join("cross.ini");
            std::fs::write(
                &file,
                format!(
                    "[binaries]\nc = '{cross}gcc'\nar = '{cross}ar'\nstrip = '{cross}strip'\n\
                     windres = '{cross}windres'\n\n[host_machine]\nsystem = 'windows'\n\
                     cpu_family = 'x86_64'\ncpu = 'x86_64'\nendian = 'little'\n"
                ),
            )?;
            setup.arg("--cross-file").arg(&file);
        }
        if self.target.is_musl() {
            setup.env("CC", MUSL_GCC);
        }
        run(&mut setup)?;
        run(Command::new("ninja").current_dir(&dir).args(["-C", "build", "install"]))
    }

    fn ffmpeg(&self) -> Result<()> {
        let dir = self.fetch(&FFMPEG)?;
        let mut configure = Command::new("./configure");
        configure
            .current_dir(&dir)
            .arg(format!("--prefix={}", self.prefix.display()))
            .args(["--disable-everything", "--disable-autodetect", "--disable-network"])
            // only the libraries; tgradish links them in
            .args(["--disable-doc", "--disable-debug", "--disable-programs"])
            .args(["--enable-static", "--disable-shared"])
            .args(["--enable-zlib", "--enable-libvpx", "--enable-libdav1d"])
            .args(["--pkg-config-flags=--static", "--extra-version=tgradish"])
            .arg(format!("--extra-cflags=-I{}/include", self.prefix.display()))
            .arg(format!("--extra-ldflags=-L{}/lib", self.prefix.display()))
            .env("PKG_CONFIG_PATH", self.prefix.join("lib/pkgconfig"));
        for (kind, names) in FFMPEG_COMPONENTS {
            configure.arg(format!("--enable-{kind}={}", names.join(",")));
        }
        if !self.asm {
            configure.arg("--disable-x86asm");
        }
        // threads are not left to autodetection; on Windows the native ones
        // avoid depending on winpthread's DLL
        if self.target.is_windows() {
            configure.args(["--disable-pthreads", "--enable-w32threads"]);
        } else {
            // position-independent code for Rust's PIE executables, which
            // not every distribution's compiler makes by default
            configure.args(["--enable-pthreads", "--enable-pic"]);
        }
        if let Some(cross) = self.target.cross_prefix() {
            configure
                .args(["--enable-cross-compile", "--target-os=mingw32", "--arch=x86_64"])
                .arg(format!("--cross-prefix={cross}"))
                .arg("--pkg-config=pkg-config")
                // no libgcc or winpthreads DLLs next to the executables
                .arg("--extra-ldexeflags=-static");
        }
        if self.target.is_musl() {
            configure.arg(format!("--cc={MUSL_GCC}"));
        }
        run(&mut configure)?;
        check_components(&dir)?;
        // installs the static libraries too, for `--features linked-static`
        self.make(&dir)
    }

    /// libvpx's pkg-config file asks for `-lpthread`, which rustc links as a
    /// DLL even with `-static`, so tgradish.exe would need
    /// libwinpthread-1.dll. A static copy named libpthread.a in the prefix,
    /// which the linker searches first, is picked instead.
    fn static_pthread(&self) -> Result<()> {
        let found =
            output(Command::new("x86_64-w64-mingw32-gcc").arg("-print-file-name=libwinpthread.a"))?;
        let library = PathBuf::from(found.trim());
        ensure!(library.is_absolute(), "MinGW's libwinpthread.a was not found");
        std::fs::copy(&library, self.prefix.join("lib/libpthread.a"))?;
        Ok(())
    }

    /// Collects the licenses of everything in the build, and where its
    /// sources come from, for release archives.
    fn stage_licenses(&self) -> Result<PathBuf> {
        let stage = ffmpeg_licenses(self.target);
        if stage.exists() {
            std::fs::remove_dir_all(&stage)?;
        }
        std::fs::create_dir_all(&stage)?;
        let mut sources = format!(
            "tgradish links a minimal ffmpeg {} build, made by `cargo xtask ffmpeg`.\n\
             ffmpeg is licensed under the LGPL 2.1 or later. Sources:\n\n",
            FFMPEG.version
        );
        for source in LIBRARIES {
            sources.push_str(&format!("{} {}: {}\n", source.name, source.version, source.url));
            let dir = self.source_dir(source);
            for license in source.licenses {
                std::fs::copy(dir.join(license), stage.join(format!("{}-{license}", source.name)))?;
            }
        }
        std::fs::write(stage.join("SOURCES.txt"), sources)?;
        Ok(stage)
    }
}

/// The `ffmpeg` task: [`ensure_ffmpeg`], or with `--force` [`ffmpeg_for`].
pub fn build_ffmpeg(args: &[String]) -> Result<()> {
    let mut target = Target::host();
    let mut asm = true;
    let mut force = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--target" => target = Target::parse(iter.next())?,
            "--no-asm" => asm = false,
            "--force" => force = true,
            other => bail!("unknown argument {other:?}"),
        }
    }
    if force { ffmpeg_for(target, asm) } else { ensure_ffmpeg(target, asm) }
}

/// What an ffmpeg build is made of: sources, parts, whether assembly is in,
/// and this file, which holds every flag the libraries are configured with.
/// A build from another recipe is out of date, as is one from before an
/// edit here, the same as the release workflow's cache of builds is.
pub fn recipe(asm: bool) -> String {
    let sources: Vec<String> = LIBRARIES
        .iter()
        .map(|source| format!("{} {} {}", source.name, source.version, source.sha256))
        .collect();
    // FNV-1a, which stays the same from one Rust release to the next
    let this_file =
        include_str!("ffmpeg.rs").bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
    format!("{sources:?}\n{FFMPEG_COMPONENTS:?}\nasm {asm}\nrecipe {this_file:016x}\n")
}

/// Where the static ffmpeg libraries of `target` go.
pub fn ffmpeg_prefix(target: Target) -> PathBuf {
    workspace_root().join("target/ffmpeg").join(format!("prefix-{}", target.triple()))
}

/// Where the licences of the libraries of `target` are gathered, for
/// release archives.
pub fn ffmpeg_licenses(target: Target) -> PathBuf {
    workspace_root().join("target/ffmpeg").join(format!("licenses-{}", target.triple()))
}

/// Builds the static ffmpeg libraries of `target` unless they are there
/// from `recipe(asm)` already, with their licences, as when the release
/// workflow restored them from its cache.
pub fn ensure_ffmpeg(target: Target, asm: bool) -> Result<()> {
    let prefix = ffmpeg_prefix(target);
    let built = std::fs::read_to_string(prefix.join("recipe.txt")).ok();
    let licenses = ffmpeg_licenses(target);
    if built.as_deref() != Some(recipe(asm).as_str()) || !licenses.is_dir() {
        eprintln!("building ffmpeg for {}", target.name());
        return ffmpeg_for(target, asm);
    }
    eprintln!("ffmpeg for {} is up to date; --force builds it again", target.name());
    carry_licenses(&licenses)?;
    println!("{}\n{}", prefix.display(), licenses.display());
    Ok(())
}

/// Builds the static ffmpeg libraries of `target` into its prefix, and
/// keeps `licenses/ffmpeg` up to date with what is in them.
pub fn ffmpeg_for(target: Target, asm: bool) -> Result<()> {
    let build = Build {
        target,
        asm,
        jobs: std::thread::available_parallelism().map_or(4, |n| n.get()).to_string(),
        sources: workspace_root().join("target/ffmpeg/sources"),
        prefix: ffmpeg_prefix(target),
    };
    std::fs::create_dir_all(&build.sources)?;
    if build.prefix.exists() {
        std::fs::remove_dir_all(&build.prefix)?;
    }

    build.zlib()?;
    build.libvpx()?;
    build.dav1d()?;
    build.ffmpeg()?;
    if target.is_windows() {
        build.static_pthread()?;
    }
    let licenses = build.stage_licenses()?;
    carry_licenses(&licenses)?;
    forget_linked_ffmpeg(target)?;
    std::fs::write(build.prefix.join("recipe.txt"), recipe(asm))?;
    println!("{}\n{}", build.prefix.display(), licenses.display());
    Ok(())
}

/// Copies the gathered licences into `licenses/ffmpeg`, which binaries
/// carry (see crates/core/build.rs).
fn carry_licenses(licenses: &Path) -> Result<()> {
    let carried = workspace_root().join("licenses/ffmpeg");
    std::fs::create_dir_all(&carried)?;
    for entry in std::fs::read_dir(licenses)? {
        let entry = entry?;
        std::fs::copy(entry.path(), carried.join(entry.file_name()))?;
    }
    Ok(())
}

/// Makes cargo build ffmpeg-sys-next again. It bundles the static
/// libraries into its rlib, and nothing tells cargo they changed, so builds
/// (and CI caches of them) would keep linking the ffmpeg built before.
fn forget_linked_ffmpeg(target: Target) -> Result<()> {
    let root = workspace_root().join("target");
    let mut dirs = vec![root.join(target.triple())];
    // builds without --target, for the machine itself
    if target == Target::host() {
        dirs.push(root.clone());
    }
    for dir in dirs {
        for profile in ["debug", "release"] {
            let fingerprints = dir.join(profile).join(".fingerprint");
            let Ok(entries) = std::fs::read_dir(&fingerprints) else { continue };
            for entry in entries {
                let entry = entry?;
                if entry.file_name().to_string_lossy().starts_with("ffmpeg-sys-next-") {
                    std::fs::remove_dir_all(entry.path())?;
                }
            }
        }
    }
    Ok(())
}

/// Packs the exact sources of the libraries in the ffmpeg build, for
/// publishing next to binaries as the LGPL asks.
pub fn package_sources() -> Result<()> {
    let out = workspace_root().join("target/ffmpeg");
    let downloads = out.join("sources");

    let name = format!("ffmpeg-{}-sources", FFMPEG.version);
    let stage = out.join(&name);
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    std::fs::create_dir_all(&stage)?;
    let mut readme = String::from(
        "Sources of the ffmpeg build that tgradish releases include or link\n\
         statically. Build it with `cargo xtask ffmpeg` from the tgradish\n\
         repository at the same version; xtask/src/ffmpeg.rs has every configure\n\
         flag. To relink tgradish against a modified ffmpeg, build that\n\
         with `cargo xtask ffmpeg`, then tgradish with `cargo build --release\n\
         --features linked-static`, PKG_CONFIG_PATH pointing at its lib/pkgconfig.\n\n",
    );
    for source in LIBRARIES {
        let archive = download(source, &downloads)?;
        let file_name = archive.file_name().unwrap();
        std::fs::copy(&archive, stage.join(file_name))?;
        readme.push_str(&format!(
            "{}: {} {}, sha256 {}\n",
            file_name.to_string_lossy(),
            source.name,
            source.version,
            source.sha256
        ));
    }
    std::fs::write(stage.join("README.txt"), readme)?;
    let archive = out.join(format!("{name}.tar"));
    run(Command::new("tar").arg("-cf").arg(&archive).arg("-C").arg(&out).arg(&name))?;
    println!("{}", archive.display());
    Ok(())
}
