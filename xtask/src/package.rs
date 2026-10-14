//! Release archives of a built tgradish, and for Linux builds with ffmpeg
//! built in also a .deb and an .rpm. Each holds the binary with every
//! notice it needs, and for Linux its menu entry and icons. Binaries are
//! checked before they are packed, since a release must run where it says
//! it does: Windows ones without MinGW's DLLs, Linux ones on a glibc as old
//! as asked. nfpm, which writes the packages, is downloaded pinned by its
//! hash, like ffmpeg's sources.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use crate::ffmpeg::{FFMPEG, Source, download};
use crate::icons::ICON_SIZES;
use crate::{Target, output, run, workspace_root};

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

/// The `package` task: [`package`] as its arguments ask.
pub fn package_release(args: &[String]) -> Result<()> {
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
    package(target, system_ffmpeg, max_glibc)
}

/// Packs a release archive: the tgradish binary built with
/// `--features linked-static --target <triple>`, and the licenses of the
/// ffmpeg build linked into it. With `system_ffmpeg`, the binary is one
/// built without ffmpeg instead. With `max_glibc`, fails if a Linux binary
/// needs a newer glibc.
pub fn package(target: Target, system_ffmpeg: bool, max_glibc: Option<(u32, u32)>) -> Result<()> {
    ensure!(!(system_ffmpeg && target.is_windows()), "Windows builds always link ffmpeg in");
    let root = workspace_root();
    // xtask shares the workspace's version, so this is tgradish's
    let version = env!("CARGO_PKG_VERSION");
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
        for package in linux_packages(root, target, version, &stage)? {
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
    // the glibc it was built against, which a build on a newer system
    // than CI's needs newer
    let (major, minor) = needed_glibc(&stage.join("tgradish"))?;
    let debs = std::iter::once(format!("libc6 (>= {major}.{minor})"))
        .chain(["libgcc-s1 | libgcc1".to_owned()])
        .chain(WINDOW_LIBRARIES.iter().map(|(deb, _)| (*deb).to_owned()))
        .collect();
    // as rpm names the libraries of 64-bit programs
    let glibc = format!("libc.so.6(GLIBC_{major}.{minor})");
    let rpms = [glibc.as_str(), "libgcc_s.so.1"]
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
    let newest = needed_glibc(exe)?;
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

/// The newest glibc `exe` needs: the newest version among its glibc
/// symbols.
fn needed_glibc(exe: &Path) -> Result<(u32, u32)> {
    let dump = output(Command::new("objdump").arg("-T").arg(exe))?;
    dump.split_whitespace()
        .filter_map(|word| word.trim_matches(['(', ')']).strip_prefix("GLIBC_"))
        .filter_map(|version| glibc_version(version).ok())
        .max()
        .context("the executable uses no versioned glibc symbols")
}
