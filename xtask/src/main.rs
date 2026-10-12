//! Development tasks, run with `cargo xtask <task>`.
//!
//! `cargo xtask ffmpeg [--target TARGET] [--no-asm]` builds the minimal
//! static ffmpeg libraries that release builds link in: only what tgradish
//! needs to decode common inputs and encode VP9 WebM. Runs on Linux, for the
//! machine's own architecture; Windows builds are cross-compiled with
//! mingw-w64. Needs a C toolchain, make, pkg-config, nasm (x86-64, unless
//! --no-asm), meson, ninja, curl and tar. Targets are `linux-x86_64`,
//! `linux-aarch64` and `windows-x86_64`.
//!
//! The static libraries end up in `target/ffmpeg/prefix-<triple>`, and the
//! licenses of everything in them in `target/ffmpeg/licenses-<triple>`.
//! ffmpeg is configured without GPL parts, so it is LGPL 2.1 or later.
//! Point `PKG_CONFIG_PATH` at the prefix's `lib/pkgconfig` to build
//! tgradish with `--features linked-static`.
//!
//! `cargo xtask package --target TARGET [--system-ffmpeg] [--max-glibc
//! 2.28]` packs a built tgradish into a release archive. With
//! `--system-ffmpeg` it is a build without ffmpeg, which uses the system's.
//! Linux builds with ffmpeg built in are also packed as a .deb and an .rpm,
//! with nfpm, which it downloads.
//!
//! `cargo xtask local-release [--targets linux-x86_64,windows-x86_64]
//! [--no-asm]` builds and packs every release variant for manual tests,
//! like the release workflow: with ffmpeg built in, and for Linux also
//! without. ffmpeg is built first where its libraries are missing or were
//! built from another recipe. Windows needs mingw-w64. The archives and
//! binaries end up in `target/release-artifacts`.
//!
//! `cargo xtask licenses [--check]` writes the licenses of the Rust crates
//! in tgradish to `licenses/THIRD-PARTY-CRATES.txt`, which binaries carry;
//! `--check` fails if it is out of date, as after dependencies change. Needs
//! cargo-about 0.9.2, which CI uses too: other versions word the list
//! differently. cargo-about skips files a `.gitignore` above the crates'
//! sources ignores, such as one of a home directory kept in git: then set
//! `CARGO_HOME` to a directory outside it. `cargo xtask ffmpeg` keeps
//! `licenses/ffmpeg` up to date the same way.
//!
//! `cargo xtask icons [--check]` makes the icons for menus and the Windows
//! executable from the window's own, `crates/gui/assets/icon.png`;
//! `--check` fails if they are out of date.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

struct Source {
    name: &'static str,
    version: &'static str,
    url: &'static str,
    sha256: &'static str,
    /// License files to ship, relative to the source directory.
    licenses: &'static [&'static str],
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
const FFMPEG: Source = Source {
    name: "ffmpeg",
    version: "9.0.2",
    url: "https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz",
    sha256: "8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e",
    licenses: &["COPYING.LGPLv2.1", "LICENSE.md"],
};

/// nfpm, which writes the .deb and .rpm packages, for the machine that runs
/// it; hashes from the release's checksums.txt.
const NFPM: [(&str, Source); 2] = [
    (
        "x86_64",
        Source {
            name: "nfpm",
            version: "2.47.0",
            url: "https://github.com/goreleaser/nfpm/releases/download/v2.47.0/nfpm_2.47.0_Linux_x86_64.tar.gz",
            sha256: "0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783",
            licenses: &[],
        },
    ),
    (
        "aarch64",
        Source {
            name: "nfpm",
            version: "2.47.0",
            url: "https://github.com/goreleaser/nfpm/releases/download/v2.47.0/nfpm_2.47.0_Linux_arm64.tar.gz",
            sha256: "1c0f5f2999b9a974bfb04fdb0cc3306096de530ac5dbb25d739cc5f5219c919c",
            licenses: &[],
        },
    ),
];

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

#[derive(Clone, Copy, PartialEq)]
enum Target {
    LinuxX64,
    LinuxArm64,
    WindowsX64,
}

impl Target {
    const ALL: [Target; 3] = [Target::LinuxX64, Target::LinuxArm64, Target::WindowsX64];

    fn name(self) -> &'static str {
        match self {
            Target::LinuxX64 => "linux-x86_64",
            Target::LinuxArm64 => "linux-aarch64",
            Target::WindowsX64 => "windows-x86_64",
        }
    }

    fn parse(name: Option<&String>) -> Result<Target> {
        let names: Vec<_> = Target::ALL.iter().map(|target| target.name()).collect();
        Target::ALL
            .into_iter()
            .find(|target| Some(target.name()) == name.map(String::as_str))
            .with_context(|| {
                format!("unknown target {name:?}, expected one of {}", names.join(", "))
            })
    }

    /// Linux on the machine's own architecture.
    fn host() -> Target {
        match std::env::consts::ARCH {
            "aarch64" => Target::LinuxArm64,
            _ => Target::LinuxX64,
        }
    }

    fn triple(self) -> &'static str {
        match self {
            Target::LinuxX64 => "x86_64-unknown-linux-gnu",
            Target::LinuxArm64 => "aarch64-unknown-linux-gnu",
            Target::WindowsX64 => "x86_64-pc-windows-gnu",
        }
    }

    fn is_windows(self) -> bool {
        self == Target::WindowsX64
    }

    fn cross_prefix(self) -> Option<&'static str> {
        self.is_windows().then_some("x86_64-w64-mingw32-")
    }

    fn exe(self, name: &str) -> String {
        if self.is_windows() { format!("{name}.exe") } else { name.to_string() }
    }
}

struct Build {
    target: Target,
    asm: bool,
    jobs: String,
    /// Downloads and extracted sources.
    sources: PathBuf,
    /// Install prefix for the libraries.
    prefix: PathBuf,
    out: PathBuf,
}

fn run(cmd: &mut Command) -> Result<()> {
    eprintln!("$ {cmd:?}");
    let status = cmd.status().with_context(|| format!("could not run {cmd:?}"))?;
    ensure!(status.success(), "{cmd:?} failed with {status}");
    Ok(())
}

fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().with_context(|| format!("could not run {cmd:?}"))?;
    ensure!(out.status.success(), "{cmd:?} failed with {}", out.status);
    Ok(String::from_utf8(out.stdout)?)
}

/// Downloads the archive of `source` into `dir`, unless it is there, and
/// verifies it.
fn download(source: &Source, dir: &Path) -> Result<PathBuf> {
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
    /// Downloads and verifies the archive of `source`.
    fn fetch_archive(&self, source: &Source) -> Result<PathBuf> {
        download(source, &self.sources)
    }

    /// Downloads, verifies and extracts `source`. Returns the source directory.
    fn fetch(&self, source: &Source) -> Result<PathBuf> {
        let archive = self.fetch_archive(source)?;

        // fresh copy per target, builds happen in the source tree
        let dir = self.sources.join(format!(
            "{}-{}-{}",
            source.name,
            source.version,
            self.target.triple()
        ));
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

    fn zlib(&self) -> Result<PathBuf> {
        let dir = self.fetch(&ZLIB)?;
        match self.target.cross_prefix() {
            None => {
                run(Command::new("./configure")
                    .current_dir(&dir)
                    .arg("--static")
                    .arg(format!("--prefix={}", self.prefix.display()))
                    .env("CFLAGS", "-O2 -fPIC"))?;
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
        Ok(dir)
    }

    fn libvpx(&self) -> Result<PathBuf> {
        let dir = self.fetch(&LIBVPX)?;
        let target = match (self.target, self.asm) {
            (_, false) => "generic-gnu",
            (Target::LinuxX64, true) => "x86_64-linux-gcc",
            (Target::LinuxArm64, true) => "arm64-linux-gcc",
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
        run(&mut configure)?;
        self.make(&dir)?;
        Ok(dir)
    }

    fn dav1d(&self) -> Result<PathBuf> {
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
        run(&mut setup)?;
        run(Command::new("ninja").current_dir(&dir).args(["-C", "build", "install"]))?;
        Ok(dir)
    }

    fn ffmpeg(&self) -> Result<PathBuf> {
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
        run(&mut configure)?;
        check_components(&dir)?;
        // installs the static libraries too, for `--features linked-static`
        self.make(&dir)?;
        Ok(dir)
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
    fn stage_licenses(&self, dirs: &[(&Source, PathBuf)]) -> Result<PathBuf> {
        let stage = self.out.join(format!("licenses-{}", self.target.triple()));
        if stage.exists() {
            std::fs::remove_dir_all(&stage)?;
        }
        std::fs::create_dir_all(&stage)?;
        let mut sources = format!(
            "tgradish links a minimal ffmpeg {} build, made by `cargo xtask ffmpeg`.\n\
             ffmpeg is licensed under the LGPL 2.1 or later. Sources:\n\n",
            FFMPEG.version
        );
        for (source, dir) in dirs {
            sources.push_str(&format!("{} {}: {}\n", source.name, source.version, source.url));
            for license in source.licenses {
                std::fs::copy(dir.join(license), stage.join(format!("{}-{license}", source.name)))?;
            }
        }
        std::fs::write(stage.join("SOURCES.txt"), sources)?;
        Ok(stage)
    }
}

fn build_ffmpeg(args: &[String]) -> Result<()> {
    let mut target = Target::host();
    let mut asm = true;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--target" => target = Target::parse(iter.next())?,
            "--no-asm" => asm = false,
            other => bail!("unknown argument {other:?}"),
        }
    }
    ffmpeg_for(target, asm)
}

/// What an ffmpeg build is made of: sources, parts and whether assembly is
/// in, so a build from another recipe is known to be out of date.
fn recipe(asm: bool) -> String {
    let sources: Vec<String> = [&ZLIB, &LIBVPX, &DAV1D, &FFMPEG]
        .iter()
        .map(|source| format!("{} {} {}", source.name, source.version, source.sha256))
        .collect();
    format!("{sources:?}\n{FFMPEG_COMPONENTS:?}\nasm {asm}\n")
}

/// Where the static ffmpeg libraries of `target` go.
fn ffmpeg_prefix(target: Target) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    root.join("target/ffmpeg").join(format!("prefix-{}", target.triple()))
}

fn ffmpeg_for(target: Target, asm: bool) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("target/ffmpeg");
    let build = Build {
        target,
        asm,
        jobs: std::thread::available_parallelism().map_or(4, |n| n.get()).to_string(),
        sources: root.join("sources"),
        prefix: root.join(format!("prefix-{}", target.triple())),
        out: root.clone(),
    };
    std::fs::create_dir_all(&build.sources)?;
    if build.prefix.exists() {
        std::fs::remove_dir_all(&build.prefix)?;
    }

    let dirs = vec![
        (&ZLIB, build.zlib()?),
        (&LIBVPX, build.libvpx()?),
        (&DAV1D, build.dav1d()?),
        (&FFMPEG, build.ffmpeg()?),
    ];
    if target.is_windows() {
        build.static_pthread()?;
    }
    let licenses = build.stage_licenses(&dirs)?;
    // what binaries carry, see crates/core/build.rs
    let carried = root.parent().unwrap().parent().unwrap().join("licenses/ffmpeg");
    std::fs::create_dir_all(&carried)?;
    for entry in std::fs::read_dir(&licenses)? {
        let entry = entry?;
        std::fs::copy(entry.path(), carried.join(entry.file_name()))?;
    }
    forget_linked_ffmpeg(target)?;
    std::fs::write(build.prefix.join("recipe.txt"), recipe(asm))?;
    println!("{}\n{}", build.prefix.display(), licenses.display());
    Ok(())
}

/// Builds every release variant of `targets` and packs them into
/// `target/release-artifacts`, building ffmpeg first where its libraries
/// are missing or from another recipe.
fn local_release(args: &[String]) -> Result<()> {
    let mut targets = vec![Target::host(), Target::WindowsX64];
    let mut asm = true;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--targets" => {
                let names = iter
                    .next()
                    .context("--targets needs a list, like linux-x86_64,windows-x86_64")?;
                targets = names
                    .split(',')
                    .map(|name| Target::parse(Some(&name.trim().to_owned())))
                    .collect::<Result<_>>()?;
            }
            "--no-asm" => asm = false,
            other => bail!("unknown argument {other:?}"),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    for target in targets {
        let prefix = ffmpeg_prefix(target);
        let built = std::fs::read_to_string(prefix.join("recipe.txt")).ok();
        if built.as_deref() != Some(recipe(asm).as_str()) {
            eprintln!("building ffmpeg for {}", target.name());
            ffmpeg_for(target, asm)?;
        }
        let triple = target.triple();
        let mut linked = Command::new(&cargo);
        linked
            .current_dir(root)
            .args(["build", "--release", "--locked", "-p", "tgradish"])
            .args(["--features", "linked-static", "--target", triple])
            .env("PKG_CONFIG_PATH", prefix.join("lib/pkgconfig"))
            .env("PKG_CONFIG_ALL_STATIC", "1");
        if target.is_windows() {
            let stubs = root.join("xtask/mingw-stubs");
            linked
                .env("PKG_CONFIG_ALLOW_CROSS", "1")
                .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER", "x86_64-w64-mingw32-gcc")
                .env("CC_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-gcc")
                .env("AR_x86_64_pc_windows_gnu", "x86_64-w64-mingw32-ar")
                // the stubs come last, so they only fill in headers MinGW lacks
                .env(
                    "BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_gnu",
                    format!("-I/usr/x86_64-w64-mingw32/include -I{}", stubs.display()),
                )
                .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS", "-C link-arg=-static");
        }
        run(&mut linked)?;
        package_release(&["--target".into(), target.name().into()])?;
        // the build without ffmpeg goes where the linked one was, so after
        // that one is packed
        if !target.is_windows() {
            run(Command::new(&cargo).current_dir(root).args([
                "build",
                "--release",
                "--locked",
                "-p",
                "tgradish",
                "--target",
                triple,
            ]))?;
            let system = ["--target".into(), target.name().into(), "--system-ffmpeg".into()];
            package_release(&system)?;
        }
    }
    let out = root.join("target/release-artifacts");
    println!("\nin {}:", out.display());
    let mut entries: Vec<_> = std::fs::read_dir(&out)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        for name in ["tgradish", "tgradish.exe"] {
            let binary = entry.path().join(name);
            if let Ok(metadata) = std::fs::metadata(&binary) {
                println!("  {} ({:.1} MB)", binary.display(), metadata.len() as f64 / 1e6);
            }
        }
    }
    Ok(())
}

/// Makes cargo build ffmpeg-sys-next again. It bundles the static
/// libraries into its rlib, and nothing tells cargo they changed, so builds
/// (and CI caches of them) would keep linking the ffmpeg built before.
fn forget_linked_ffmpeg(target: Target) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("target");
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
fn package_sources() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let out = root.join("target/ffmpeg");
    let build = Build {
        target: Target::host(),
        asm: true,
        jobs: "1".into(),
        sources: out.join("sources"),
        prefix: out.join("unused"),
        out: out.clone(),
    };
    std::fs::create_dir_all(&build.sources)?;

    let name = format!("ffmpeg-{}-sources", FFMPEG.version);
    let stage = out.join(&name);
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    std::fs::create_dir_all(&stage)?;
    let mut readme = String::from(
        "Sources of the ffmpeg build that tgradish releases include or link\n\
         statically. Build it with `cargo xtask ffmpeg` from the tgradish\n\
         repository at the same version; xtask/src/main.rs has every configure\n\
         flag. To relink tgradish against a modified ffmpeg, build that\n\
         with `cargo xtask ffmpeg`, then tgradish with `cargo build --release\n\
         --features linked-static`, PKG_CONFIG_PATH pointing at its lib/pkgconfig.\n\n",
    );
    for source in [&ZLIB, &LIBVPX, &DAV1D, &FFMPEG] {
        let archive = build.fetch_archive(source)?;
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

/// Version of the tgradish crates, from the workspace manifest.
fn workspace_version(root: &Path) -> Result<String> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml"))?;
    let package = manifest.split("[workspace.package]").nth(1).context("no [workspace.package]")?;
    let line = package
        .lines()
        .find(|line| line.trim_start().starts_with("version"))
        .context("no workspace version")?;
    Ok(line.split('"').nth(1).context("malformed version")?.to_string())
}

fn sha256_file(path: &Path) -> Result<()> {
    let dir = path.parent().unwrap();
    let name = path.file_name().unwrap();
    let sum = output(Command::new("sha256sum").current_dir(dir).arg(name))?;
    let mut sum_path = path.as_os_str().to_owned();
    sum_path.push(".sha256");
    std::fs::write(sum_path, sum)?;
    Ok(())
}

/// DLLs that every Windows has. Anything else, like MinGW's winpthread or
/// libgcc, would have to be shipped next to the executable.
const SYSTEM_DLLS: &[&str] = &[
    "advapi32.dll",
    "bcrypt.dll",
    "crypt32.dll",
    "gdi32.dll",
    "kernel32.dll",
    "msvcrt.dll",
    "ntdll.dll",
    "ole32.dll",
    "oleaut32.dll",
    "secur32.dll",
    "shell32.dll",
    "user32.dll",
    "userenv.dll",
    "ws2_32.dll",
    "psapi.dll",
    "dbghelp.dll",
    "shlwapi.dll",
    "mfplat.dll",
    "mfuuid.dll",
    "strmiids.dll",
    "api-ms-win-core-synch-l1-2-0.dll",
    "uxtheme.dll",
    "comctl32.dll",
    "comdlg32.dll",
    "winmm.dll",
    "iphlpapi.dll",
    "ucrtbase.dll",
    "bcryptprimitives.dll",
    "combase.dll",
    // the window's
    "dwmapi.dll",
    "imm32.dll",
    "opengl32.dll",
];

/// Fails if a Windows executable imports a DLL that is not part of Windows.
fn check_dll_imports(exe: &Path) -> Result<()> {
    let dump = output(Command::new("x86_64-w64-mingw32-objdump").arg("-p").arg(exe))?;
    let imports: Vec<String> = dump
        .lines()
        .filter_map(|line| line.trim().strip_prefix("DLL Name:"))
        .map(|name| name.trim().to_lowercase())
        .collect();
    let foreign: Vec<_> = imports
        .iter()
        .filter(|dll| !SYSTEM_DLLS.contains(&dll.as_str()) && !dll.starts_with("api-ms-win-"))
        .collect();
    ensure!(foreign.is_empty(), "{} needs DLLs Windows does not have: {foreign:?}", exe.display());
    println!("DLL imports: {}", imports.join(", "));
    Ok(())
}

/// Packs a release archive: the tgradish binary built with
/// `--features linked-static --target <triple>`, and the licenses of the
/// ffmpeg build linked into it.
fn package_release(args: &[String]) -> Result<()> {
    let mut target = None;
    let mut system_ffmpeg = false;
    let mut max_glibc = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--target" => target = Some(Target::parse(iter.next())?),
            "--system-ffmpeg" => system_ffmpeg = true,
            "--max-glibc" => {
                max_glibc =
                    Some(glibc_version(iter.next().context("--max-glibc needs a version")?)?)
            }
            other => bail!("unknown argument {other:?}"),
        }
    }
    let target = target.context(
        "usage: cargo xtask package --target TARGET [--system-ffmpeg] [--max-glibc 2.28]",
    )?;
    ensure!(!(system_ffmpeg && target.is_windows()), "Windows builds always link ffmpeg in");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let version = workspace_version(root)?;
    let triple = target.triple();
    let binary = root.join("target").join(triple).join("release").join(target.exe("tgradish"));
    ensure!(binary.is_file(), "{} is missing, build it first", binary.display());
    let licenses = root.join(format!("target/ffmpeg/licenses-{triple}"));
    ensure!(
        system_ffmpeg || licenses.is_dir(),
        "{} is missing, run cargo xtask ffmpeg",
        licenses.display()
    );

    let out = root.join("target/release-artifacts");
    let name = match system_ffmpeg {
        true => format!("tgradish-{version}-{triple}-system-ffmpeg"),
        false => format!("tgradish-{version}-{triple}"),
    };
    let stage = out.join(&name);
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    std::fs::create_dir_all(&stage)?;
    std::fs::copy(&binary, stage.join(target.exe("tgradish")))?;
    std::fs::copy(root.join("README.md"), stage.join("README.md"))?;
    std::fs::copy(root.join("LICENSE.txt"), stage.join("LICENSE.txt"))?;
    if !target.is_windows() {
        // for menus: install as share/applications/tgradish.desktop, and
        // icons/ as share/icons
        let desktop = "tgradish.desktop";
        std::fs::copy(root.join("crates/cli/assets").join(desktop), stage.join(desktop))?;
        for size in ICON_SIZES {
            let icon = format!("icons/hicolor/{size}x{size}/apps/tgradish.png");
            std::fs::create_dir_all(stage.join(&icon).parent().unwrap())?;
            std::fs::copy(root.join("crates/cli/assets").join(&icon), stage.join(&icon))?;
        }
    }
    // libwebp, which every build has for image stickers
    std::fs::create_dir_all(stage.join("licenses"))?;
    for file in ["COPYING", "PATENTS"] {
        let to = stage.join("licenses").join(format!("libwebp-{file}"));
        std::fs::copy(root.join("licenses/libwebp").join(file), to)?;
    }
    let third_party = if system_ffmpeg {
        "tgradish is MIT licensed, see LICENSE.txt.\n\n\
         This build has no ffmpeg in it: it runs the system's ffmpeg and ffprobe\n\
         (6.0 or newer, with libvpx for VP9) from PATH.\n"
            .to_owned()
    } else {
        for entry in std::fs::read_dir(&licenses)? {
            let entry = entry?;
            std::fs::copy(entry.path(), stage.join("licenses").join(entry.file_name()))?;
        }
        format!(
            "tgradish is MIT licensed, see LICENSE.txt.\n\n\
             It includes ffmpeg {ffmpeg} (LGPL 2.1 or later) built with zlib, libvpx and\n\
             dav1d, linked statically. Their licences are in licenses/. The exact\n\
             sources are in ffmpeg-{ffmpeg}-sources.tar, published with this release,\n\
             with how to rebuild tgradish against a modified ffmpeg.\n",
            ffmpeg = FFMPEG.version,
        )
    };
    let libwebp = "\nIt includes libwebp (BSD 3-Clause), for image stickers: its licence and\n\
                   patent grant are licenses/libwebp-COPYING and licenses/libwebp-PATENTS.\n";
    std::fs::write(
        stage.join("THIRD-PARTY.txt"),
        third_party
            + libwebp
            + "\nThe Rust crates in it and their licences are in THIRD-PARTY-CRATES.txt.\n",
    )?;
    // the crates of every build, as the binary carries them
    std::fs::copy(
        root.join("licenses/THIRD-PARTY-CRATES.txt"),
        stage.join("THIRD-PARTY-CRATES.txt"),
    )?;

    if target.is_windows() {
        check_dll_imports(&binary)?;
    }
    if let Some(max) = max_glibc {
        check_glibc(&binary, max)?;
    }

    let archive = if target.is_windows() {
        let archive = out.join(format!("{name}.zip"));
        if archive.exists() {
            std::fs::remove_file(&archive)?;
        }
        run(Command::new("zip").current_dir(&out).arg("-qr").arg(&archive).arg(&name))?;
        archive
    } else {
        let archive = out.join(format!("{name}.tar.gz"));
        run(Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&out).arg(&name))?;
        archive
    };
    sha256_file(&archive)?;
    println!("{}", archive.display());

    // with ffmpeg built in, as the system's may be too old or lack codecs
    if !target.is_windows() && !system_ffmpeg {
        for package in linux_packages(root, target, &version, &stage)? {
            sha256_file(&package)?;
            println!("{}", package.display());
        }
    }
    Ok(())
}

/// What the window loads when it opens, as Debian packages and as the
/// libraries an .rpm can ask for by name, which Fedora and openSUSE both
/// resolve: the graphics libraries, and D-Bus for file dialogs.
const WINDOW_LIBRARIES: [(&str, &str); 13] = [
    ("libegl1", "libEGL.so.1"),
    ("libgl1", "libGL.so.1"),
    ("libwayland-client0", "libwayland-client.so.0"),
    ("libwayland-egl1", "libwayland-egl.so.1"),
    ("libx11-6", "libX11.so.6"),
    ("libx11-xcb1", "libX11-xcb.so.1"),
    ("libxcb1", "libxcb.so.1"),
    ("libxcursor1", "libXcursor.so.1"),
    ("libxi6", "libXi.so.6"),
    ("libxrender1", "libXrender.so.1"),
    ("libxkbcommon0", "libxkbcommon.so.0"),
    ("libxkbcommon-x11-0", "libxkbcommon-x11.so.0"),
    ("libdbus-1-3", "libdbus-1.so.3"),
];

/// Packs the release staged in `stage` as a .deb and an .rpm, with nfpm.
/// Returns their paths.
fn linux_packages(
    root: &Path,
    target: Target,
    version: &str,
    stage: &Path,
) -> Result<Vec<PathBuf>> {
    let host = std::env::consts::ARCH;
    let (_, source) = NFPM
        .iter()
        .find(|(arch, _)| *arch == host)
        .with_context(|| format!("no nfpm for {host} machines"))?;
    let tools = root.join("target/tools");
    let archive = download(source, &tools)?;
    let nfpm_dir = tools.join(format!("nfpm-{}", source.version));
    std::fs::create_dir_all(&nfpm_dir)?;
    run(Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(&nfpm_dir).arg("nfpm"))?;

    let quoted = |path: &Path| format!("{:?}", path.display().to_string());
    let mut contents = vec![
        format!(
            "  - src: {}\n    dst: /usr/bin/tgradish\n    file_info:\n      mode: 0755",
            quoted(&stage.join("tgradish"))
        ),
        format!(
            "  - src: {}\n    dst: /usr/share/applications/tgradish.desktop",
            quoted(&stage.join("tgradish.desktop"))
        ),
    ];
    for size in ICON_SIZES {
        let icon = format!("icons/hicolor/{size}x{size}/apps/tgradish.png");
        contents
            .push(format!("  - src: {}\n    dst: /usr/share/{icon}", quoted(&stage.join(&icon))));
    }
    for doc in ["README.md", "THIRD-PARTY.txt", "THIRD-PARTY-CRATES.txt"] {
        contents.push(format!(
            "  - src: {}\n    dst: /usr/share/doc/tgradish/{doc}",
            quoted(&stage.join(doc))
        ));
    }
    contents.push(format!(
        "  - src: {}\n    dst: /usr/share/licenses/tgradish/LICENSE.txt",
        quoted(&stage.join("LICENSE.txt"))
    ));
    for entry in std::fs::read_dir(stage.join("licenses"))? {
        let entry = entry?;
        contents.push(format!(
            "  - src: {}\n    dst: /usr/share/licenses/tgradish/{}",
            quoted(&entry.path()),
            entry.file_name().to_string_lossy()
        ));
    }
    contents.sort();
    let list = |names: Vec<String>| {
        names.iter().map(|name| format!("\n      - {name:?}")).collect::<String>()
    };
    let debs = std::iter::once("libc6 (>= 2.28)".to_owned())
        .chain(["libgcc-s1 | libgcc1".to_owned()])
        .chain(WINDOW_LIBRARIES.iter().map(|(deb, _)| (*deb).to_owned()))
        .collect();
    // as rpm names the libraries of 64-bit programs
    let rpms = ["libc.so.6(GLIBC_2.28)", "libgcc_s.so.1"]
        .into_iter()
        .chain(WINDOW_LIBRARIES.iter().map(|(_, library)| *library))
        .map(|library| match library.ends_with(')') {
            true => format!("{library}(64bit)"),
            false => format!("{library}()(64bit)"),
        })
        .collect();
    let arch = match target {
        Target::LinuxX64 => "amd64",
        Target::LinuxArm64 => "arm64",
        Target::WindowsX64 => bail!("Windows has no .deb or .rpm"),
    };
    let config = format!(
        "name: tgradish\n\
         arch: {arch}\n\
         platform: linux\n\
         version: {version:?}\n\
         release: \"1\"\n\
         section: graphics\n\
         priority: optional\n\
         maintainer: \"sliva0 <sliva0mk@gmail.com>\"\n\
         description: |-\n  \
           Telegram stickers and emoji from videos, pictures and pixel art\n  \
           Makes video (WebM) and image (WebP) stickers and custom emoji from any\n  \
           video or picture, and animated (TGS) stickers from pixel art, fitted to\n  \
           Telegram's limits. It has ffmpeg built in.\n\
         homepage: \"https://github.com/sliva0/tgradish\"\n\
         license: \"MIT AND LGPL-2.1-or-later AND BSD-3-Clause AND BSD-2-Clause AND Zlib\"\n\
         contents:\n{contents}\n\
         overrides:\n  \
           deb:\n    depends:{debs}\n    recommends:\n      - xdg-desktop-portal\n  \
           rpm:\n    depends:{rpms}\n    recommends:\n      - xdg-desktop-portal\n",
        contents = contents.join("\n"),
        debs = list(debs),
        rpms = list(rpms),
    );
    let out = stage.parent().unwrap();
    let config_path = out.join(format!("nfpm-{}.yaml", target.triple()));
    std::fs::write(&config_path, config)?;
    let rpm_arch = target.triple().split('-').next().unwrap();
    let packages = [
        ("deb", out.join(format!("tgradish_{version}-1_{arch}.deb"))),
        ("rpm", out.join(format!("tgradish-{version}-1.{rpm_arch}.rpm"))),
    ];
    for (packager, path) in &packages {
        run(Command::new(nfpm_dir.join("nfpm"))
            .arg("package")
            .arg("--config")
            .arg(&config_path)
            .args(["--packager", packager, "--target"])
            .arg(path))?;
    }
    Ok(packages.into_iter().map(|(_, path)| path).collect())
}

/// `2.28` as `(2, 28)`; `2.2.5` is `(2, 2)`.
fn glibc_version(text: &str) -> Result<(u32, u32)> {
    let mut parts = text.split('.').map(str::parse);
    match (parts.next(), parts.next()) {
        (Some(Ok(major)), Some(Ok(minor))) => Ok((major, minor)),
        _ => bail!("{text:?} is not a glibc version like 2.28"),
    }
}

/// Fails if a Linux executable needs a newer glibc than `max`, so it would
/// not start on older systems.
fn check_glibc(exe: &Path, max: (u32, u32)) -> Result<()> {
    let dump = output(Command::new("objdump").arg("-T").arg(exe))?;
    let newest = dump
        .split_whitespace()
        .filter_map(|word| word.trim_matches(['(', ')']).strip_prefix("GLIBC_"))
        .filter_map(|version| glibc_version(version).ok())
        .max()
        .context("the executable uses no versioned glibc symbols")?;
    ensure!(
        newest <= max,
        "{} needs glibc {}.{}, newer than {}.{}",
        exe.display(),
        newest.0,
        newest.1,
        max.0,
        max.1
    );
    Ok(())
}

/// Writes or checks `licenses/THIRD-PARTY-CRATES.txt`: the crates of
/// every target and build, ffmpeg linked in or not.
fn crate_licenses(args: &[String]) -> Result<()> {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => bail!("usage: cargo xtask licenses [--check]"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let file = root.join("licenses/THIRD-PARTY-CRATES.txt");
    let fresh = root.join("target/THIRD-PARTY-CRATES.txt");
    run(Command::new("cargo")
        .current_dir(root)
        .args(["about", "generate", "--locked", "--fail", "-c", "xtask/about.toml"])
        .args(["-m", "crates/cli/Cargo.toml", "--features", "linked-static"])
        .arg("xtask/about.hbs")
        .arg("-o")
        .arg(&fresh))?;
    let text = sorted_sections(&std::fs::read_to_string(&fresh)?);
    // crates' own license files say who holds the copyright; without them
    // (see the module's notes) every crate gets the bare license text
    ensure!(
        text.matches("\n----").count() > 40,
        "cargo-about found almost no license files: is a .gitignore hiding CARGO_HOME?"
    );
    if check {
        let committed = std::fs::read_to_string(&file).unwrap_or_default();
        ensure!(
            committed == text,
            "{} is out of date: run cargo xtask licenses and commit it",
            file.display()
        );
        println!("{} is up to date", file.display());
    } else {
        std::fs::write(&file, text)?;
        println!("{}", file.display());
    }
    Ok(())
}

/// cargo-about's list with its sections in the order they read in, by
/// licence and then crates, and each one's crates by name: it lists those
/// of a licence in no order a reader could follow.
fn sorted_sections(text: &str) -> String {
    let rule = format!("\n{}\n", "-".repeat(80));
    let mut parts = text.split(rule.as_str());
    let intro = parts.next().unwrap_or_default().to_owned();
    let mut sections: Vec<String> = parts
        .map(|section| {
            let (head, body) = section.split_once("\n\n").unwrap_or((section, ""));
            let mut lines = head.lines();
            let title = lines.next().unwrap_or_default();
            let mut crates: Vec<&str> = lines.collect();
            crates.sort_by_key(|line| line.trim().to_lowercase());
            format!("{title}\n{}\n\n{body}", crates.join("\n"))
        })
        .collect();
    sections.sort_by_cached_key(|section| {
        let head = section.split_once("\n\n").map_or(section.as_str(), |(head, _)| head);
        head.lines().map(|line| line.trim().to_lowercase()).collect::<Vec<_>>()
    });
    std::iter::once(intro).chain(sections).collect::<Vec<_>>().join(&rule)
}

/// Sizes of the icons for menus; the Windows executable has those up to 256.
const ICON_SIZES: [usize; 10] = [16, 24, 32, 48, 64, 96, 128, 192, 256, 512];

/// Writes or checks the icons made from the window's: PNGs in
/// `crates/cli/assets/icons`, laid out as in `share/icons`, and
/// `crates/cli/assets/tgradish.ico` for the Windows executable.
fn icons(args: &[String]) -> Result<()> {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => bail!("usage: cargo xtask icons [--check]"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let (side, pixels) = read_rgba(&root.join("crates/gui/assets/icon.png"))?;
    let assets = root.join("crates/cli/assets");
    let mut files = Vec::new();
    let mut ico = Vec::new();
    for size in ICON_SIZES {
        let png = rgba_png(size, &scale_pixel_art(&pixels, side, size))?;
        if size <= 256 {
            ico.push((size, png.clone()));
        }
        files.push((assets.join(format!("icons/hicolor/{size}x{size}/apps/tgradish.png")), png));
    }
    files.push((assets.join("tgradish.ico"), ico_file(&ico)));
    for (path, bytes) in files {
        if check {
            ensure!(
                std::fs::read(&path).is_ok_and(|committed| committed == bytes),
                "{} is out of date: run cargo xtask icons and commit it",
                path.display()
            );
        } else {
            std::fs::create_dir_all(path.parent().unwrap())?;
            std::fs::write(&path, bytes)?;
            println!("{}", path.display());
        }
    }
    if check {
        println!("the icons are up to date");
    }
    Ok(())
}

/// The side and straight RGBA pixels of a square 8-bit RGBA PNG.
fn read_rgba(path: &Path) -> Result<(usize, Vec<u8>)> {
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut reader = png::Decoder::new(file).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size().context("the PNG is too big")?];
    let info = reader.next_frame(&mut pixels)?;
    ensure!(
        (info.color_type, info.bit_depth) == (png::ColorType::Rgba, png::BitDepth::Eight)
            && info.width == info.height,
        "{} should be a square 8-bit RGBA PNG",
        path.display()
    );
    pixels.truncate(info.buffer_size());
    Ok((info.width as usize, pixels))
}

/// Square pixel art, straight RGBA `from` pixels wide, made `to` pixels
/// wide: each pixel the average of the part of the picture it covers.
/// Whole multiples copy pixels; other sizes blend them only along their
/// edges.
fn scale_pixel_art(pixels: &[u8], from: usize, to: usize) -> Vec<u8> {
    // in 1/(from*to) of the side, the picture's pixel i spans
    // [i*to, (i+1)*to) and the result's pixel j [j*from, (j+1)*from):
    // which of the picture's pixels each of the result's covers, and how
    // much of them
    let spans: Vec<Vec<(usize, u64)>> = (0..to)
        .map(|j| {
            let (start, end) = (j * from, (j + 1) * from);
            (start / to..end.div_ceil(to))
                .map(|i| (i, (end.min((i + 1) * to) - start.max(i * to)) as u64))
                .collect()
        })
        .collect();
    let round = |sum: u64, count: u64| ((2 * sum + count) / (2 * count)) as u8;
    let mut out = Vec::with_capacity(to * to * 4);
    for rows in &spans {
        for columns in &spans {
            // colours weighted by the area they cover and their opacity
            let mut sums = [0u64; 4];
            for &(row, height) in rows {
                for &(column, width) in columns {
                    let pixel = &pixels[(row * from + column) * 4..][..4];
                    let weight = height * width * u64::from(pixel[3]);
                    for (sum, &channel) in sums.iter_mut().zip(&pixel[..3]) {
                        *sum += weight * u64::from(channel);
                    }
                    sums[3] += weight;
                }
            }
            match sums[3] {
                0 => out.extend([0; 4]),
                opacity => {
                    out.extend(sums[..3].iter().map(|&sum| round(sum, opacity)));
                    out.push(round(opacity, (from * from) as u64));
                }
            }
        }
    }
    out
}

/// Straight RGBA pixels, `side` square, as a PNG.
fn rgba_png(side: usize, pixels: &[u8]) -> Result<Vec<u8>> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, side as u32, side as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(pixels)?;
    writer.finish()?;
    Ok(png)
}

/// A Windows icon holding PNGs, each with its side.
fn ico_file(images: &[(usize, Vec<u8>)]) -> Vec<u8> {
    // reserved, an icon rather than a cursor, how many images
    let mut ico: Vec<u8> =
        [0u16, 1, images.len() as u16].iter().flat_map(|n| n.to_le_bytes()).collect();
    let mut offset = 6 + 16 * images.len();
    for (side, png) in images {
        // 0 stands for 256
        let side = (side % 256) as u8;
        // no palette, a reserved byte, one plane, 32 bits per pixel
        ico.extend([side, side, 0, 0, 1, 0, 32, 0]);
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in images {
        ico.extend(png);
    }
    ico
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ffmpeg") => build_ffmpeg(&args[1..]),
        Some("ffmpeg-sources") => package_sources(),
        Some("package") => package_release(&args[1..]),
        Some("licenses") => crate_licenses(&args[1..]),
        Some("icons") => icons(&args[1..]),
        Some("local-release") => local_release(&args[1..]),
        _ => bail!(
            "usage: cargo xtask ffmpeg [--target TARGET] [--no-asm]\n       \
             cargo xtask ffmpeg-sources\n       \
             cargo xtask package --target TARGET [--system-ffmpeg] [--max-glibc 2.28]\n       \
             cargo xtask licenses [--check]\n       \
             cargo xtask icons [--check]\n       \
             cargo xtask local-release [--targets TARGET,...] [--no-asm]\n\
             targets: linux-x86_64, linux-aarch64, windows-x86_64"
        ),
    }
}
