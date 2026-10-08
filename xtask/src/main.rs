//! Development tasks, run with `cargo xtask <task>`.
//!
//! `cargo xtask ffmpeg [--target linux|windows] [--no-asm]` builds the minimal
//! static ffmpeg and ffprobe that release archives ship with: only what
//! tgradish needs to decode common inputs and encode VP9 WebM. Runs on Linux;
//! Windows builds are cross-compiled with mingw-w64. Needs a C toolchain,
//! make, pkg-config, nasm (unless --no-asm), meson, ninja, curl and tar.
//!
//! The result is `target/ffmpeg/ffmpeg-<version>-<target>.tar.gz` with the
//! two executables and the licenses of everything linked into them. ffmpeg
//! is configured without GPL parts, so it is LGPL 2.1 or later.
//!
//! The static libraries end up in `target/ffmpeg/prefix-<target>`; point
//! `PKG_CONFIG_PATH` at its `lib/pkgconfig` to build tgradish with
//! `--features linked-static`.

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
    Linux,
    Windows,
}

impl Target {
    fn triple(self) -> &'static str {
        match self {
            Target::Linux => "x86_64-unknown-linux-gnu",
            Target::Windows => "x86_64-pc-windows-gnu",
        }
    }

    fn cross_prefix(self) -> Option<&'static str> {
        (self == Target::Windows).then_some("x86_64-w64-mingw32-")
    }

    fn exe(self, name: &str) -> String {
        match self {
            Target::Linux => name.to_string(),
            Target::Windows => format!("{name}.exe"),
        }
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
                .args(["-sSfL", "--retry", "3", "--retry-all-errors", "-o"])
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
            (Target::Linux, true) => "x86_64-linux-gcc",
            (Target::Windows, true) => "x86_64-win64-gcc",
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
            .args(["--disable-doc", "--disable-debug", "--disable-ffplay"])
            .args(["--enable-ffmpeg", "--enable-ffprobe", "--enable-static", "--disable-shared"])
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
        match self.target {
            Target::Linux => configure.arg("--enable-pthreads"),
            Target::Windows => configure.args(["--disable-pthreads", "--enable-w32threads"]),
        };
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

    fn package(&self, dirs: &[(&Source, PathBuf)]) -> Result<PathBuf> {
        let name = format!("ffmpeg-{}-{}", FFMPEG.version, self.target.triple());
        let stage = self.out.join(&name);
        if stage.exists() {
            std::fs::remove_dir_all(&stage)?;
        }
        std::fs::create_dir_all(stage.join("licenses"))?;

        let ffmpeg_dir = &dirs.iter().find(|(s, _)| s.name == "ffmpeg").unwrap().1;
        let strip = format!("{}strip", self.target.cross_prefix().unwrap_or_default());
        for program in ["ffmpeg", "ffprobe"] {
            let exe = self.target.exe(program);
            let dest = stage.join(&exe);
            std::fs::copy(ffmpeg_dir.join(&exe), &dest)?;
            run(Command::new(&strip).arg(&dest))?;
        }
        let mut readme = format!(
            "Minimal ffmpeg {} build for tgradish, made by `cargo xtask ffmpeg`.\n\
             ffmpeg is licensed under the LGPL 2.1 or later. Sources:\n\n",
            FFMPEG.version
        );
        for (source, dir) in dirs {
            readme.push_str(&format!("{} {}: {}\n", source.name, source.version, source.url));
            for license in source.licenses {
                std::fs::copy(
                    dir.join(license),
                    stage.join("licenses").join(format!("{}-{license}", source.name)),
                )?;
            }
        }
        std::fs::write(stage.join("README.txt"), readme)?;

        let archive = self.out.join(format!("{name}.tar.gz"));
        run(Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&self.out).arg(&name))?;
        let sum = output(Command::new("sha256sum").arg(&archive))?;
        std::fs::write(archive.with_extension("gz.sha256"), &sum)?;
        Ok(archive)
    }
}

fn build_ffmpeg(args: &[String]) -> Result<()> {
    let mut target = Target::Linux;
    let mut asm = true;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--target" => {
                target = match iter.next().map(String::as_str) {
                    Some("linux") => Target::Linux,
                    Some("windows") => Target::Windows,
                    other => bail!("unknown target {other:?}, expected linux or windows"),
                }
            }
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
    if target == Target::Windows {
        build.static_pthread()?;
    }
    let archive = build.package(&dirs)?;
    println!("{}", archive.display());
    Ok(())
}

/// Packs the exact sources of the libraries in the ffmpeg build, for
/// publishing next to binaries as the LGPL asks.
fn package_sources() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let out = root.join("target/ffmpeg");
    let build = Build {
        target: Target::Linux,
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

/// Packs a release archive with the tgradish binary built with
/// `--features linked-static --target <triple>`, and copies the ffmpeg
/// executables archive for `tgradish ffmpeg download` next to it.
fn package_release(args: &[String]) -> Result<()> {
    let target = match args {
        [flag, name] if flag == "--target" => match name.as_str() {
            "linux" => Target::Linux,
            "windows" => Target::Windows,
            other => bail!("unknown target {other:?}, expected linux or windows"),
        },
        _ => bail!("usage: cargo xtask package --target linux|windows"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let version = workspace_version(root)?;
    let triple = target.triple();
    let binary = root.join("target").join(triple).join("release").join(target.exe("tgradish"));
    ensure!(binary.is_file(), "{} is missing, build it first", binary.display());
    let ffmpeg_dir = root.join(format!("target/ffmpeg/ffmpeg-{}-{triple}", FFMPEG.version));
    ensure!(ffmpeg_dir.is_dir(), "{} is missing, run cargo xtask ffmpeg", ffmpeg_dir.display());

    let out = root.join("target/release-artifacts");
    let name = format!("tgradish-{version}-{triple}");
    let stage = out.join(&name);
    if stage.exists() {
        std::fs::remove_dir_all(&stage)?;
    }
    std::fs::create_dir_all(stage.join("licenses"))?;
    std::fs::copy(&binary, stage.join(target.exe("tgradish")))?;
    std::fs::copy(root.join("README.md"), stage.join("README.md"))?;
    std::fs::copy(root.join("LICENSE.txt"), stage.join("LICENSE.txt"))?;
    for entry in std::fs::read_dir(ffmpeg_dir.join("licenses"))? {
        let entry = entry?;
        std::fs::copy(entry.path(), stage.join("licenses").join(entry.file_name()))?;
    }
    std::fs::write(
        stage.join("THIRD-PARTY.txt"),
        format!(
            "tgradish is MIT licensed, see LICENSE.txt.\n\n\
             It includes ffmpeg {ffmpeg} (LGPL 2.1 or later) built with zlib, libvpx and\n\
             dav1d, linked statically. Their licenses are in licenses/. The exact\n\
             sources are in ffmpeg-{ffmpeg}-sources.tar, published with this release;\n\
             docs/ffmpeg.md in the tgradish repository explains how to rebuild\n\
             tgradish against a modified ffmpeg.\n",
            ffmpeg = FFMPEG.version,
        ),
    )?;

    if target == Target::Windows {
        check_dll_imports(&binary)?;
    }

    let archive = match target {
        Target::Linux => {
            let archive = out.join(format!("{name}.tar.gz"));
            run(Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&out).arg(&name))?;
            archive
        }
        Target::Windows => {
            let archive = out.join(format!("{name}.zip"));
            if archive.exists() {
                std::fs::remove_file(&archive)?;
            }
            run(Command::new("zip").current_dir(&out).arg("-qr").arg(&archive).arg(&name))?;
            archive
        }
    };
    sha256_file(&archive)?;

    let ffmpeg_archive = format!("ffmpeg-{}-{triple}.tar.gz", FFMPEG.version);
    std::fs::copy(root.join("target/ffmpeg").join(&ffmpeg_archive), out.join(&ffmpeg_archive))?;
    sha256_file(&out.join(&ffmpeg_archive))?;
    println!("{}", archive.display());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ffmpeg") => build_ffmpeg(&args[1..]),
        Some("ffmpeg-sources") => package_sources(),
        Some("package") => package_release(&args[1..]),
        _ => bail!(
            "usage: cargo xtask ffmpeg [--target linux|windows] [--no-asm]\n       \
             cargo xtask ffmpeg-sources\n       \
             cargo xtask package --target linux|windows"
        ),
    }
}
