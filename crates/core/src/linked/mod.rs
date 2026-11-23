//! Built-in ffmpeg backend linked directly into the binary (enabled by the
//! `linked` feature). Mirrors the external process backend in
//! [`crate::ffmpeg`]: decoding, filtering, `libvpx-vp9` encoding, WebM muxing
//! and SSIM scoring.
// libvpx two-pass statistics and display matrices are not wrapped by
// ffmpeg-next, so a few raw pointer calls are required.
#![allow(unsafe_code)]

mod reader;

use std::ffi::{CStr, CString};
use std::path::Path;
use std::sync::OnceLock;

use ff::{Dictionary, Packet, Rational, codec, filter, format, frame};
use ffmpeg_next as ff;

use self::reader::Reader;
use crate::CancelToken;
use crate::backend::{Pass, pass_log_file};
use crate::config::Threads;
use crate::error::{Error, FileAction, Result};
use crate::ffmpeg::{Capabilities, Output, Probe};
use crate::ffmpeg::{Facts, Orientation, frames_filter, picture_filter, read_seconds};
use crate::ffmpeg::{ssim_graph, video_filter};
use crate::webm::{Params, Rate};
use crate::webm::{Plan, frame_count};

/// Turns an ffmpeg error into ours, saying what was being done.
fn libav(doing: &'static str) -> impl Fn(ff::Error) -> Error {
    move |err| Error::Libav(format!("{doing}: {err}"))
}

/// Converts `path` into a NUL-terminated path for `libavformat`: on Unix, raw
/// bytes as passed on the ffmpeg command line (so non-Unicode paths work too,
/// whereas `ffmpeg-next`'s path functions panic on non-Unicode); elsewhere,
/// UTF-8, which ffmpeg converts to the system encoding.
fn c_path(path: &Path) -> std::result::Result<CString, &'static str> {
    #[cfg(unix)]
    let bytes = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str());
    #[cfg(not(unix))]
    let bytes = path
        .to_str()
        .ok_or("path is not valid UTF-8 (required by the built-in ffmpeg)")?
        .as_bytes();
    CString::new(bytes).map_err(|_| "path contains a NUL byte")
}

/// Opens a WebM output context at `path` (like
/// `ffmpeg_next::format::output_as`,
/// but accepting any path handled by [`c_path`]).
fn create_webm(path: &Path) -> Result<format::context::Output> {
    let failed = libav("creating the output");
    let name =
        c_path(path).map_err(|why| Error::Libav(format!("creating {}: {why}", path.display())))?;
    // SAFETY: the calls ffmpeg-next's `output_as` makes, with
    // NUL-terminated strings that outlive them. The context is freed here
    // when its file can't be opened, and otherwise by the wrapper, which
    // closes the file too
    unsafe {
        let mut context = std::ptr::null_mut();
        let made = ff::ffi::avformat_alloc_output_context2(
            &mut context,
            std::ptr::null_mut(),
            c"webm".as_ptr(),
            name.as_ptr(),
        );
        if made < 0 {
            return Err(failed(ff::Error::from(made)));
        }
        let opened =
            ff::ffi::avio_open(&mut (*context).pb, name.as_ptr(), ff::ffi::AVIO_FLAG_WRITE);
        if opened < 0 {
            ff::ffi::avformat_free_context(context);
            return Err(failed(ff::Error::from(opened)));
        }
        Ok(format::context::Output::wrap(context))
    }
}

fn init() -> Result<()> {
    static INIT: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| {
        ff::init().map_err(|err| err.to_string())?;
        // the library would print warnings straight to stderr
        ff::util::log::set_level(ff::util::log::Level::Error);
        Ok(())
    })
    .clone()
    .map_err(Error::Libav)
}

pub(crate) fn capabilities() -> Result<Capabilities> {
    init()?;
    // SAFETY: returns a static string
    let version = unsafe { CStr::from_ptr(ff::ffi::av_version_info()) }.to_string_lossy();
    Ok(Capabilities {
        version: format!("ffmpeg version {version} (built in)"),
        libvpx_vp9: ff::encoder::find_by_name("libvpx-vp9").is_some(),
    })
}

/// Orientation from the display matrix of `stream`.
fn orientation(stream: &format::stream::Stream) -> Orientation {
    // SAFETY: the parameters belong to the open stream; side data is
    // checked for presence and size before reading
    unsafe {
        let par = stream.parameters().as_ptr();
        let data = ff::ffi::av_packet_side_data_get(
            (*par).coded_side_data,
            (*par).nb_coded_side_data,
            ff::ffi::AVPacketSideDataType::AV_PKT_DATA_DISPLAYMATRIX,
        );
        if data.is_null() || (*data).size < 9 * 4 {
            return Orientation::Normal;
        }
        let matrix = std::ptr::read_unaligned((*data).data as *const [i32; 9]);
        Orientation::from_display_matrix(&matrix)
    }
}

fn seconds(value: i64, time_base: Rational) -> Option<f64> {
    (value > 0 && time_base.denominator() > 0).then(|| {
        value as f64 * f64::from(time_base.numerator()) / f64::from(time_base.denominator())
    })
}

fn ratio(value: Rational) -> Option<f64> {
    (value.numerator() > 0 && value.denominator() > 0).then(|| f64::from(value))
}

pub(crate) fn probe(input: &Path, cancel: &CancelToken) -> Result<Probe> {
    init()?;
    let source = Reader::open_file(input, None, Threads::Auto, cancel)?;
    let (context, stream, decoder) = (source.input(), source.stream(), source.decoder());
    // like ffprobe: the stream's aspect ratio wins over the codec's
    // SAFETY: reading a plain field of the open stream
    let stream_sar = unsafe { Rational::from((*stream.as_ptr()).sample_aspect_ratio) };
    // where the video starts in the file, which other streams can start
    // before
    let start = {
        // SAFETY: reading a plain field of the open context
        let file_start = unsafe { (*context.as_ptr()).start_time };
        let video_start = stream.start_time();
        if file_start == ff::ffi::AV_NOPTS_VALUE || video_start == ff::ffi::AV_NOPTS_VALUE {
            0.0
        } else {
            let tb = stream.time_base();
            let video =
                video_start as f64 * f64::from(tb.numerator()) / f64::from(tb.denominator().max(1));
            (video - file_start as f64 / f64::from(ff::ffi::AV_TIME_BASE)).max(0.0)
        }
    };
    let mut facts = Facts {
        format: context.format().name().to_string(),
        codec: stream.parameters().id().name().to_string(),
        pix_fmt: decoder.format().descriptor().map(|d| d.name().to_string()).unwrap_or_default(),
        width: f64::from(decoder.width()),
        height: f64::from(decoder.height()),
        sample_aspect: ratio(stream_sar).or_else(|| ratio(decoder.aspect_ratio())),
        orientation: orientation(&stream),
        avg_fps: ratio(stream.avg_frame_rate()),
        base_fps: ratio(stream.rate()),
        durations: (
            seconds(stream.duration(), stream.time_base()),
            seconds(context.duration(), Rational::new(1, ff::ffi::AV_TIME_BASE)),
        ),
        start,
        webm_alpha: stream
            .metadata()
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("alpha_mode") && value == "1"),
        // libavformat says 0 when it doesn't know
        frames: u64::try_from(stream.frames()).ok().filter(|&frames| frames > 0),
        timing: None,
    };
    if facts.untimed() {
        facts.timing = Reader::open_file(input, None, Threads::Auto, cancel)?.timing()?;
    }
    facts.probe(input)
}

pub(crate) fn frame_starts(input: &Path, cancel: &CancelToken) -> Result<Vec<f64>> {
    init()?;
    Reader::open_file(input, None, Threads::Auto, cancel)?.frame_starts()
}

/// Builds a libavfilter graph connecting `buffer` sources named by `inputs` to
/// a `buffersink` named `"out"`, using up to `threads`. `spec` connects them
/// via `[name]` and `[out]`.
fn filter_graph(inputs: &[(&str, String)], spec: &str, threads: Threads) -> Result<filter::Graph> {
    let mut graph = filter::Graph::new();
    if let Some(threads) = threads.limit() {
        // SAFETY: a plain field of the graph, which is set before any
        // filter is added, as libavfilter asks
        unsafe { (*graph.as_mut_ptr()).nb_threads = threads.get().try_into().unwrap_or(i32::MAX) };
    }
    let buffer = filter::find("buffer").expect("buffer filter is always built");
    let sink = filter::find("buffersink").expect("buffersink filter is always built");
    for (name, args) in inputs {
        graph.add(&buffer, name, args).map_err(libav("creating a filter source"))?;
    }
    graph.add(&sink, "out", "").map_err(libav("creating a filter sink"))?;
    let mut parser = graph.input("out", 0).map_err(libav("building filters"))?;
    for (name, _) in inputs {
        parser = parser.output(name, 0).map_err(libav("building filters"))?;
    }
    parser.parse(spec).map_err(|err| Error::Libav(format!("filters {spec:?}: {err}")))?;
    graph.validate().map_err(libav("configuring filters"))?;
    Ok(graph)
}

/// Pulls frames from a [`Reader`] through a filter graph (`[in]` -> `[out]`).
struct Filtered {
    source: Reader,
    graph: filter::Graph,
    /// Next decoded frame buffered for the filter source.
    next: frame::Video,
    input_open: bool,
}

impl Filtered {
    /// Creates a [`Filtered`] stream using the first decoded frame to configure
    /// the filter input. Returns `Ok(None)` if `source` yields no frames.
    fn new(mut source: Reader, spec: &str, threads: Threads) -> Result<Option<Filtered>> {
        let mut next = frame::Video::empty();
        if !source.next(&mut next)? {
            return Ok(None);
        }
        let graph = filter_graph(&[("in", source.buffer_args(&next))], spec, threads)?;
        Ok(Some(Filtered { source, graph, next, input_open: true }))
    }

    fn time_base(&mut self) -> Rational {
        self.graph.get("out").expect("added").sink().time_base()
    }

    /// Reads the next filtered frame into `frame`, returning false on EOF.
    fn next(&mut self, frame: &mut frame::Video, cancel: &CancelToken) -> Result<bool> {
        loop {
            match self.graph.get("out").expect("added").sink().frame(frame) {
                Ok(()) => return Ok(true),
                Err(ff::Error::Eof) => return Ok(false),
                Err(err) if is_again(&err) && !self.input_open => return Ok(false),
                Err(err) if is_again(&err) => {}
                Err(err) => return Err(libav("filtering")(err)),
            }
            cancel.check()?;
            let mut input = self.graph.get("in").expect("added");
            let mut input = input.source();
            input.add(&self.next).map_err(libav("filtering"))?;
            if !self.source.next(&mut self.next)? {
                input.flush().map_err(libav("filtering"))?;
                self.input_open = false;
            }
        }
    }
}

/// The pixels of an RGBA `frame` `width` wide, without the padding its
/// rows can have.
fn rgba(frame: &frame::Video, width: u32, height: u32) -> Vec<u8> {
    let (stride, data, row) = (frame.stride(0), frame.data(0), width as usize * 4);
    let mut pixels = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        pixels.extend_from_slice(&data[y * stride..][..row]);
    }
    pixels
}

fn is_again(err: &ff::Error) -> bool {
    matches!(err, ff::Error::Other { errno } if *errno == ff::util::error::EAGAIN)
}

/// A string allocated by ffmpeg's allocator, freed on drop.
struct AvString(*mut std::ffi::c_char);

impl AvString {
    fn new(text: &CStr) -> AvString {
        // SAFETY: av_strdup copies a NUL-terminated string
        AvString(unsafe { ff::ffi::av_strdup(text.as_ptr()) })
    }
}

impl Drop for AvString {
    fn drop(&mut self) {
        // SAFETY: allocated by av_strdup in new; av_free accepts null
        unsafe { ff::ffi::av_free(self.0.cast()) }
    }
}

/// Opens `encoder`, failing on options libavcodec did not use, as the ffmpeg
/// command line does. ffmpeg-next's `open_with` drops them silently.
fn open_encoder(
    mut encoder: ff::encoder::video::Video,
    options: Dictionary,
) -> Result<ff::encoder::Video> {
    // SAFETY: the same calls as ffmpeg-next's `open_with`, keeping the
    // dictionary libavcodec hands back with the options it didn't use
    let (result, unused) = unsafe {
        let mut options = options.disown();
        let result = ff::ffi::avcodec_open2(encoder.as_mut_ptr(), std::ptr::null(), &mut options);
        (result, Dictionary::own(options))
    };
    if result < 0 {
        return Err(libav("opening libvpx-vp9")(ff::Error::from(result)));
    }
    let unused: Vec<_> = unused.iter().map(|(name, _)| name.to_string()).collect();
    if !unused.is_empty() {
        let reason = format!("has options libvpx-vp9 does not know: {}", unused.join(", "));
        return Err(Error::invalid("encoder-options", reason));
    }
    Ok(ff::encoder::video::Encoder(encoder))
}

/// The options libvpx-vp9 is opened with, as the ffmpeg command lines
/// give them: tgradish's own, then `encoder_options` over them.
fn encoder_options(plan: &Plan, params: &Params) -> Dictionary<'static> {
    let mut options = Dictionary::new();
    // automatic as the ffmpeg command line has it; the library default is
    // one thread
    let threads = plan.threads.limit().map_or("auto".to_owned(), |n| n.to_string());
    options.set("threads", &threads);
    options.set("deadline", "good");
    options.set("cpu-used", &plan.effort.cpu_used().to_string());
    options.set("row-mt", "1");
    match params.rate {
        Rate::Bitrate(_) => {}
        Rate::Crf(crf) => options.set("crf", &crf.to_string()),
        Rate::Lossless => options.set("lossless", "1"),
    }
    for (name, value) in &plan.encoder_options {
        options.set(name, value);
    }
    options
}

/// Opened encoder, the output it writes to, and two-pass state.
struct Encoder {
    encoder: ff::encoder::Video,
    time_base: Rational,
    /// One frame in `time_base` units. The muxer adds the last packet's
    /// duration to the header duration, and libvpx leaves it unset.
    frame_duration: i64,
    output: Option<format::context::Output>,
    /// `stats_in` of the encoder. Declared after it, so it is dropped after
    /// the encoder, which only borrows it.
    _stats_in: Option<AvString>,
}

impl Encoder {
    fn open(
        plan: &Plan,
        params: &Params,
        pass: Pass,
        output: Option<&Path>,
        time_base: Rational,
    ) -> Result<Encoder> {
        let codec = ff::encoder::find_by_name("libvpx-vp9")
            .ok_or_else(|| Error::Libav("this ffmpeg build lacks the libvpx-vp9 encoder".into()))?;
        let mut output = output.map(create_webm).transpose()?;

        let mut encoder = codec::context::Context::new_with_codec(codec)
            .encoder()
            .video()
            .map_err(libav("creating the encoder"))?;
        encoder.set_width(plan.width);
        encoder.set_height(plan.height);
        encoder.set_format(if plan.alpha {
            format::Pixel::YUVA420P
        } else {
            format::Pixel::YUV420P
        });
        encoder.set_time_base(time_base);
        encoder.set_frame_rate(Some(Rational::from(params.fps)));

        let options = encoder_options(plan, params);
        match params.rate {
            Rate::Bitrate(kbps) => encoder.set_bit_rate((kbps * 1000.0).round() as usize),
            Rate::Crf(_) => encoder.set_bit_rate(0),
            Rate::Lossless => {}
        }

        let mut flags = codec::Flags::empty();
        if output
            .as_ref()
            .is_some_and(|o| o.format().flags().contains(format::Flags::GLOBAL_HEADER))
        {
            flags |= codec::Flags::GLOBAL_HEADER;
        }
        let mut stats_in = None;
        match pass {
            Pass::Single => {}
            Pass::First(_) => flags |= codec::Flags::PASS1,
            Pass::Second(log) => {
                flags |= codec::Flags::PASS2;
                let file = pass_log_file(log);
                let stats = std::fs::read(&file).map_err(Error::io(FileAction::Read, &file))?;
                let stats = CString::new(stats)
                    .map_err(|_| Error::Libav("invalid first-pass statistics".into()))?;
                let stats = stats_in.insert(AvString::new(&stats));
                // SAFETY: libvpx reads stats_in while opening and encoding.
                // On success the string moves into Encoder after the
                // encoder field, so it is freed after it; on errors it is
                // freed while the context is only being released, which
                // does not read it
                unsafe { (*encoder.as_mut_ptr()).stats_in = stats.0 };
            }
        }
        encoder.set_flags(flags);

        let encoder = open_encoder(encoder, options)?;
        if let Some(output) = &mut output {
            let mut stream = output.add_stream(codec).map_err(libav("adding the stream"))?;
            stream.set_parameters(&encoder);
            stream.set_time_base(time_base);
            let mut metadata = Dictionary::new();
            if plan.watermark {
                // stored as WritingApp by the Matroska muxer
                metadata.set("encoding_tool", crate::TOOL_ID);
            }
            if let Some(title) = &plan.title {
                metadata.set("title", title);
            }
            output.set_metadata(metadata);
            output.write_header().map_err(libav("writing the header"))?;
        }
        let frame_duration = (f64::from(time_base.invert()) / params.fps).round().max(1.0) as i64;
        Ok(Encoder { encoder, time_base, frame_duration, output, _stats_in: stats_in })
    }

    fn send(&mut self, frame: Option<&frame::Video>) -> Result<()> {
        match frame {
            Some(frame) => self.encoder.send_frame(frame),
            None => self.encoder.send_eof(),
        }
        .map_err(libav("encoding"))?;
        let mut packet = Packet::empty();
        loop {
            match self.encoder.receive_packet(&mut packet) {
                Ok(()) => {}
                Err(ff::Error::Eof) => return Ok(()),
                Err(err) if is_again(&err) => return Ok(()),
                Err(err) => return Err(libav("encoding")(err)),
            }
            if let Some(output) = &mut self.output {
                let stream_time_base = output.stream(0).expect("added in open").time_base();
                packet.set_stream(0);
                if packet.duration() == 0 {
                    packet.set_duration(self.frame_duration);
                }
                packet.rescale_ts(self.time_base, stream_time_base);
                packet.write_interleaved(output).map_err(libav("writing the output"))?;
            }
        }
    }

    fn finish(mut self, pass: Pass) -> Result<()> {
        self.send(None)?;
        if let Pass::First(log) = pass {
            // SAFETY: libvpx sets stats_out to a NUL-terminated string when
            // the first pass is flushed
            let stats = unsafe {
                let stats = (*self.encoder.as_ptr()).stats_out;
                if stats.is_null() { Vec::new() } else { CStr::from_ptr(stats).to_bytes().to_vec() }
            };
            let file = pass_log_file(log);
            std::fs::write(&file, stats).map_err(Error::io(FileAction::Write, &file))?;
        }
        if let Some(output) = &mut self.output {
            output.write_trailer().map_err(libav("finishing the output"))?;
        }
        Ok(())
    }
}

pub(crate) fn encode(
    plan: &Plan,
    params: &Params,
    pass: Pass,
    output: Option<&Path>,
    cancel: &CancelToken,
    on_output: &mut dyn FnMut(Output),
) -> Result<()> {
    init()?;
    cancel.check()?;
    let total = frame_count(params.length, params.fps);
    let read = read_seconds(total, params.fps);
    let source = Reader::open(plan, read, cancel)?;
    let pix_fmt = if plan.alpha { "yuva420p" } else { "yuv420p" };
    let chain = video_filter(plan, params.fps, params.length, pix_fmt);
    let spec = format!("[in]{}{chain}[out]", plan.source.orientation.filters());
    let Some(mut filtered) = Filtered::new(source, &spec, plan.threads)? else {
        return Err(Error::Libav(format!("{} has no frames to encode", plan.input.display())));
    };
    let mut encoder = Encoder::open(plan, params, pass, output, filtered.time_base())?;
    let frame_duration = encoder.frame_duration;

    let mut frame = frame::Video::empty();
    let mut sent = 0;
    // fewer when the input is shorter
    while sent < total && filtered.next(&mut frame, cancel)? {
        // as ffmpeg's command line does: a picture type left from decoding
        // the input would make the encoder start a keyframe where the
        // input had one, spending bytes the size limit can't spare
        frame.set_kind(ff::picture::Type::None);
        // SAFETY: setting a plain field of a frame we own
        unsafe { (*frame.as_mut_ptr()).duration = frame_duration };
        encoder.send(Some(&frame))?;
        sent += 1;
        on_output(Output::Time { micros: (sent as f64 / params.fps * 1e6) as u64 });
    }
    encoder.finish(pass)
}

/// Up to `count` frames of `input` from `start` seconds on, at `fps`,
/// scaled to `size`, as straight RGBA, read in as many `threads`.
pub(crate) fn frames(
    input: &Path,
    probe: &Probe,
    (start, fps): (f64, f64),
    (width, height): (u32, u32),
    count: usize,
    threads: Threads,
    cancel: &CancelToken,
) -> Result<Vec<Vec<u8>>> {
    init()?;
    let read = read_seconds(count as u64, fps);
    let source = Reader::open_window(input, probe, (start, read), threads, cancel)?;
    let chain = frames_filter(probe.still_image || count == 1, fps, (width, height));
    let spec = format!("[in]{}{chain},format=rgba[out]", probe.orientation.filters());
    let Some(mut filtered) = Filtered::new(source, &spec, threads)? else { return Ok(Vec::new()) };
    let mut frame = frame::Video::empty();
    let mut out = Vec::new();
    while out.len() < count && filtered.next(&mut frame, cancel)? {
        out.push(rgba(&frame, width, height));
    }
    Ok(out)
}

/// Linked version of [`crate::ffmpeg::still`].
pub(crate) fn still(plan: &Plan, cancel: &CancelToken) -> Result<Vec<u8>> {
    init()?;
    cancel.check()?;
    let length = 1.0 / plan.fps;
    let source = Reader::open(plan, 2.0 * length, cancel)?;
    let no_frame = || Error::NoFrame { path: plan.input.clone(), at: plan.start };
    // the first frame from the start, as it is: resampling to a frame rate
    // can skip a short one
    let chain = picture_filter(plan, crate::presets::Format::Webp, "rgba");
    let spec = format!("[in]{}{chain}[out]", plan.source.orientation.filters());
    let mut filtered = Filtered::new(source, &spec, plan.threads)?.ok_or_else(no_frame)?;
    let mut frame = frame::Video::empty();
    if !filtered.next(&mut frame, cancel)? {
        return Err(no_frame());
    }
    Ok(rgba(&frame, plan.width, plan.height))
}

/// Linked version of [`crate::ffmpeg::ssim`], averaging the per-frame SSIM
/// the filter reports.
pub(crate) fn ssim(plan: &Plan, candidate: &Path, fps: f64, cancel: &CancelToken) -> Result<f64> {
    init()?;
    let frames = frame_count(plan.length, plan.fps);
    let mut sources = [
        Reader::open(plan, read_seconds(frames, plan.fps), cancel)?,
        Reader::open_file(candidate, None, plan.threads, cancel)?,
    ];
    let mut next = [frame::Video::empty(), frame::Video::empty()];
    let mut open = [false; 2];
    for i in 0..2 {
        open[i] = sources[i].next(&mut next[i])?;
    }
    if !open[0] || !open[1] {
        return Err(Error::Libav("nothing to compare".into()));
    }

    // turned upright here, as ffmpeg's command line does itself
    let upright = plan.source.orientation.filters();
    let spec = ssim_graph(plan, fps, ("source", "attempt"), upright, "[out]");
    let inputs = [
        ("source", sources[0].buffer_args(&next[0])),
        ("attempt", sources[1].buffer_args(&next[1])),
    ];
    let mut graph = filter_graph(&inputs, &spec, plan.threads)?;

    let (mut total, mut count) = (0.0, 0u64);
    let mut scored = frame::Video::empty();
    while open[0] || open[1] {
        cancel.check()?;
        // feed whichever input is behind, so the comparison never waits long
        let time = |i: usize| {
            let tb = sources[i].time_base();
            next[i].pts().unwrap_or(0) as f64 * f64::from(tb)
        };
        let i = match open {
            [true, true] => usize::from(time(1) < time(0)),
            [true, false] => 0,
            _ => 1,
        };
        let mut buffer = graph.get(inputs[i].0).expect("added");
        let mut buffer = buffer.source();
        buffer.add(&next[i]).map_err(libav("filtering"))?;
        open[i] = sources[i].next(&mut next[i])?;
        if !open[i] {
            buffer.flush().map_err(libav("filtering"))?;
        }

        loop {
            match graph.get("out").expect("added").sink().frame(&mut scored) {
                Ok(()) => {}
                Err(ff::Error::Eof) => break,
                Err(err) if is_again(&err) => break,
                Err(err) => return Err(libav("filtering")(err)),
            }
            if let Some(value) = scored.metadata().get("lavfi.ssim.All")
                && let Ok(value) = value.parse::<f64>()
            {
                total += value;
                count += 1;
            }
        }
    }
    if count == 0 {
        return Err(Error::Libav("SSIM filter produced no output".into()));
    }
    Ok(total / count as f64)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::input::Input;
    use crate::webm::{Options, Request};

    fn plan(threads: Threads, options: Options) -> Plan {
        let source = Probe {
            format: "mov,mp4".into(),
            codec: "h264".into(),
            width: 640,
            height: 480,
            fps: Some(30.0),
            base_fps: Some(30.0),
            duration: Some(2.0),
            start: 0.0,
            alpha: false,
            still_image: false,
            orientation: Default::default(),
            decoder: None,
        };
        let request = Request { threads, options, ..Request::new(Input::File("in.mp4".into())) };
        crate::webm::plan(&request, source).unwrap().0
    }

    #[test]
    fn limits_its_threads_where_asked() {
        init().unwrap();
        let three = Threads::Limit(NonZeroU32::new(3).unwrap());
        let params = |plan: &Plan| crate::webm::first_params(plan);
        // the encoder's: automatic as the command line has it, else as
        // many as asked, and the user's own over that
        let auto = plan(Threads::Auto, Options::default());
        assert_eq!(encoder_options(&auto, &params(&auto)).get("threads"), Some("auto"));
        let limited = plan(three, Options::default());
        assert_eq!(encoder_options(&limited, &params(&limited)).get("threads"), Some("3"));
        let own = Options {
            encoder_options: Some([("threads".to_owned(), "5".to_owned())].into()),
            ..Options::default()
        };
        let theirs = plan(three, own);
        assert_eq!(encoder_options(&theirs, &params(&theirs)).get("threads"), Some("5"));
        // the filters': the graph's own choice, which goes by the
        // machine's cores, or as many as asked
        let input = [("in", "video_size=8x8:pix_fmt=0:time_base=1/30:pixel_aspect=1/1".into())];
        let threads_of = |threads| {
            let mut graph = filter_graph(&input, "[in]scale=4:4[out]", threads).unwrap();
            // SAFETY: reading a plain field of the graph
            unsafe { (*graph.as_mut_ptr()).nb_threads }
        };
        assert!(threads_of(Threads::Auto) >= 1);
        assert_eq!(threads_of(three), 3);
    }
}
