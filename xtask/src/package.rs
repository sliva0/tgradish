//! Builds release archives, plus `.deb` and `.rpm` packages (via `nfpm`,
//! downloaded and pinned by SHA-256) for glibc builds with built-in ffmpeg,
//! and a standalone `tgradish.exe` for Windows (published on its own too
//! because it embeds all required licence notices).
//!
//! Each archive contains the binary and all required licence notices, plus the
//! desktop entry, icons, man pages, and shell completions on Linux. Archive
//! and package filenames omit the version so `/releases/latest/download/`
//! links stay stable; the git tag and the inner archive directory carry the
//! version so two unpacked versions do not collide. Binaries are verified
//! before packaging so releases run everywhere promised: no DLLs beyond
//! Windows' own (such as MinGW's), glibc builds no newer than `--max-glibc`
//! asks, and musl builds linked only to `libc.so` via musl's loader.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

use crate::ffmpeg::{FFMPEG, Source, download, sources_archive};
use crate::icons::ICON_SIZES;
use crate::{NOTICES, Target, output, run, workspace_root};

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
    // the GUI's
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
    ensure!(foreign.is_empty(), "{} imports non-system DLLs: {foreign:?}", exe.display());
    println!("DLL imports: {}", imports.join(", "));
    Ok(())
}

/// Where the GUI's fonts are, with their licences: those of
/// [`FONT_LICENCES`], which the binary carries too (`notices` in
/// crates/gui/src/about.rs) and the archives' `licenses/` has, as the
/// `tgradish` AUR package installs them.
const FONTS: &str = "crates/gui/assets/fonts";
const FONT_LICENCES: [&str; 3] =
    ["Ubuntu-LICENCE.txt", "JetBrainsMono-OFL.txt", "emoji-icon-font-LICENSE.txt"];

/// Packs a release archive: the tgradish binary built with
/// `--features linked-static --target <triple>`, and the licences of the
/// ffmpeg build linked into it. With `system_ffmpeg`, the binary is one
/// built without ffmpeg instead. With `max_glibc`, fails if a Linux binary
/// needs a newer glibc. Returns the binary as the archive has it.
pub fn package(
    target: Target,
    system_ffmpeg: bool,
    max_glibc: Option<(u32, u32)>,
) -> Result<PathBuf> {
    ensure!(!(system_ffmpeg && target.is_windows()), "Windows builds always include ffmpeg");
    let root = workspace_root();
    // xtask shares the workspace's version, so this is tgradish's
    let version = env!("CARGO_PKG_VERSION");
    let triple = target.triple();
    let binary = root.join("target").join(triple).join("release").join(target.exe("tgradish"));
    ensure!(binary.is_file(), "{} is missing; build it first", binary.display());
    let licenses = root.join(format!("target/ffmpeg/licenses-{triple}"));
    ensure!(
        system_ffmpeg || licenses.is_dir(),
        "{} is missing; run cargo xtask ffmpeg first",
        licenses.display()
    );

    let out = root.join("target/release-artifacts");
    let name = format!("tgradish-{version}-{}", variant(target, system_ffmpeg));
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
    if !target.is_windows() {
        shell_files(&binary, &stage, target)?;
    }
    // libwebp, which every build has for static stickers
    std::fs::create_dir_all(stage.join("licenses"))?;
    for file in ["COPYING", "PATENTS"] {
        let to = stage.join("licenses").join(format!("libwebp-{file}"));
        std::fs::copy(root.join(NOTICES).join("libwebp").join(file), to)?;
    }
    // the GUI's fonts, which every build has too
    for file in FONT_LICENCES {
        std::fs::copy(root.join(FONTS).join(file), stage.join("licenses").join(file))?;
    }
    let third_party = if system_ffmpeg {
        "tgradish is under the MIT licence (see LICENSE.txt).\n\n\
         This build does not include ffmpeg. It runs ffmpeg and ffprobe (6.0 or newer,\n\
         with libvpx for VP9) from PATH.\n"
            .to_owned()
    } else {
        for entry in std::fs::read_dir(&licenses)? {
            let entry = entry?;
            std::fs::copy(entry.path(), stage.join("licenses").join(entry.file_name()))?;
        }
        format!(
            "tgradish is under the MIT licence (see LICENSE.txt).\n\n\
             This build statically links ffmpeg {ffmpeg} (LGPL 2.1 or later) with zlib,\n\
             libvpx and dav1d. Their licences are in licenses/. The exact source code\n\
             and relinking instructions are in {sources}.tar,\n\
             published alongside this release.\n",
            ffmpeg = FFMPEG.version,
            sources = sources_archive(),
        )
    };
    let libwebp = "\nIt also includes libwebp (BSD 3-Clause) for static stickers. Its licence and\n\
                   patent grant are in licenses/libwebp-COPYING and licenses/libwebp-PATENTS.\n";
    let fonts = "\nThe GUI includes subsets of Ubuntu Light and Medium, as Ubuntu Light and\n\
                 Medium derivative tgradish (Ubuntu Font Licence 1.0), JetBrains Mono NL\n\
                 (SIL Open Font License 1.1) and emoji-icon-font (MIT). Their licences are in\n\
                 licenses/Ubuntu-LICENCE.txt, licenses/JetBrainsMono-OFL.txt and\n\
                 licenses/emoji-icon-font-LICENSE.txt.\n";
    std::fs::write(
        stage.join("THIRD-PARTY.txt"),
        third_party
            + libwebp
            + fonts
            + "\nThe included Rust crates and their licences are listed in\n\
               THIRD-PARTY-CRATES.txt.\n",
    )?;
    // the crates of every build, as the binary carries them
    std::fs::copy(
        root.join(NOTICES).join("THIRD-PARTY-CRATES.txt"),
        stage.join("THIRD-PARTY-CRATES.txt"),
    )?;

    if target.is_windows() {
        check_dll_imports(&binary)?;
    }
    if target.is_musl() {
        check_musl_links(&binary, target)?;
    }
    if let Some(max) = max_glibc {
        check_glibc(&binary, max)?;
    }

    let archive = out.join(archive_name(target, system_ffmpeg));
    if target.is_windows() {
        if archive.exists() {
            std::fs::remove_file(&archive)?;
        }
        run(Command::new("zip").current_dir(&out).arg("-qr").arg(&archive).arg(&name))?;
    } else {
        run(Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&out).arg(&name))?;
    }
    sha256_file(&archive)?;
    println!("{}", archive.display());

    // the executable alone, for those who want no archive: it carries the
    // licences and notices of the archive's licenses/ in itself, which
    // `tgradish licenses` prints, and ffmpeg's sources are beside it in the
    // release, as the LGPL asks
    if target.is_windows() {
        let exe = out.join(target.exe("tgradish"));
        std::fs::copy(&binary, &exe)?;
        sha256_file(&exe)?;
        println!("{}", exe.display());
    }

    // with ffmpeg built in, as the system's may be too old or lack codecs;
    // musl distributions have packages of their own
    if !target.is_windows() && !target.is_musl() && !system_ffmpeg {
        for package in linux_packages(root, target, version, &stage)? {
            sha256_file(&package)?;
            println!("{}", package.display());
        }
    }
    Ok(stage.join(target.exe("tgradish")))
}

/// A build as the release's files and folders name it: its target, and
/// `-system-ffmpeg` after it for one without ffmpeg.
fn variant(target: Target, system_ffmpeg: bool) -> String {
    match system_ffmpeg {
        true => format!("{}-system-ffmpeg", target.name()),
        false => target.name().to_owned(),
    }
}

/// The release's archive of a build, as `tgradish-linux-x86_64.tar.gz`.
fn archive_name(target: Target, system_ffmpeg: bool) -> String {
    let extension = if target.is_windows() { "zip" } else { "tar.gz" };
    format!("tgradish-{}.{extension}", variant(target, system_ffmpeg))
}

/// The release's .deb and .rpm of a glibc build, as
/// `tgradish-linux-x86_64.deb`: named like the archives, by the processor
/// as `uname -m` names it rather than as Debian does (amd64, arm64), so
/// one `$(uname -m)` picks any of a machine's files. apt, dnf and zypper
/// install a package whatever its file is called.
fn package_names(target: Target) -> [String; 2] {
    ["deb", "rpm"].map(|extension| format!("tgradish-{}.{extension}", target.name()))
}

/// The completion scripts of bash, zsh, fish and nushell, as
/// `completions/`, and the man pages, gzipped as `man/*.1.gz`, written by
/// the built `binary` into `stage`. A musl build that this machine can't run, without musl's
/// loader, goes without them, as it says.
fn shell_files(binary: &Path, stage: &Path, target: Target) -> Result<()> {
    let man = stage.join("man");
    if let Err(err) = run(Command::new(binary).arg("__man").arg(&man)) {
        ensure!(target.is_musl(), err);
        println!("no man pages or completions in the archive: {err:#}");
        return Ok(());
    }
    for page in std::fs::read_dir(&man)? {
        // -n leaves out the time, so the same pages pack the same
        run(Command::new("gzip").arg("-n9").arg(page?.path()))?;
    }
    let completions = stage.join("completions");
    std::fs::create_dir_all(&completions)?;
    for (shell, name) in SHELL_FILES {
        let script = output(Command::new(binary).args(["completions", shell]))?;
        std::fs::write(completions.join(name), script)?;
    }
    Ok(())
}

/// Each shell's completion script, as the archive names it, and where
/// packages install it, from `/usr/share`.
const SHELL_FILES: [(&str, &str); 4] = [
    ("bash", "tgradish.bash"),
    ("zsh", "_tgradish"),
    ("fish", "tgradish.fish"),
    ("nushell", "tgradish.nu"),
];

/// Where packages install each of [`SHELL_FILES`], under `/usr/share`.
/// Nushell sources every file in its vendor autoload folders as it starts,
/// those of `XDG_DATA_DIRS` among them, which are for packages' files.
const SHELL_PATHS: [&str; 4] = [
    "bash-completion/completions/tgradish",
    "zsh/site-functions/_tgradish",
    "fish/vendor_completions.d/tgradish.fish",
    "nushell/vendor/autoload/tgradish.nu",
];

/// What the GUI loads when it opens, as Debian packages and as the
/// libraries an .rpm can ask for by name, which Fedora and openSUSE both
/// resolve: the graphics libraries, and D-Bus for file dialogs. winit opens
/// libX11, libX11-xcb, libXcursor and libXi, and does RandR through x11rb,
/// which loads libxcb; glutin opens libXrender, and egui libEGL and libGL;
/// xkbcommon-dl loads libxkbcommon and its X11 half. The AUR packages'
/// `depends` and `packaging/nix/package.nix` list the same, by their own
/// names.
const GUI_LIBRARIES: [(&str, &str); 13] = [
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

/// What tgradish is, as short as a package's summary has to be: the .deb's
/// and .rpm's, and the AUR packages' `pkgdesc`. Where length is free, as in
/// `crates/cli/Cargo.toml`, the sentence names the kinds of sticker too.
const SUMMARY: &str = "Telegram stickers and emoji from videos, images or pixel art";

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
    for ((_, name), path) in SHELL_FILES.iter().zip(SHELL_PATHS) {
        let script = stage.join("completions").join(name);
        contents.push(format!("  - src: {}\n    dst: /usr/share/{path}", quoted(&script)));
    }
    for page in std::fs::read_dir(stage.join("man"))? {
        let page = page?;
        let name = page.file_name().to_string_lossy().into_owned();
        contents.push(format!(
            "  - src: {}\n    dst: /usr/share/man/man1/{name}",
            quoted(&page.path())
        ));
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
    // Debian looks for a package's licences in its copyright file, and
    // nowhere else
    let copyright = stage.parent().unwrap().join(format!("copyright-{}", target.triple()));
    std::fs::write(&copyright, debian_copyright(stage)?)?;
    contents.push(format!(
        "  - src: {}\n    dst: /usr/share/doc/tgradish/copyright\n    packager: deb",
        quoted(&copyright)
    ));
    contents.sort();
    let list = |names: Vec<String>| {
        names.iter().map(|name| format!("\n      - {name:?}")).collect::<String>()
    };
    // the glibc it was built against, which a build on a newer system
    // than CI's needs newer
    let (major, minor) = needed_glibc(&stage.join("tgradish"))?;
    let debs = std::iter::once(format!("libc6 (>= {major}.{minor})"))
        .chain(["libgcc-s1 | libgcc1".to_owned()])
        .chain(GUI_LIBRARIES.iter().map(|(deb, _)| (*deb).to_owned()))
        .collect();
    // as rpm names the libraries of 64-bit programs
    let glibc = format!("libc.so.6(GLIBC_{major}.{minor})");
    let rpms = [glibc.as_str(), "libgcc_s.so.1"]
        .into_iter()
        .chain(GUI_LIBRARIES.iter().map(|(_, library)| *library))
        .map(|library| match library.ends_with(')') {
            true => format!("{library}(64bit)"),
            false => format!("{library}()(64bit)"),
        })
        .collect();
    let arch = match target {
        Target::LinuxX64 => "amd64",
        Target::LinuxArm64 => "arm64",
        Target::LinuxMuslX64 | Target::LinuxMuslArm64 => bail!("musl builds have no .deb or .rpm"),
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
         maintainer: {maintainer:?}\n\
         description: |-\n  \
           {SUMMARY}\n  \
           Converts videos and images into Telegram video (WebM) and static (WebP)\n  \
           stickers and custom emoji, and pixel art into animated (TGS) stickers,\n  \
           within Telegram's limits. Includes a built-in ffmpeg.\n\
         homepage: {homepage:?}\n\
         license: \"MIT AND LGPL-2.1-or-later AND BSD-3-Clause AND BSD-2-Clause AND Zlib \
           AND Ubuntu-font-1.0 AND OFL-1.1\"\n\
         contents:\n{contents}\n\
         overrides:\n  \
           deb:\n    depends:{debs}\n    recommends:\n      - xdg-desktop-portal\n    \
           suggests:\n      - ffmpeg\n  \
           rpm:\n    depends:{rpms}\n    recommends:\n      - xdg-desktop-portal\n    \
           suggests:\n      - ffmpeg\n",
        // the workspace's, which xtask shares; the first author keeps the
        // packages
        maintainer = env!("CARGO_PKG_AUTHORS").split(':').next().unwrap_or_default(),
        homepage = env!("CARGO_PKG_REPOSITORY"),
        contents = contents.join("\n"),
        debs = list(debs),
        rpms = list(rpms),
    );
    let out = stage.parent().unwrap();
    let config_path = out.join(format!("nfpm-{}.yaml", target.triple()));
    std::fs::write(&config_path, config)?;
    let [deb, rpm] = package_names(target);
    let packages = [("deb", out.join(deb)), ("rpm", out.join(rpm))];
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

/// The .deb's `copyright`: the staged release's THIRD-PARTY.txt, with its
/// paths where the package installs them, and tgradish's licence.
fn debian_copyright(stage: &Path) -> Result<String> {
    let licence = std::fs::read_to_string(stage.join("LICENSE.txt"))?;
    let third_party = std::fs::read_to_string(stage.join("THIRD-PARTY.txt"))?
        .replace("see LICENSE.txt", "below")
        .replace("licenses/", "/usr/share/licenses/tgradish/")
        .replace("THIRD-PARTY-CRATES.txt", "/usr/share/doc/tgradish/THIRD-PARTY-CRATES.txt");
    Ok(format!("tgradish\n{}\n\n{third_party}\n{licence}", env!("CARGO_PKG_REPOSITORY")))
}

/// What a musl build may be linked to: musl alone, by the name musl-gcc
/// links it as, which musl's loader takes for itself. The GUI's
/// libraries are loaded when it opens, so the command line runs without
/// them, and the unwinder is linked in (see `static_unwinder` in build.rs).
const MUSL_LINKS: [&str; 1] = ["libc.so"];

/// Fails if a musl build doesn't start with musl's loader where musl
/// distributions keep it, or is linked to more than [`MUSL_LINKS`], as
/// to a glibc library the build machine has.
fn check_musl_links(exe: &Path, target: Target) -> Result<()> {
    let headers = output(Command::new("readelf").args(["-l", "-d"]).arg(exe))?;
    let loader = target.musl_loader();
    ensure!(
        headers.contains(&format!("[Requesting program interpreter: {loader}]")),
        "{} does not use interpreter {loader}",
        exe.display()
    );
    let links: Vec<&str> = headers
        .lines()
        .filter_map(|line| line.split_once("Shared library: [")?.1.strip_suffix(']'))
        .collect();
    let foreign: Vec<_> = links.iter().filter(|link| !MUSL_LINKS.contains(link)).collect();
    ensure!(foreign.is_empty(), "{} links against non-musl libraries: {foreign:?}", exe.display());
    println!("linked to: {}", links.join(", "));
    Ok(())
}

/// `2.28` as `(2, 28)`; `2.2.5` is `(2, 2)`.
pub fn glibc_version(text: &str) -> Result<(u32, u32)> {
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
        .context("no versioned glibc symbols found in executable")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file a release has, as the release workflow builds them with
    /// `package` and gathers them: each build's archive, the glibc ones
    /// also without ffmpeg and as a .deb and an .rpm, Windows's executable,
    /// ffmpeg's sources, and the sums.
    fn release_files() -> Vec<String> {
        let mut files = vec![];
        for target in Target::ALL {
            let glibc = !target.is_windows() && !target.is_musl();
            files.push(archive_name(target, false));
            if glibc {
                files.push(archive_name(target, true));
                files.extend(package_names(target));
            }
        }
        files.push(Target::WindowsX64.exe("tgradish"));
        // each built file has its own sum beside it
        let sums: Vec<String> = files.iter().map(|file| format!("{file}.sha256")).collect();
        files.extend(sums);
        files.push(format!("{}.tar", sources_archive()));
        files.push("SHA256SUMS".to_owned());
        files
    }

    /// The names of the release's files, every one different, which the
    /// workflow gathers into one folder, and none with the version in it.
    #[test]
    fn release_files_are_named_apart_and_without_the_version() {
        let files = release_files();
        let mut sorted = files.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), files.len(), "{files:?}");
        for file in &files {
            assert!(!file.contains(env!("CARGO_PKG_VERSION")), "{file} has the version in it");
        }
    }

    /// The files the README names, in links to the latest release and in
    /// the commands around them, and those the -bin AUR packages download
    /// from a release, are files releases have: `$(uname -m)` stands for
    /// either processor.
    #[test]
    fn links_name_files_releases_have() {
        let files = release_files();
        let root = workspace_root();
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        let has = |path: &str, name: &str| {
            for arch in ["x86_64", "aarch64"] {
                let name = name.replace("{arch}", arch);
                assert!(files.contains(&name), "{path} names {name}, which releases lack");
            }
        };
        // the README: every name of a release's file, in a link or not
        let readme = read("README.md").replace("$(uname -m)", "{arch}");
        let named: Vec<&str> = readme
            .split(|c: char| !(c.is_ascii_alphanumeric() || "-_.{}".contains(c)))
            .filter(|word| word.starts_with("tgradish"))
            .filter(|word| {
                [".tar.gz", ".zip", ".deb", ".rpm", ".exe"].iter().any(|ext| word.ends_with(ext))
            })
            .collect();
        assert!(named.len() >= 6, "{named:?}");
        for name in named {
            has("README.md", name);
        }
        let links = readme.matches("/releases/latest/download/").count();
        assert!(links >= 5, "the README has {links} links to the latest release's files");
        // the AUR's: a release's files, by its tag
        for package in ["tgradish-bin", "tgradish-static-bin"] {
            let path = format!("packaging/aur/{package}/PKGBUILD");
            let text = read(&path).replace("$_pkgname", "tgradish");
            let downloads: Vec<&str> = text
                .split("/releases/download/")
                .skip(1)
                .map(|link| link.split_once('/').unwrap().1)
                .map(|link| link.split(['"', '\n', ')']).next().unwrap())
                .collect();
            assert_eq!(downloads.len(), 2, "{path}: {downloads:?}");
            for name in downloads {
                has(&path, name);
            }
        }
    }

    /// The fonts' licences that `package` copies are those the GUI carries.
    #[test]
    fn font_licences_are_the_guis() {
        let root = workspace_root();
        let about = std::fs::read_to_string(root.join("crates/gui/src/about.rs")).unwrap();
        for file in FONT_LICENCES {
            assert!(root.join(FONTS).join(file).is_file(), "{FONTS}/{file} is missing");
            let included = format!("include_str!(\"../assets/fonts/{file}\")");
            assert!(about.contains(&included), "about.rs doesn't carry {file}");
        }
    }

    #[test]
    fn debian_copyright_names_where_the_package_puts_things() {
        let stage = workspace_root().join("target/xtask-tests/copyright");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("LICENSE.txt"), "MIT License\n").unwrap();
        std::fs::write(
            stage.join("THIRD-PARTY.txt"),
            "tgradish is under the MIT licence (see LICENSE.txt).\n\nTheir licences are in licenses/. \
             See licenses/libwebp-COPYING.\n\nThe crates are in THIRD-PARTY-CRATES.txt.\n",
        )
        .unwrap();
        let copyright = debian_copyright(&stage).unwrap();
        assert!(copyright.contains("under the MIT licence (below)."), "{copyright}");
        assert!(copyright.contains("are in /usr/share/licenses/tgradish/."), "{copyright}");
        assert!(copyright.contains(" /usr/share/licenses/tgradish/libwebp-COPYING"), "{copyright}");
        assert!(
            copyright.contains(" /usr/share/doc/tgradish/THIRD-PARTY-CRATES.txt"),
            "{copyright}"
        );
        assert!(copyright.ends_with("MIT License\n"), "{copyright}");
    }

    /// What tgradish is said to be is written out wherever a format wants
    /// it, and had drifted into five wordings.
    #[test]
    fn everything_describes_tgradish_alike() {
        let root = workspace_root();
        let read = |path: &str| std::fs::read_to_string(root.join(path)).unwrap();
        let manifest = read("crates/cli/Cargo.toml");
        let sentence = manifest
            .lines()
            .find_map(|line| line.strip_prefix("description = \"")?.strip_suffix('"'))
            .unwrap();
        // the summary is the sentence without the kinds of sticker
        assert_eq!(sentence.replacen(" video, static and animated", "", 1), SUMMARY);
        for (path, line) in [
            ("flake.nix", format!("  description = \"{sentence}\";")),
            ("packaging/nix/package.nix", format!("    description = \"{sentence}\";")),
            ("crates/cli/assets/tgradish.desktop", format!("Comment={sentence}")),
        ] {
            assert!(read(path).lines().any(|found| found == line), "{path} lacks {line:?}");
        }
        // About in the GUI, before the licence and the link to the source
        let (path, line) = (
            "crates/gui/i18n/en-GB/tgradish.ftl",
            format!("about-summary = {SUMMARY}. MIT licence;"),
        );
        assert!(read(path).lines().any(|found| found == line), "{path} lacks {line:?}");
        // the AUR asks for about 80 characters at most, within which the
        // static build says it brings its own ffmpeg
        for (package, note) in
            [("tgradish", ""), ("tgradish-bin", ""), ("tgradish-static-bin", " (ffmpeg built-in)")]
        {
            let described = format!("{SUMMARY}{note}");
            assert!(described.len() <= 80, "{described:?} is too long for a pkgdesc");
            let (path, line) =
                (format!("packaging/aur/{package}/PKGBUILD"), format!("pkgdesc='{described}'"));
            assert!(read(&path).lines().any(|found| found == line), "{path} lacks {line:?}");
        }
    }
}
