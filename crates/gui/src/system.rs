//! Folders and web addresses opened with the system's own programs, as a
//! file manager or browser would open them; eframe opens links only with
//! a dependency much larger than this. Tests note the addresses instead.

use std::path::Path;

/// Opens the folder holding `path`, with the file selected where the
/// system can do that.
pub(crate) fn reveal(path: &Path) {
    #[cfg(target_os = "windows")]
    wait_for(std::process::Command::new("explorer").arg("/select,").arg(path).spawn());
    #[cfg(target_os = "macos")]
    wait_for(std::process::Command::new("open").arg("-R").arg(path).spawn());
    // the file manager is asked over D-Bus, which may start it and so takes
    // a while; the folder alone where none answers
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let path = path.to_path_buf();
        let _ = std::thread::Builder::new().name("reveal".into()).spawn(move || {
            let selected = path
                .canonicalize()
                .ok()
                .and_then(|path| std::ffi::CString::new(file_uri(&path)).ok())
                .is_some_and(|uri| crate::dbus::show_items(&uri));
            if !selected {
                let folder =
                    path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
                if let Ok(mut child) = std::process::Command::new("xdg-open").arg(folder).spawn() {
                    let _ = child.wait();
                }
            }
        });
    }
}

/// `path`, absolute, as a file:// address, its bytes beyond letters,
/// digits and `-._~/` escaped.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// Opens a web address in the system's browser; anything else, which
/// could be a file or an option, is left alone. A link to a Telegram chat
/// opens in Telegram's app where one is installed, as [`telegram`] says.
pub(crate) fn browse(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    // tests only note it; the condition keeps what follows reachable
    #[cfg(test)]
    if cfg!(test) {
        BROWSED.lock().unwrap().push(url.to_owned());
        return;
    }
    match app_link(url) {
        // the app asked about, and perhaps tried, on a thread of its own, as
        // that runs programs and waits for them
        Some(app) => {
            let web = url.to_owned();
            let _ = std::thread::Builder::new()
                .name("telegram".into())
                .spawn(move || telegram(&Host, &app, &web));
        }
        None => wait_for(opener(url).spawn()),
    }
}

/// The program that opens `url` as the system's file manager would.
fn opener(url: &str) -> std::process::Command {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    let mut command = std::process::Command::new(program);
    command.arg(url);
    command
}

/// The `tg:` link that opens the chat a `https://t.me/` link names in
/// Telegram's app, as https://core.telegram.org/api/links has them: a
/// username, `t.me/name`, is `tg://resolve?domain=name`, and the direct
/// messages of a channel, `t.me/name?direct`, are that with `&direct`.
/// Other links of t.me, and others, have none here.
pub(crate) fn app_link(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://t.me/")?;
    let (name, query) = rest.split_once('?').unwrap_or((rest, ""));
    let name = name.strip_suffix('/').unwrap_or(name);
    let username = |c: char| c.is_ascii_alphanumeric() || c == '_';
    if name.len() < 4 || !name.chars().all(username) {
        return None;
    }
    match query {
        "" => Some(format!("tg://resolve?domain={name}")),
        "direct" => Some(format!("tg://resolve?domain={name}&direct")),
        _ => None,
    }
}

/// What the system says of Telegram's app, and the way it opens links.
pub(crate) trait Telegram {
    /// Whether a program takes `tg:` links, where the system can say;
    /// `None` where it can't.
    fn takes_tg_links(&self) -> Option<bool>;
    /// Whether an unknown answer is worth trying: where opening a link no
    /// program takes fails, so the web one can follow, rather than leave a
    /// dialog of its own up.
    fn may_try(&self) -> bool;
    /// Opens `url`, and whether that went: ran and ended well, as far as the
    /// system says.
    fn open(&self, url: &str) -> bool;
}

/// Opens a chat in Telegram's app, by `app`, its `tg:` link, where the
/// system says a program takes those, or can't say and failing there
/// shows; else, or where that fails, on the web, by `web`. Something is
/// always opened.
pub(crate) fn telegram(system: &dyn Telegram, app: &str, web: &str) {
    let tried = match system.takes_tg_links() {
        Some(takes) => takes,
        None => system.may_try(),
    };
    if !(tried && system.open(app)) {
        system.open(web);
    }
}

/// The system tgradish runs on, as [`Telegram`] asks of it.
struct Host;

impl Telegram for Host {
    /// Linux and the like: the desktop's default for `x-scheme-handler/tg`,
    /// as `xdg-mime` reads it, unless in a Flatpak or Snap sandbox, which
    /// knows nothing of the system's programs and opens links through the
    /// desktop portal. Windows: a `tg` protocol registered, which Telegram
    /// Desktop's installer does for the user or the machine. macOS: not
    /// asked, as `open` fails where no app takes the link.
    fn takes_tg_links(&self) -> Option<bool> {
        #[cfg(target_os = "windows")]
        return Some(windows_takes_tg());
        #[cfg(target_os = "macos")]
        return None;
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            if sandboxed() {
                return None;
            }
            let asked = std::process::Command::new("xdg-mime")
                .args(["query", "default", "x-scheme-handler/tg"])
                .stderr(std::process::Stdio::null())
                .output()
                .ok()?;
            Some(
                asked.status.success() && !String::from_utf8_lossy(&asked.stdout).trim().is_empty(),
            )
        }
    }

    fn may_try(&self) -> bool {
        // where xdg-mime is missing but not in a sandbox, xdg-open fails
        // as plainly as macOS's `open`
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        return !sandboxed();
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        return cfg!(target_os = "macos");
    }

    fn open(&self, url: &str) -> bool {
        // through the shell, which says whether it opened it: Explorer
        // ends with 1 either way
        #[cfg(target_os = "windows")]
        return windows_open(url);
        #[cfg(not(target_os = "windows"))]
        {
            let Ok(mut child) = opener(url).spawn() else { return false };
            child.wait().is_ok_and(|status| status.success())
        }
    }
}

/// Opens `url` as the shell opens it, and whether that went: a `tg:` link
/// whose registered program is gone fails here, rather than nothing
/// happening, so the web link can follow.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn windows_open(url: &str) -> bool {
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx,
    };
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(url));
    // SAFETY: COM is set up for this thread, which the shell may need for
    // the program a link opens with, as Microsoft asks before
    // ShellExecute; both strings end in a NUL, and the null window and
    // parameters are allowed
    let opened = unsafe {
        let flags = (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32;
        CoInitializeEx(std::ptr::null(), flags);
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // a value above 32 is success, as ShellExecute has it
    opened as usize > 32
}

/// The program of a command line as the registry keeps one for opening a
/// link, `"C:\path\Telegram.exe" -- "%1"` or `C:\path\Telegram.exe -- %1`:
/// the quoted part, or else up to the first `.exe` and the rest.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn command_program(command: &str) -> Option<std::path::PathBuf> {
    let command = command.trim();
    let program = match command.strip_prefix('"') {
        Some(rest) => rest.split_once('"').map(|(program, _)| program)?,
        None => {
            let end = command.to_ascii_lowercase().find(".exe").map(|at| at + 4);
            end.map_or_else(
                || command.split_whitespace().next().unwrap_or(""),
                |end| &command[..end],
            )
        }
    };
    (!program.is_empty()).then(|| std::path::PathBuf::from(program))
}

/// Whether tgradish runs in a Flatpak or Snap sandbox.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn sandboxed() -> bool {
    Path::new("/.flatpak-info").exists()
        || std::env::var_os("FLATPAK_ID").is_some()
        || std::env::var_os("SNAP").is_some()
}

/// Whether a `tg` URL protocol is registered, for the user or the machine,
/// with a command to open it with whose program is there: one left behind
/// by a Telegram since removed opens nothing.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn windows_takes_tg() -> bool {
    use windows_sys::Win32::System::Registry::{
        HKEY_CLASSES_ROOT, RRF_RT_ANY, RRF_RT_REG_SZ, RegGetValueW,
    };
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    // the value's size, or with `data` its text, where it is there
    let value = |key: &str, name: Option<&str>, data: Option<&mut Vec<u16>>| {
        let key = wide(key);
        let name = name.map(wide);
        let name = name.as_ref().map_or(std::ptr::null(), |name| name.as_ptr());
        let mut size = data.as_ref().map_or(0, |data| (data.len() * 2) as u32);
        let (flags, pointer) = match data {
            Some(data) => (RRF_RT_REG_SZ, data.as_mut_ptr().cast()),
            None => (RRF_RT_ANY, std::ptr::null_mut()),
        };
        // SAFETY: the key and the name, where given, end in a NUL; `size`
        // says how many bytes `pointer` has room for, none where it is null
        let status = unsafe {
            RegGetValueW(
                HKEY_CLASSES_ROOT,
                key.as_ptr(),
                name,
                flags,
                std::ptr::null_mut(),
                pointer,
                &mut size,
            )
        };
        (status == 0).then_some(size)
    };
    let command_key = "tg\\shell\\open\\command";
    if value("tg", Some("URL Protocol"), None).is_none() {
        return false;
    }
    let Some(size) = value(command_key, None, None) else { return false };
    let mut text = vec![0u16; (size as usize).div_ceil(2).max(1)];
    if value(command_key, None, Some(&mut text)).is_none() {
        return false;
    }
    let end = text.iter().position(|&unit| unit == 0).unwrap_or(text.len());
    let command = String::from_utf16_lossy(&text[..end]);
    command_program(&command).is_some_and(|program| program.is_file())
}

/// Web addresses tests asked to open, rather than opening them.
#[cfg(test)]
pub(crate) static BROWSED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Waits for a program started to open something to end, on a thread of
/// its own, so it doesn't linger as a zombie.
fn wait_for(started: std::io::Result<std::process::Child>) {
    if let Ok(mut child) = started {
        std::thread::spawn(move || child.wait());
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::testing::harness;

    #[test]
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fn names_files_as_addresses() {
        assert_eq!(
            file_uri(Path::new("/tmp/a b/ü,1%.webm")),
            "file:///tmp/a%20b/%C3%BC%2C1%25.webm"
        );
    }

    #[test]
    fn opens_telegram_chats_in_the_app() {
        let direct = "tg://resolve?domain=s0untitled&direct";
        assert_eq!(app_link("https://t.me/s0untitled?direct").as_deref(), Some(direct));
        let resolve = "tg://resolve?domain=durov";
        assert_eq!(app_link("https://t.me/durov").as_deref(), Some(resolve));
        for other in ["https://t.me/+invite", "https://t.me/s/channel?x", "https://example.org"] {
            assert_eq!(app_link(other), None, "{other}");
        }
    }

    #[test]
    fn finds_the_program_of_a_registered_command() {
        let program = |command| command_program(command).map(|path| path.display().to_string());
        let quoted = r#""C:\Users\me\AppData\Roaming\Telegram Desktop\Telegram.exe" -- "%1""#;
        let expected = r"C:\Users\me\AppData\Roaming\Telegram Desktop\Telegram.exe";
        assert_eq!(program(quoted).as_deref(), Some(expected));
        assert_eq!(
            program(r"C:\Telegram\Telegram.exe -- %1").as_deref(),
            Some(r"C:\Telegram\Telegram.exe")
        );
        assert_eq!(program("telegram %1").as_deref(), Some("telegram"));
        assert_eq!(program("  "), None);
        assert_eq!(program(r#""unclosed"#), None);
    }

    /// A system that says `takes`, `may_try`s or not, whose opening of
    /// `tg:` links goes as `app_opens` and is noted.
    struct Pretend {
        takes: Option<bool>,
        may_try: bool,
        app_opens: bool,
        opened: std::cell::RefCell<Vec<String>>,
    }

    impl Telegram for Pretend {
        fn takes_tg_links(&self) -> Option<bool> {
            self.takes
        }
        fn may_try(&self) -> bool {
            self.may_try
        }
        fn open(&self, url: &str) -> bool {
            self.opened.borrow_mut().push(url.to_owned());
            !url.starts_with("tg:") || self.app_opens
        }
    }

    #[test]
    fn opens_telegram_or_else_the_web() {
        let (app, web) =
            ("tg://resolve?domain=s0untitled&direct", "https://t.me/s0untitled?direct");
        let opened = |takes, may_try, app_opens| {
            let system =
                Pretend { takes, may_try, app_opens, opened: std::cell::RefCell::default() };
            telegram(&system, app, web);
            system.opened.into_inner()
        };
        // a program takes them: the app, and only that where it opens
        assert_eq!(opened(Some(true), false, true), [app]);
        // the web where it fails, so something always opens
        assert_eq!(opened(Some(true), false, false), [app, web]);
        // none does: the web alone, not a dialog of the system's
        assert_eq!(opened(Some(false), true, true), [web]);
        // the system can't say: tried where failing shows, as on macOS
        assert_eq!(opened(None, true, true), [app]);
        assert_eq!(opened(None, true, false), [app, web]);
        // and not in a sandbox, which can't tell either way
        assert_eq!(opened(None, false, true), [web]);
    }

    #[test]
    fn opens_links() {
        let mut harness = harness(Config::default());
        harness.state_mut().settings_dialog.about = true;
        harness.run_steps(2);
        // a link, a label whose text is its name rather than its value
        let link = egui_kittest::kittest::by()
            .predicate(|node| node.label().as_deref() == Some("source code"));
        harness.get(link).click_accesskit();
        harness.run_steps(2);
        let browsed = BROWSED.lock().unwrap();
        assert!(
            browsed.iter().any(|url| url == "https://github.com/sliva0/tgradish"),
            "{browsed:?}"
        );
        drop(browsed);
        harness.get_by_label("Send feedback on Telegram").click_accesskit();
        harness.run_steps(2);
        let browsed = BROWSED.lock().unwrap();
        assert!(browsed.iter().any(|url| url == "https://t.me/s0untitled?direct"), "{browsed:?}");
        drop(browsed);
        // and from Settings, beside the way to About
        let before = BROWSED.lock().unwrap().len();
        harness.state_mut().settings_dialog.about = false;
        harness.state_mut().settings_dialog.open();
        harness.run_steps(2);
        harness.get_by_label("Send feedback").click_accesskit();
        harness.run_steps(2);
        let browsed = BROWSED.lock().unwrap();
        assert_eq!(browsed[before..], ["https://t.me/s0untitled?direct"]);
    }
}
