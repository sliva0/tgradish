//! Conversions, run one at a time on a worker thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use tgradish_core::backend::Backend;
use tgradish_core::convert::{Request, convert, default_output};
use tgradish_core::events::Event;
use tgradish_core::ffmpeg::CancelToken;
use tgradish_core::options::Options;
use tgradish_core::tgs::{self, Preview, TgsEvent, TgsOptions, TgsRequest};

/// What a job converts with, fixed when it starts.
pub enum Plan {
    Webm { options: Options, backend: Backend },
    Tgs { options: TgsOptions },
}

impl Plan {
    /// Where the result for `input` goes: in `dir`, or next to the input.
    pub fn output(&self, input: &Path, dir: Option<&Path>) -> PathBuf {
        let next_to_input = match self {
            Plan::Webm { options, .. } => default_output(input, options.target.unwrap_or_default()),
            Plan::Tgs { options } => tgs::default_output(input, options.target.unwrap_or_default()),
        };
        match (dir, next_to_input.file_name()) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => next_to_input,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Waiting,
    Running {
        stage: String,
        /// How far along the current stage is, when that is known.
        fraction: Option<f32>,
    },
    Done {
        bytes: u64,
        limit: u64,
        lossy: bool,
        /// Problems Telegram would still have, worst first.
        issues: Vec<String>,
    },
    Failed(String),
    Cancelled,
}

enum Message {
    Stage(String, Option<f32>),
    Log(String),
    Done(Result<Done, String>),
}

struct Done {
    bytes: u64,
    limit: u64,
    lossy: bool,
    issues: Vec<String>,
    preview: Option<Preview>,
}

pub struct Job {
    /// One file, or the frames of one sticker.
    pub inputs: Vec<PathBuf>,
    pub sequence: bool,
    pub output: Option<PathBuf>,
    pub status: Status,
    pub log: Vec<String>,
    pub preview: Option<Preview>,
    cancel: CancelToken,
    messages: Option<Receiver<Message>>,
}

fn kib(bytes: u64) -> String {
    format!("{:.1} KiB", bytes as f64 / 1024.0)
}

impl Job {
    pub fn new(inputs: Vec<PathBuf>, sequence: bool) -> Job {
        Job {
            inputs,
            sequence,
            output: None,
            status: Status::Waiting,
            log: Vec::new(),
            preview: None,
            cancel: CancelToken::new(),
            messages: None,
        }
    }

    pub fn name(&self) -> String {
        let first = self
            .inputs
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let first = first.unwrap_or_default();
        match self.inputs.len() {
            1 if self.sequence => format!("{first} (frames)"),
            1 => first,
            n => format!("{first} and {} more, as one sticker", n - 1),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, Status::Running { .. })
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.status, Status::Done { .. } | Status::Failed(_) | Status::Cancelled)
    }

    /// Starts converting on a worker thread, which repaints `ctx` as it goes.
    pub fn start(&mut self, plan: Plan, output: PathBuf, overwrite: bool, ctx: egui::Context) {
        let (sender, receiver) = channel();
        self.messages = Some(receiver);
        self.output = Some(output.clone());
        self.status = Status::Running { stage: "starting".into(), fraction: None };
        let inputs = self.inputs.clone();
        let sequence = self.sequence;
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let send = |message| {
                let _ = sender.send(message);
                ctx.request_repaint();
            };
            let result = run(plan, inputs, sequence, output, overwrite, &cancel, &send);
            send(Message::Done(result));
        });
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Takes in what the worker reported.
    pub fn poll(&mut self) {
        let Some(messages) = &self.messages else { return };
        while let Ok(message) = messages.try_recv() {
            match message {
                Message::Stage(stage, fraction) => {
                    self.status = Status::Running { stage, fraction }
                }
                Message::Log(line) => self.log.push(line),
                Message::Done(Ok(done)) => {
                    self.status = Status::Done {
                        bytes: done.bytes,
                        limit: done.limit,
                        lossy: done.lossy,
                        issues: done.issues,
                    };
                    self.preview = done.preview;
                }
                Message::Done(Err(message)) if message == "cancelled" => {
                    self.status = Status::Cancelled
                }
                Message::Done(Err(message)) => self.status = Status::Failed(message),
            }
        }
        if self.is_finished() {
            self.messages = None;
        }
    }
}

fn run(
    plan: Plan,
    inputs: Vec<PathBuf>,
    sequence: bool,
    output: PathBuf,
    overwrite: bool,
    cancel: &CancelToken,
    send: &dyn Fn(Message),
) -> Result<Done, String> {
    if let Some(dir) = output.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    match plan {
        Plan::Webm { options, backend } => {
            let request = Request {
                input: inputs[0].clone(),
                output: Some(output),
                options,
                overwrite,
                keep_temp: false,
            };
            let mut attempt_name = String::new();
            let outcome = convert(&backend, &request, cancel, &mut |event| match event {
                Event::Started { plan } => send(Message::Log(format!(
                    "{} {}x{}, {:.0} fps, {:.2} s",
                    plan.target.name(),
                    plan.width,
                    plan.height,
                    plan.fps,
                    plan.length
                ))),
                Event::AttemptStarted { attempt, params } => {
                    attempt_name =
                        format!("attempt {attempt}: {:.0} fps, {:?}", params.fps, params.rate);
                    send(Message::Stage(attempt_name.clone(), Some(0.0)));
                }
                Event::Progress { pass, passes, fraction, .. } => {
                    let done = (f64::from(pass - 1) + fraction) / f64::from(passes);
                    send(Message::Stage(attempt_name.clone(), Some(done as f32)));
                }
                Event::AttemptFinished { attempt, bytes, fits, .. } => send(Message::Log(format!(
                    "attempt {attempt}: {}, {}",
                    kib(bytes),
                    if fits { "fits" } else { "too big" }
                ))),
                Event::Scored { attempt, ssim } => {
                    send(Message::Log(format!("attempt {attempt}: similarity {ssim:.4}")))
                }
                Event::Warning { message } => send(Message::Log(format!("warning: {message}"))),
                _ => {}
            })
            .map_err(|err| err.to_string())?;
            send(Message::Stage("loading the preview".into(), None));
            // the sticker is done either way
            let preview = backend.preview(&outcome.output, cancel).map_err(|err| {
                send(Message::Log(format!("no preview: {err}")));
            });
            Ok(Done {
                bytes: outcome.bytes,
                limit: tgradish_core::telegram::MAX_BYTES,
                lossy: false,
                issues: outcome.issues.iter().map(ToString::to_string).collect(),
                preview: preview.ok(),
            })
        }
        Plan::Tgs { options } => {
            let request = TgsRequest { inputs, sequence, output, options, overwrite };
            let mut issues = Vec::new();
            let outcome = tgs::convert(&request, cancel, &mut |event| match event {
                TgsEvent::Started { report, .. } => {
                    let scale = if report.scale > 1 { format!(" at {}x", report.scale) } else { String::new() };
                    send(Message::Log(format!(
                        "{}x{} cells{scale}, {} colours, {} frames, {:.2} s",
                        report.width,
                        report.height,
                        report.colours,
                        report.frames,
                        f64::from(report.ticks) / 60.0
                    )));
                    if let Some(likely) = report.likely_scale {
                        send(Message::Log(format!(
                            "looks like {}x art with some pixels off the grid; a pixel scale of {} snaps them",
                            likely.scale, likely.scale
                        )));
                    }
                    send(Message::Stage("encoding".into(), None));
                }
                TgsEvent::TooLarge { bytes } => {
                    send(Message::Log(format!("about {} losslessly, too large; fitting", kib(bytes as u64))));
                    send(Message::Stage("fitting".into(), None));
                }
                TgsEvent::Reduced { step } => {
                    send(Message::Log(format!("{:?} → about {}", step.reduction, kib(step.bytes as u64))))
                }
                TgsEvent::Packing => send(Message::Stage("compressing".into(), None)),
                TgsEvent::Warning { message } => send(Message::Log(format!("warning: {message}"))),
                TgsEvent::Finished { issues: found, .. } => {
                    issues = found.iter().map(|issue| issue.message.clone()).collect();
                }
                TgsEvent::Error { .. } => {}
            })
            .map_err(|err| err.to_string())?;
            Ok(Done {
                bytes: outcome.bytes,
                limit: 64 * 1024,
                lossy: outcome.lossy,
                issues,
                preview: Some(outcome.preview),
            })
        }
    }
}

/// Opens a file with the system's default program.
pub fn reveal(path: &Path) {
    // the folder, since the result itself may open in something unhelpful
    let target = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    #[cfg(target_os = "windows")]
    let command = std::process::Command::new("explorer").arg(target).spawn();
    #[cfg(target_os = "macos")]
    let command = std::process::Command::new("open").arg(target).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let command = std::process::Command::new("xdg-open").arg(target).spawn();
    let _ = command;
}
