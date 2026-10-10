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
//! Needs cargo-about, which lists the licenses of the crates built in.

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

impl Build {
    /// Downloads and verifies the archive of `source`.
    fn fetch_archive(&self, source: &Source) -> Result<PathBuf> {
        let file_name = source.url.rsplit('/').next().unwrap();
        let archive = self.sources.join(format!("{}-{file_name}", source.name));
        if !archive.exists() {
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
    println!("{}\n{}", build.prefix.display(), licenses.display());
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
         flag. To relink tgradish against a modified ffmpeg, build it as\n\
         described in docs/ffmpeg.md.\n\n",
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
        // for menus: install as share/applications/tgradish.desktop
        let desktop = "tgradish.desktop";
        std::fs::copy(root.join("crates/cli/assets").join(desktop), stage.join(desktop))?;
    }
    let third_party = if system_ffmpeg {
        "tgradish is MIT licensed, see LICENSE.txt.\n\n\
         This build has no ffmpeg in it: it runs the system's ffmpeg and ffprobe\n\
         (6.0 or newer, with libvpx for VP9) from PATH.\n"
            .to_owned()
    } else {
        std::fs::create_dir_all(stage.join("licenses"))?;
        for entry in std::fs::read_dir(&licenses)? {
            let entry = entry?;
            std::fs::copy(entry.path(), stage.join("licenses").join(entry.file_name()))?;
        }
        format!(
            "tgradish is MIT licensed, see LICENSE.txt.\n\n\
             It includes ffmpeg {ffmpeg} (LGPL 2.1 or later) built with zlib, libvpx and\n\
             dav1d, linked statically. Their licenses are in licenses/. The exact\n\
             sources are in ffmpeg-{ffmpeg}-sources.tar, published with this release;\n\
             docs/ffmpeg.md in the tgradish repository explains how to rebuild\n\
             tgradish against a modified ffmpeg.\n",
            ffmpeg = FFMPEG.version,
        )
    };
    std::fs::write(
        stage.join("THIRD-PARTY.txt"),
        third_party + "\nThe Rust crates in it and their licenses are in THIRD-PARTY-CRATES.txt.\n",
    )?;
    // the crates differ by target and by whether ffmpeg is linked in
    let mut about = Command::new("cargo");
    about
        .current_dir(root)
        .args(["about", "generate", "--locked", "--fail", "-c", "xtask/about.toml"])
        .args(["-m", "crates/cli/Cargo.toml", "--target", triple]);
    if !system_ffmpeg {
        about.args(["--features", "linked-static"]);
    }
    run(about.arg("xtask/about.hbs").arg("-o").arg(stage.join("THIRD-PARTY-CRATES.txt")))?;

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
    Ok(())
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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ffmpeg") => build_ffmpeg(&args[1..]),
        Some("ffmpeg-sources") => package_sources(),
        Some("package") => package_release(&args[1..]),
        _ => bail!(
            "usage: cargo xtask ffmpeg [--target TARGET] [--no-asm]\n       \
             cargo xtask ffmpeg-sources\n       \
             cargo xtask package --target TARGET [--system-ffmpeg] [--max-glibc 2.28]\n\
             targets: linux-x86_64, linux-aarch64, windows-x86_64"
        ),
    }
}
