//! One GUI at a time for each session. The GUI that keeps the session
//! listens beside it, on a Unix socket next to its lock, or on Windows a
//! named pipe named after it; a GUI started while it runs hands it the files
//! it was given, with what the system gave it to bring a window forward
//! with, and ends rather than open a second.
//!
//! What is handed over is one line of JSON, of no more than [`LONGEST`]
//! bytes, which the GUI answers with [`ACCEPTED`] once it has read it whole.
//! It holds paths to add, as dropped files are, and the tokens a window is
//! raised with, and nothing else is taken from it. A GUI that doesn't
//! answer within [`ANSWER`], or a socket that nothing listens on any more,
//! is given up on, and the GUI started opens as a second one would.
//!
//! The files are the started GUI's until it says otherwise: having the
//! answer in time, it writes [`CONFIRMED`] and ends, and the GUI that
//! keeps the session adds them only once it has read that. One that
//! answers too late, or is never told, adds nothing, so the files are
//! opened by one GUI at most, and never by both: the one byte is either
//! written, and the started GUI ends, or isn't, and it opens them itself.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::session::Store;

/// How long a GUI that keeps the session is waited for, to be reached and
/// to take the files, before the one started opens beside it.
const ANSWER: Duration = Duration::from_secs(1);
/// How often it is tried again meanwhile, as one that has only just
/// started may not listen yet.
const AGAIN: Duration = Duration::from_millis(25);
/// The most a message may be: room for as many files as a command line
/// may name.
const LONGEST: usize = 4 << 20;
/// What a GUI answers once it has the files.
const ACCEPTED: &[u8] = b"ok\n";
/// What the GUI started says once it has the answer, and leaves the files
/// to the one that keeps the session: a byte, which is written or not.
const CONFIRMED: &[u8] = b"!";
/// The layout of the message, so another is left alone: 2 is confirmed,
/// which 1 wasn't, so a GUI of either never takes what the other leaves.
const VERSION: u64 = 2;
/// The longest token a window is raised with that is passed on.
const LONGEST_TOKEN: usize = 1024;

/// What starting the GUI came to, once, as it starts, so its size is no
/// matter.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Start {
    /// It is the one that keeps the session, and listens for the others
    /// where it can.
    First { store: Store, listener: Option<Listener> },
    /// The GUI that keeps the session took the files.
    HandedOver,
    /// Another keeps the session but couldn't be reached: this one opens
    /// beside it, keeping none.
    Second,
}

/// Starts the GUI whose session is at `session`: the one that keeps it,
/// where none does, or else hands `request` to the one that does.
pub(crate) fn start(session: PathBuf, request: &Request) -> Start {
    let address = transport::address(&session);
    match Store::open(session) {
        // where there is no telling whether another runs, as on a file
        // system without locks, none is looked for or listened as
        Some(store) if !store.locked() => Start::First { store, listener: None },
        Some(store) => {
            let listener = address.and_then(|address| Listener::start(&address).ok());
            Start::First { store, listener }
        }
        None => match address.map(|address| hand_over(&address, request, ANSWER)) {
            Some(Ok(())) => Start::HandedOver,
            _ => Start::Second,
        },
    }
}

/// What a GUI started second asks of the first.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Request {
    /// The files to add, each a whole path.
    pub(crate) files: Vec<PathBuf>,
    /// The token Wayland raises a window with, as the launcher gave it in
    /// `XDG_ACTIVATION_TOKEN`.
    pub(crate) activation_token: Option<String>,
    /// X11's, `DESKTOP_STARTUP_ID`: passed on, though winit has no way to
    /// raise a window already open with it.
    pub(crate) startup_id: Option<String>,
}

impl Request {
    /// Asks to add `files`, with the tokens the environment has.
    pub(crate) fn from_env(files: Vec<PathBuf>) -> Request {
        let token = |name: &str| std::env::var(name).ok().filter(|token| plain_token(token));
        Request {
            files,
            activation_token: token("XDG_ACTIVATION_TOKEN"),
            startup_id: token("DESKTOP_STARTUP_ID"),
        }
    }

    /// The message, a line of JSON ending in a newline; `None` where it
    /// would be longer than a GUI reads.
    fn encode(&self) -> Option<Vec<u8>> {
        let files: Option<Vec<serde_json::Value>> =
            self.files.iter().map(|file| name(file)).collect();
        let message = serde_json::json!({
            "version": VERSION,
            "files": files?,
            "xdg-activation-token": self.activation_token,
            "desktop-startup-id": self.startup_id,
        });
        let mut line = serde_json::to_vec(&message).ok()?;
        line.push(b'\n');
        (line.len() <= LONGEST).then_some(line)
    }

    /// The request a message holds, or `None` where it isn't one of this
    /// layout, as what isn't a whole path to a file isn't.
    fn decode(line: &[u8]) -> Option<Request> {
        use serde_json::Value;
        let message: serde_json::Map<String, Value> = serde_json::from_slice(line).ok()?;
        if message.get("version")?.as_u64()? != VERSION {
            return None;
        }
        let files = message.get("files")?.as_array()?;
        let files: Vec<PathBuf> = files.iter().map(path).collect::<Option<_>>()?;
        if !files.iter().all(|file| file.is_absolute()) {
            return None;
        }
        let token = |key: &str| match message.get(key) {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(token)) if plain_token(token) => Some(Some(token.clone())),
            Some(_) => None,
        };
        Some(Request {
            files,
            activation_token: token("xdg-activation-token")?,
            startup_id: token("desktop-startup-id")?,
        })
    }
}

/// Whether `token` may be passed on: not empty, not too long, and of
/// printable ASCII alone, as the tokens of every system are.
fn plain_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= LONGEST_TOKEN
        && token.bytes().all(|byte| byte.is_ascii_graphic())
}

/// A path as the message holds it: as text, or where it isn't Unicode,
/// as the system's own units of it.
fn name(path: &Path) -> Option<serde_json::Value> {
    if let Some(text) = path.to_str() {
        return Some(text.into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Some(path.as_os_str().as_bytes().into())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        Some(path.as_os_str().encode_wide().collect::<Vec<u16>>().into())
    }
    #[cfg(not(any(unix, windows)))]
    None
}

/// A path the message holds, the other way.
fn path(value: &serde_json::Value) -> Option<PathBuf> {
    if let Some(text) = value.as_str() {
        return Some(PathBuf::from(text));
    }
    let units = value.as_array()?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let bytes: Option<Vec<u8>> =
            units.iter().map(|unit| u8::try_from(unit.as_u64()?).ok()).collect();
        Some(std::ffi::OsString::from_vec(bytes?).into())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let wide: Option<Vec<u16>> =
            units.iter().map(|unit| u16::try_from(unit.as_u64()?).ok()).collect();
        Some(std::ffi::OsString::from_wide(&wide?).into())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = units;
        None
    }
}

/// Why a GUI couldn't be handed the files.
#[derive(Debug)]
pub(crate) enum Refused {
    /// The message would be too long.
    TooLong,
    /// Nothing listens, or nothing could be reached in time.
    Unreached,
    /// It didn't answer in time, or not as a GUI does.
    Unanswered,
}

/// Hands `request` to the GUI listening at `address`, trying to reach it
/// until `within` is up, and waiting no longer than that for its answer.
/// Where it is `Ok`, the files are that GUI's; where it isn't, they were
/// never left to it, and it adds none of them.
pub(crate) fn hand_over(
    address: &transport::Address,
    request: &Request,
    within: Duration,
) -> Result<(), Refused> {
    let line = request.encode().ok_or(Refused::TooLong)?;
    let deadline = Instant::now() + within;
    let (sender, receiver) = channel();
    let address = address.clone();
    // on a thread of its own, as reaching a GUI may take longer; it only
    // reads the answer, and gives up as this one does, closing the
    // connection unconfirmed where the answer comes too late
    let exchange = move || {
        let answer = (|| {
            let mut link = transport::connect(&address, deadline)?;
            if Instant::now() >= deadline {
                return Err(Refused::Unreached);
            }
            write_all_by(&mut link, &line, deadline).map_err(|_| Refused::Unreached)?;
            let mut answer = [0; ACCEPTED.len()];
            read_exact_by(&mut link, &mut answer, deadline).map_err(|_| Refused::Unanswered)?;
            if answer == ACCEPTED { Ok(link) } else { Err(Refused::Unanswered) }
        })();
        // given up on, it goes here, and the connection with it
        let _ = sender.send(answer);
    };
    std::thread::Builder::new()
        .name("hand over".into())
        .spawn(exchange)
        .map_err(|_| Refused::Unreached)?;
    let left = deadline.saturating_duration_since(Instant::now());
    let mut link = receiver.recv_timeout(left).unwrap_or(Err(Refused::Unanswered))?;
    // this one decides, and only here: once the byte is written the files
    // are the other GUI's, and while it isn't they are this one's, which
    // the other, waiting a while longer than this, adds none of
    if Instant::now() >= deadline {
        return Err(Refused::Unanswered);
    }
    write_all_by(&mut link, CONFIRMED, deadline).map_err(|_| Refused::Unanswered)
}

/// A connection between two GUIs, which gives up on whatever it is
/// waiting for at a deadline.
pub(crate) trait Link {
    /// Reads what comes into `buf`, as `Read::read` does, waiting no
    /// longer than `deadline`, though taking what has come by then.
    fn read_by(&mut self, buf: &mut [u8], deadline: Instant) -> std::io::Result<usize>;
    /// Writes some of `buf`, as `Write::write` does, waiting no longer
    /// than `deadline`.
    fn write_by(&mut self, buf: &[u8], deadline: Instant) -> std::io::Result<usize>;
}

/// Writes all of `buf` to `link`, by `deadline`.
fn write_all_by(link: &mut dyn Link, mut buf: &[u8], deadline: Instant) -> std::io::Result<()> {
    while !buf.is_empty() {
        match link.write_by(buf, deadline)? {
            0 => return Err(std::io::ErrorKind::WriteZero.into()),
            written => buf = &buf[written..],
        }
        // one that reads a little at a time gets no more time for it
        if !buf.is_empty() && Instant::now() >= deadline {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
    }
    Ok(())
}

/// Fills `buf` from `link`, by `deadline`.
fn read_exact_by(
    link: &mut dyn Link,
    mut buf: &mut [u8],
    deadline: Instant,
) -> std::io::Result<()> {
    while !buf.is_empty() {
        match link.read_by(buf, deadline)? {
            0 => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            read => buf = &mut buf[read..],
        }
        if !buf.is_empty() && Instant::now() >= deadline {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
    }
    Ok(())
}

/// How many GUIs are served at once at most, so one that keeps its
/// connection open holds back no other, and many that do take no more
/// than this; any beyond them are let go of unread.
#[cfg_attr(not(unix), allow(dead_code))]
const AT_ONCE: usize = 4;
/// How long a GUI is served from its connecting, to read its message,
/// answer it and read that it leaves the files: a while longer than it
/// waits itself, from before it connected, so what it says in time is
/// read in time.
const SERVING: Duration = ANSWER.saturating_add(Duration::from_millis(250));

/// What serves a GUI on `link`, by the deadline it is given.
type Serve = dyn Fn(&mut dyn Link, Instant) + Send + Sync;

/// Serves the GUIs that connect, each on a thread of its own, no more
/// than [`AT_ONCE`] at once, and each by [`SERVING`] after it connected.
#[cfg_attr(not(unix), allow(dead_code))]
struct Workers {
    serve: Box<Serve>,
    /// How many are being served.
    busy: std::sync::atomic::AtomicUsize,
}

/// One GUI being served, which it no longer is as this goes.
#[cfg_attr(not(unix), allow(dead_code))]
struct Busy(Arc<Workers>);

impl Drop for Busy {
    fn drop(&mut self) {
        self.0.busy.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg_attr(not(unix), allow(dead_code))]
impl Workers {
    fn new(serve: impl Fn(&mut dyn Link, Instant) + Send + Sync + 'static) -> Arc<Workers> {
        Arc::new(Workers { serve: Box::new(serve), busy: 0.into() })
    }

    /// Serves `link`, which connected just now, on a thread of its own,
    /// and closes it then; where as many are served as may be, or there
    /// is no thread for it, closes it at once.
    fn take(self: &Arc<Self>, link: impl Link + Send + 'static) {
        let deadline = Instant::now() + SERVING;
        let before = self.busy.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let busy = Busy(self.clone());
        if before >= AT_ONCE {
            return;
        }
        // where there is no thread, both go with the closure unserved
        let _ = std::thread::Builder::new().name("other instance".into()).spawn(move || {
            let mut link = link;
            (busy.0.serve)(&mut link, deadline);
            drop(link);
            drop(busy);
        });
    }
}

/// How long is left until `deadline`, though never nothing, so what has
/// come by then is still taken.
#[cfg(unix)]
fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(1))
}

/// Files another GUI handed over, and whether its window was raised for
/// them already, as on Wayland it is as they come.
#[derive(Debug)]
pub(crate) struct Handed {
    pub(crate) files: Vec<PathBuf>,
    pub(crate) raised: bool,
}

/// What the listening thread shares with the GUI.
#[derive(Default)]
struct Shared {
    /// To draw a frame with, once the GUI has one.
    ctx: OnceLock<egui::Context>,
    /// What raises the window on Wayland, while the window is there.
    #[cfg(all(unix, not(target_os = "macos")))]
    raiser: std::sync::Mutex<Option<crate::activation::Raiser>>,
}

impl Shared {
    /// Raises the window with `request`'s token, where the system needs
    /// one and the GUI can: whether it did.
    fn raise(&self, request: &Request) -> bool {
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(raiser) =
            self.raiser.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut()
        {
            return raiser.raise(request.activation_token.as_deref());
        }
        let _ = request;
        false
    }

    /// Passes on what `request` asks, raising the window first.
    fn deliver(&self, sender: &Sender<Handed>, request: Request) {
        let raised = self.raise(&request);
        let _ = sender.send(Handed { files: request.files, raised });
        if let Some(ctx) = self.ctx.get() {
            ctx.request_repaint();
        }
    }
}

/// Listens for GUIs started while this one keeps the session, on a
/// thread of its own, until the GUI ends.
pub(crate) struct Listener {
    receiver: Receiver<Handed>,
    shared: Arc<Shared>,
    /// Takes the socket away again.
    _bound: transport::Bound,
}

impl Listener {
    /// Listens at `address`.
    pub(crate) fn start(address: &transport::Address) -> std::io::Result<Listener> {
        let (sender, receiver) = channel();
        let shared = Arc::new(Shared::default());
        let serving = shared.clone();
        let bound = transport::listen(address, move |link, deadline| {
            serve(link, deadline, &serving, &sender);
        })?;
        Ok(Listener { receiver, shared, _bound: bound })
    }

    /// Lets the listener wake the GUI as files come.
    pub(crate) fn attach(&self, ctx: &egui::Context) {
        let _ = self.shared.ctx.set(ctx.clone());
    }

    /// Lets the listener raise `window` as files come, where that takes
    /// more than egui does: on Wayland.
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) fn raise(&self, window: &winit::window::Window) {
        if let Some(raiser) = crate::activation::Raiser::new(window) {
            *self.shared.raiser.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(raiser);
        }
    }

    /// Lets go of the window as it closes, which is raised no more.
    pub(crate) fn detach(&self) {
        #[cfg(all(unix, not(target_os = "macos")))]
        self.shared.raiser.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    }

    /// What was handed over since last asked.
    pub(crate) fn poll(&self) -> Vec<Handed> {
        self.receiver.try_iter().collect()
    }
}

/// Reads one GUI's message from `link`, answers it, and passes it on
/// once that GUI says it leaves the files to this one, all by `deadline`;
/// anything else is let go of unanswered.
fn serve(link: &mut dyn Link, deadline: Instant, shared: &Shared, sender: &Sender<Handed>) {
    let Some(line) = read_line(link, deadline) else { return };
    let Some(request) = Request::decode(&line) else { return };
    if write_all_by(link, ACCEPTED, deadline).is_err() {
        return;
    }
    // one that gave up waiting, as it may have just as the answer came,
    // says nothing more, and opens the files itself
    let mut confirmed = [0; CONFIRMED.len()];
    if read_exact_by(link, &mut confirmed, deadline).is_err() || confirmed != CONFIRMED {
        return;
    }
    shared.deliver(sender, request);
}

/// A line from `link`, without its newline: `None` where it ends first,
/// or is longer than [`LONGEST`], or isn't whole by `deadline`.
fn read_line(link: &mut dyn Link, deadline: Instant) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let mut chunk = [0; 16 * 1024];
    loop {
        let read = match link.read_by(&mut chunk, deadline) {
            Ok(0) | Err(_) => return None,
            Ok(read) => read,
        };
        let start = line.len();
        line.extend_from_slice(&chunk[..read]);
        if let Some(end) = line[start..].iter().position(|&byte| byte == b'\n') {
            line.truncate(start + end);
            return (line.len() < LONGEST).then_some(line);
        }
        // one that says a little at a time gets no more time for it
        if line.len() >= LONGEST || Instant::now() >= deadline {
            return None;
        }
    }
}

#[cfg(unix)]
pub(crate) mod transport {
    //! A Unix socket beside the session's lock, in a folder of the user's
    //! own, which the user alone may connect to.

    use std::io::{Read, Write};
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use super::{AGAIN, Link, Refused, Workers, left};

    pub(crate) type Address = PathBuf;

    /// Where the GUI that keeps the session at `session` listens, if its
    /// folder is the user's own: one that isn't may hold another's socket.
    pub(crate) fn address(session: &Path) -> Option<Address> {
        let dir = session.parent()?;
        let _ = std::fs::create_dir_all(dir);
        let owner = std::fs::metadata(dir).ok()?.uid();
        (owner == rustix::process::geteuid().as_raw()).then(|| session.with_extension("sock"))
    }

    /// The socket bound, removed again as the GUI ends, unless another
    /// has been bound in its place.
    pub(crate) struct Bound {
        path: PathBuf,
        id: (u64, u64),
    }

    impl Drop for Bound {
        fn drop(&mut self) {
            if std::fs::symlink_metadata(&self.path).is_ok_and(|meta| identity(&meta) == self.id) {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }

    fn identity(meta: &std::fs::Metadata) -> (u64, u64) {
        (meta.dev(), meta.ino())
    }

    /// Waits out `io` on `stream` until `deadline`, as the socket's
    /// timeout, which is set for each, says.
    fn by<T>(
        stream: &mut UnixStream,
        deadline: Instant,
        mut io: impl FnMut(&mut UnixStream) -> std::io::Result<T>,
    ) -> std::io::Result<T> {
        loop {
            stream.set_read_timeout(Some(left(deadline)))?;
            stream.set_write_timeout(Some(left(deadline)))?;
            match io(stream) {
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(std::io::ErrorKind::TimedOut.into());
                }
                done => return done,
            }
        }
    }

    impl Link for UnixStream {
        fn read_by(&mut self, buf: &mut [u8], deadline: Instant) -> std::io::Result<usize> {
            by(self, deadline, |stream| stream.read(buf))
        }

        fn write_by(&mut self, buf: &[u8], deadline: Instant) -> std::io::Result<usize> {
            by(self, deadline, |stream| stream.write(buf))
        }
    }

    /// Listens at `path`, giving each connection to `serve` as
    /// [`Workers`] do. A socket left there by a GUI that ended without
    /// removing it is removed first: the session's lock says none listens
    /// on it.
    pub(crate) fn listen(
        path: &Address,
        serve: impl Fn(&mut dyn Link, Instant) + Send + Sync + 'static,
    ) -> std::io::Result<Bound> {
        if let Ok(meta) = std::fs::symlink_metadata(path)
            && meta.file_type().is_socket()
        {
            let _ = std::fs::remove_file(path);
        }
        let listener = UnixListener::bind(path)?;
        let bound = Bound { path: path.clone(), id: identity(&std::fs::symlink_metadata(path)?) };
        // the folder's own permissions may let others in; the socket's
        // don't, and a socket that can't be made the user's alone goes
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let workers = Workers::new(serve);
        std::thread::Builder::new().name("other instances".into()).spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => workers.take(stream),
                    // as when files run out, which may pass
                    Err(_) => std::thread::sleep(AGAIN),
                }
            }
        })?;
        Ok(bound)
    }

    /// Connects to the GUI listening at `path`, trying again until
    /// `deadline` while there is no socket yet, or nothing listening on it.
    pub(crate) fn connect(path: &Address, deadline: Instant) -> Result<UnixStream, Refused> {
        loop {
            match UnixStream::connect(path) {
                Ok(stream) => return Ok(stream),
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) && Instant::now() + AGAIN < deadline =>
                {
                    std::thread::sleep(AGAIN);
                }
                Err(_) => return Err(Refused::Unreached),
            }
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) mod transport {
    //! A named pipe named after the session's lock, which is the user's
    //! own: its folder is. Every instance of it is made by the GUI that
    //! keeps the session, the first while no other GUI has one, and there
    //! is always one while it runs, so no other program takes its name.

    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::path::Path;
    use std::time::Instant;

    use windows_sys::Win32::Foundation::{
        ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, ERROR_PIPE_BUSY,
        ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX, ReadFile,
        WriteFile,
    };
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
        PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
        PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
    use windows_sys::core::BOOL;

    use super::{AGAIN, Link, Refused};

    pub(crate) type Address = String;

    /// The pipe the GUI that keeps the session at `session` listens on,
    /// named after where it is, which is the user's own.
    pub(crate) fn address(session: &Path) -> Option<Address> {
        // FNV-1a, which no version of Rust names otherwise
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in session.with_extension("lock").to_string_lossy().bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        Some(format!(r"\\.\pipe\tgradish-{hash:016x}"))
    }

    /// Nothing to take away: the pipe goes with the GUI.
    pub(crate) struct Bound;

    fn wide(name: &str) -> Vec<u16> {
        name.encode_utf16().chain([0]).collect()
    }

    /// An instance of the pipe `name`, the first of them where `first`
    /// says, failing where another program has one.
    fn create(name: &[u16], first: bool) -> std::io::Result<OwnedHandle> {
        let open = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: the name ends in a nul, and no security attributes give
        // the default ones
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open,
                mode,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                64 * 1024,
                0,
                std::ptr::null(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: a handle just made, which nothing else owns
        Ok(unsafe { OwnedHandle::from_raw_handle(handle as _) })
    }

    /// Waits for a GUI to connect to `pipe`.
    fn accept(pipe: &OwnedHandle) -> std::io::Result<()> {
        // SAFETY: the handle is open, and used synchronously
        if unsafe { ConnectNamedPipe(pipe.as_raw_handle() as HANDLE, std::ptr::null_mut()) } != 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        // connected between the pipe's making and the wait
        if err.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) { Ok(()) } else { Err(err) }
    }

    /// Listens on the pipe `name`, giving each connection to `serve` on a
    /// thread of its own, as one that says nothing never ends.
    pub(crate) fn listen(
        name: &Address,
        serve: impl Fn(&mut dyn Link, Instant) + Send + Sync + 'static,
    ) -> std::io::Result<Bound> {
        let serve = move |pipe: &mut std::fs::File| serve(pipe, Instant::now() + super::SERVING);
        let name = wide(name);
        let mut pipe = create(&name, true)?;
        let serve = std::sync::Arc::new(serve);
        std::thread::Builder::new().name("other instances".into()).spawn(move || {
            loop {
                if accept(&pipe).is_err() {
                    // as when one connected and left already
                    // SAFETY: the handle is open
                    unsafe { DisconnectNamedPipe(pipe.as_raw_handle() as HANDLE) };
                    std::thread::sleep(AGAIN);
                    continue;
                }
                // the next instance before this one is let go of, so there
                // is always one
                let Ok(next) = create(&name, false) else {
                    let mut connected = std::fs::File::from(pipe);
                    answer(&mut connected, &*serve);
                    pipe = connected.into();
                    // SAFETY: the handle is open
                    unsafe { DisconnectNamedPipe(pipe.as_raw_handle() as HANDLE) };
                    continue;
                };
                let mut connected = std::fs::File::from(std::mem::replace(&mut pipe, next));
                let serve = serve.clone();
                let _ = std::thread::Builder::new()
                    .name("other instance".into())
                    .spawn(move || answer(&mut connected, &*serve));
            }
        })?;
        Ok(Bound)
    }

    /// Has `serve` read and answer the GUI connected to `pipe`, and waits
    /// for that GUI to read the answer, which closing the pipe would
    /// otherwise take from it.
    fn answer(pipe: &mut std::fs::File, serve: &dyn Fn(&mut std::fs::File)) {
        serve(pipe);
        let _ = pipe.sync_all();
    }

    /// A pipe used synchronously, which waits as long as it takes.
    impl Link for std::fs::File {
        fn read_by(&mut self, buf: &mut [u8], _: Instant) -> std::io::Result<usize> {
            std::io::Read::read(self, buf)
        }

        fn write_by(&mut self, buf: &[u8], _: Instant) -> std::io::Result<usize> {
            std::io::Write::write(self, buf)
        }
    }

    /// An end of a pipe opened for overlapped I/O, which is waited for as
    /// long as it may be and cancelled then.
    pub(crate) struct Pipe {
        handle: OwnedHandle,
        /// What each operation signals as it ends.
        event: OwnedHandle,
    }

    impl Pipe {
        /// The end `handle`, which was opened for overlapped I/O.
        pub(crate) fn new(handle: OwnedHandle) -> std::io::Result<Pipe> {
            // SAFETY: no attributes or name give an event of the process's
            // own, reset by hand
            let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            if event.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: an event just made, which nothing else owns
            let event = unsafe { OwnedHandle::from_raw_handle(event as _) };
            Ok(Pipe { handle, event })
        }

        fn raw(&self) -> HANDLE {
            self.handle.as_raw_handle() as HANDLE
        }

        /// Runs `start` with what it starts an operation with, and waits
        /// for that until `deadline`, or as long as it takes where there
        /// is none, cancelling it then: what it transferred.
        pub(crate) fn run(
            &self,
            deadline: Option<Instant>,
            start: impl FnOnce(*mut OVERLAPPED) -> BOOL,
        ) -> std::io::Result<u32> {
            let mut overlapped = OVERLAPPED {
                hEvent: self.event.as_raw_handle() as HANDLE,
                ..OVERLAPPED::default()
            };
            let overlapped = &raw mut overlapped;
            if start(overlapped) == 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                    return Err(err);
                }
            }
            let wait = deadline.map_or(INFINITE, |deadline| {
                let left = deadline.saturating_duration_since(Instant::now()).as_millis();
                u32::try_from(left).unwrap_or(INFINITE - 1).min(INFINITE - 1)
            });
            // SAFETY: the event is open
            if unsafe { WaitForSingleObject(self.event.as_raw_handle() as HANDLE, wait) }
                != WAIT_OBJECT_0
            {
                // SAFETY: the handle is open, and the operation is waited
                // out next, done or cancelled
                unsafe { CancelIoEx(self.raw(), overlapped) };
            }
            let mut transferred = 0;
            // SAFETY: waits until the operation is over, after which
            // nothing is written to `overlapped` or what it was given
            if unsafe { GetOverlappedResult(self.raw(), overlapped, &mut transferred, 1) } == 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32) {
                    return Err(std::io::ErrorKind::TimedOut.into());
                }
                return Err(err);
            }
            Ok(transferred)
        }
    }

    /// The most an operation is given at once.
    fn length(buf: &[u8]) -> u32 {
        u32::try_from(buf.len()).unwrap_or(u32::MAX)
    }

    impl Link for Pipe {
        fn read_by(&mut self, buf: &mut [u8], deadline: Instant) -> std::io::Result<usize> {
            let handle = self.raw();
            let (data, length) = (buf.as_mut_ptr(), length(buf));
            // SAFETY: the buffer outlives the operation, which `run` waits out
            let read = self.run(Some(deadline), |overlapped| unsafe {
                ReadFile(handle, data, length, std::ptr::null_mut(), overlapped)
            });
            match read {
                // the other end closed it
                Err(err) if err.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => Ok(0),
                read => Ok(read? as usize),
            }
        }

        fn write_by(&mut self, buf: &[u8], deadline: Instant) -> std::io::Result<usize> {
            let handle = self.raw();
            let (data, length) = (buf.as_ptr(), length(buf));
            // SAFETY: the buffer outlives the operation, which `run` waits out
            let written = self.run(Some(deadline), |overlapped| unsafe {
                WriteFile(handle, data, length, std::ptr::null_mut(), overlapped)
            });
            Ok(written? as usize)
        }
    }

    /// Connects to the GUI listening on the pipe `name`, trying again until
    /// `deadline` while there is none yet or every instance is busy, and
    /// lets it bring its window forward, which Windows lets the program
    /// started last do.
    pub(crate) fn connect(name: &Address, deadline: Instant) -> Result<Pipe, Refused> {
        use std::os::windows::fs::OpenOptionsExt;
        loop {
            let opened = std::fs::File::options()
                .read(true)
                .write(true)
                .custom_flags(FILE_FLAG_OVERLAPPED)
                .open(name);
            match opened {
                Ok(pipe) => {
                    let pipe = Pipe::new(pipe.into()).map_err(|_| Refused::Unreached)?;
                    let mut server = 0;
                    // SAFETY: the handle is open, and the id written where it says
                    if unsafe { GetNamedPipeServerProcessId(pipe.raw(), &mut server) } != 0 {
                        // SAFETY: any process id may be named
                        unsafe { AllowSetForegroundWindow(server) };
                    }
                    return Ok(pipe);
                }
                Err(err)
                    if (err.kind() == std::io::ErrorKind::NotFound
                        || err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32))
                        && Instant::now() + AGAIN < deadline =>
                {
                    std::thread::sleep(AGAIN);
                }
                Err(_) => return Err(Refused::Unreached),
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) mod transport {
    //! Nowhere to listen.

    use std::path::Path;
    use std::time::Instant;

    use super::Refused;

    pub(crate) type Address = std::path::PathBuf;
    pub(crate) struct Bound;

    pub(crate) fn address(_: &Path) -> Option<Address> {
        None
    }

    pub(crate) fn listen(
        _: &Address,
        _: impl Fn(&mut std::fs::File) + Send + 'static,
    ) -> std::io::Result<Bound> {
        Err(std::io::ErrorKind::Unsupported.into())
    }

    /// No connection, which is never made.
    pub(crate) enum Never {}

    impl super::Link for Never {
        fn read_by(&mut self, _: &mut [u8], _: Instant) -> std::io::Result<usize> {
            match *self {}
        }

        fn write_by(&mut self, _: &[u8], _: Instant) -> std::io::Result<usize> {
            match *self {}
        }
    }

    pub(crate) fn connect(_: &Address, _: Instant) -> Result<Never, Refused> {
        Err(Refused::Unreached)
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;

    fn session(dir: &Path) -> PathBuf {
        dir.join("state").join("session.json")
    }

    /// Starts as the first GUI of the session at `path`, which listens.
    fn first(path: &Path) -> (Store, Listener) {
        match start(path.to_path_buf(), &Request::default()) {
            Start::First { store, listener: Some(listener) } => (store, listener),
            Start::First { listener: None, .. } => panic!("not listening"),
            _ => panic!("not the first"),
        }
    }

    /// What `listener` is handed, as it comes a moment after the answer.
    fn handed(listener: &Listener) -> Vec<Handed> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let handed = listener.poll();
            if !handed.is_empty() || Instant::now() > deadline {
                return handed;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// All that comes over `link` until it is closed, or a while is up.
    fn answer(link: &mut impl Link) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut answer = Vec::new();
        let mut chunk = [0; 64];
        while let Ok(read @ 1..) = link.read_by(&mut chunk, deadline) {
            answer.extend_from_slice(&chunk[..read]);
        }
        answer
    }

    fn files(dir: &Path) -> Request {
        Request {
            files: vec![dir.join("a.gif"), dir.join("frames")],
            activation_token: Some("token-1_2.3".into()),
            startup_id: None,
        }
    }

    #[test]
    fn hands_the_files_to_the_gui_that_keeps_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let (store, listener) = first(&path);
        let request = files(dir.path());
        assert!(matches!(start(path.clone(), &request), Start::HandedOver));
        let handed = handed(&listener);
        assert_eq!(handed.len(), 1);
        assert_eq!(handed[0].files, request.files);
        // there is no window to raise with its token
        assert!(!handed[0].raised);
        // and once that GUI has ended, the next keeps the session, and on
        // Unix listens: a pipe goes only as the process that made it ends
        drop((store, listener));
        let started = start(path, &Request::default());
        assert!(matches!(started, Start::First { .. }));
        #[cfg(unix)]
        assert!(matches!(started, Start::First { listener: Some(_), .. }));
    }

    #[cfg(unix)]
    #[test]
    fn takes_over_from_a_gui_that_ended_without_tidying_up() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        // the socket of one that crashed, which nothing listens on
        let address = transport::address(&path).unwrap();
        drop(std::os::unix::net::UnixListener::bind(&address).unwrap());
        assert!(address.exists());
        let (_store, listener) = first(&path);
        let mode = std::fs::metadata(&address).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let request = files(dir.path());
        assert!(matches!(start(path.clone(), &request), Start::HandedOver));
        assert_eq!(handed(&listener)[0].files, request.files);
        // and takes its own away as it ends
        drop(listener);
        assert!(!address.exists());
    }

    #[test]
    fn opens_beside_a_gui_that_does_not_answer() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let _store = Store::open(path.clone()).unwrap();
        let address = transport::address(&path).unwrap();
        // listening, but never reading or answering
        let _bound = transport::listen(&address, |_, _| {
            loop {
                std::thread::park();
            }
        })
        .unwrap();
        let started = Instant::now();
        assert!(matches!(start(path, &files(dir.path())), Start::Second));
        assert!(started.elapsed() < ANSWER * 3, "{:?}", started.elapsed());
    }

    #[test]
    fn opens_beside_a_gui_that_answers_too_late_which_then_takes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let _store = Store::open(path.clone()).unwrap();
        let address = transport::address(&path).unwrap();
        let (sender, receiver) = channel();
        let shared = Arc::new(Shared::default());
        // one that serves each GUI only once that GUI has given up waiting
        let _bound = transport::listen(&address, move |link, _| {
            std::thread::sleep(ANSWER + ANSWER / 2);
            serve(link, Instant::now() + SERVING, &shared, &sender);
        })
        .unwrap();
        let started = Instant::now();
        assert!(matches!(start(path, &files(dir.path())), Start::Second));
        assert!(started.elapsed() < ANSWER * 3 / 2, "{:?}", started.elapsed());
        // the files are the second GUI's alone
        std::thread::sleep(ANSWER * 2);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn takes_nothing_from_a_gui_that_does_not_leave_it_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let (_store, listener) = first(&path);
        let address = transport::address(&path).unwrap();
        let line = files(dir.path()).encode().unwrap();
        let mut answer = [0; ACCEPTED.len()];
        // one that has the answer, but goes without a word, as one that
        // gave up just then does, and one that says something else
        for last in [&b""[..], b"?"] {
            let deadline = Instant::now() + ANSWER;
            let mut link = transport::connect(&address, deadline).unwrap();
            write_all_by(&mut link, &line, deadline).unwrap();
            read_exact_by(&mut link, &mut answer, deadline).unwrap();
            assert_eq!(answer, ACCEPTED);
            write_all_by(&mut link, last, deadline).unwrap();
            drop(link);
            std::thread::sleep(ANSWER / 2);
            assert!(listener.poll().is_empty());
        }
    }

    #[test]
    fn serves_others_beside_one_that_trickles_and_lets_it_go_in_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let (_store, listener) = first(&path);
        let address = transport::address(&path).unwrap();
        let trickling = std::thread::spawn(move || {
            let started = Instant::now();
            let mut link = transport::connect(&address, started + ANSWER).unwrap();
            // a byte at a time, never a whole line, for as long as it is let
            while started.elapsed() < ANSWER * 5 {
                if write_all_by(&mut link, b" ", Instant::now() + ANSWER).is_err() {
                    break;
                }
                std::thread::sleep(ANSWER / 10);
            }
            started.elapsed()
        });
        std::thread::sleep(ANSWER / 4);
        let request = files(dir.path());
        let started = Instant::now();
        assert!(matches!(start(path, &request), Start::HandedOver));
        assert!(started.elapsed() < ANSWER / 2, "{:?}", started.elapsed());
        assert_eq!(handed(&listener)[0].files, request.files);
        // and the one that trickles is let go of once its time is up
        let trickled = trickling.join().unwrap();
        assert!(trickled < ANSWER * 2, "{trickled:?}");
    }

    #[cfg(unix)]
    #[test]
    fn lets_go_of_more_than_it_serves_at_once_and_of_those_in_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let (_store, listener) = first(&path);
        let address = transport::address(&path).unwrap();
        let connect = || transport::connect(&address, Instant::now() + ANSWER).unwrap();
        // as many as are served at once, which say nothing
        let silent: Vec<_> = (0..AT_ONCE).map(|_| connect()).collect();
        std::thread::sleep(ANSWER / 10);
        let started = Instant::now();
        assert!(answer(&mut connect()).is_empty());
        assert!(started.elapsed() < ANSWER / 2, "{:?}", started.elapsed());
        // and once their time is up, they go, and the next is served
        for mut link in silent {
            assert!(answer(&mut link).is_empty());
        }
        assert!(started.elapsed() < SERVING + ANSWER / 2, "{:?}", started.elapsed());
        let request = files(dir.path());
        assert!(matches!(start(path, &request), Start::HandedOver));
        assert_eq!(handed(&listener)[0].files, request.files);
    }

    #[test]
    fn opens_beside_a_gui_that_does_not_listen() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        // as one of a tgradish from before GUIs listened
        let _store = Store::open(path.clone()).unwrap();
        let started = Instant::now();
        assert!(matches!(start(path, &files(dir.path())), Start::Second));
        assert!(started.elapsed() < ANSWER * 3, "{:?}", started.elapsed());
    }

    #[test]
    fn ignores_what_is_not_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let (_store, listener) = first(&path);
        let address = transport::address(&path).unwrap();
        let relative = [br#"{"version":2,"files":["a.gif"]}"#.as_slice(), b"\n"].concat();
        let long = vec![b'a'; LONGEST + 1];
        let garbage: [&[u8]; 4] = [b"not a message\n", &relative, b"{}\n", &long];
        for garbage in garbage {
            let mut link = transport::connect(&address, Instant::now() + ANSWER).unwrap();
            let _ = write_all_by(&mut link, garbage, Instant::now() + ANSWER);
            let answer = answer(&mut link);
            assert!(answer.is_empty(), "{:?}", String::from_utf8_lossy(&answer));
        }
        // and one that ends before its line does, as a pipe can't
        #[cfg(unix)]
        {
            let mut stream = transport::connect(&address, Instant::now() + ANSWER).unwrap();
            std::io::Write::write_all(&mut stream, &relative[..relative.len() - 1]).unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let answer = answer(&mut stream);
            assert!(answer.is_empty(), "{:?}", String::from_utf8_lossy(&answer));
        }
        assert!(listener.poll().is_empty());
        // and what comes next is taken as ever
        let request = files(dir.path());
        assert!(matches!(start(path, &request), Start::HandedOver));
        let handed = handed(&listener);
        assert_eq!(handed.len(), 1);
        assert_eq!(handed[0].files, request.files);
    }

    #[test]
    fn reads_only_the_files_and_tokens_of_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let request = files(dir.path());
        let line = request.encode().unwrap();
        assert!(line.ends_with(b"\n") && !line[..line.len() - 1].contains(&b'\n'));
        assert_eq!(Request::decode(&line[..line.len() - 1]), Some(request.clone()));
        let message = |text: &str| Request::decode(text.as_bytes());
        let absolute = serde_json::to_string(&request.files[0]).unwrap();
        // what a newer tgradish may say as well is no matter
        let newer = format!(r#"{{"version":2,"files":[{absolute}],"run":"rm -rf"}}"#);
        assert_eq!(message(&newer).unwrap().files, &request.files[..1]);
        for unread in [
            format!(r#"{{"version":3,"files":[{absolute}]}}"#),
            format!(r#"{{"version":1,"files":[{absolute}]}}"#),
            format!(r#"{{"files":[{absolute}]}}"#),
            r#"{"version":2,"files":["a.gif"]}"#.to_owned(),
            format!(r#"{{"version":2,"files":{absolute}}}"#),
            format!(r#"{{"version":2,"files":[{absolute}],"xdg-activation-token":"a\nb"}}"#),
            format!(r#"{{"version":2,"files":[{absolute}],"xdg-activation-token":7}}"#),
            r#"{"version":2,"files":[[1,300]]}"#.to_owned(),
            "[]".to_owned(),
        ] {
            assert_eq!(message(&unread), None, "{unread}");
        }
        assert!(!plain_token("") && !plain_token("a b") && !plain_token(&"a".repeat(2000)));
    }

    #[test]
    fn hands_over_no_more_than_a_gui_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = session(dir.path());
        let (_store, listener) = first(&path);
        let many = (0..LONGEST / 64).map(|n| dir.path().join(format!("{n:064}"))).collect();
        let request = Request { files: many, ..Request::default() };
        let address = transport::address(&path).unwrap();
        assert!(matches!(hand_over(&address, &request, ANSWER), Err(Refused::TooLong)));
        assert!(listener.poll().is_empty());
    }

    // only Unix lets a file be named with bytes that aren't UTF-8
    #[cfg(unix)]
    #[test]
    fn hands_over_a_name_that_is_not_unicode() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join(std::ffi::OsStr::from_bytes(b"odd\xff.png"));
        let request = Request { files: vec![odd], ..Request::default() };
        let line = request.encode().unwrap();
        assert_eq!(Request::decode(&line[..line.len() - 1]), Some(request));
    }
}
