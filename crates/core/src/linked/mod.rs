//! ffmpeg libraries linked into the binary, for builds with the `linked`
//! feature. Does the same as the ffmpeg command lines in [`crate::ffmpeg`]:
//! decode, run the same filter chain, encode with libvpx-vp9 and write WebM.
// libvpx two-pass statistics and display matrices are not wrapped by
// ffmpeg-next, so a few raw pointers are needed
#![allow(unsafe_code)]

mod source;

use std::ffi::{CStr, CString};
use std::path::Path;
use std::sync::OnceLock;

use ff::{Dictionary, Packet, Rational, codec, filter, format, frame};
use ffmpeg_next as ff;

use self::source::Source;
use crate::CancelToken;
use crate::backend::{Pass, pass_log_file};
use crate::convert::{Plan, frame_count};
use crate::error::{Error, Result};
use crate::events::{Params, Rate};
use crate::ffmpeg::{Capabilities, Output, Probe};
use crate::ffmpeg::{Facts, Orientation, picture_filter, video_filter};

/// Turns an ffmpeg error into ours, saying what was being done.
fn libav(doing: &'static str) -> impl Fn(ff::Error) -> Error {
    move |err| Error::Libav(format!("{doing}: {err}"))
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
    let source = Source::open_file(input, None, cancel)?;
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
        facts.timing = Source::open_file(input, None, cancel)?.timing()?;
    }
    facts.probe(input)
}

pub(crate) fn frame_starts(input: &Path, cancel: &CancelToken) -> Result<Vec<f64>> {
    init()?;
    Source::open_file(input, None, cancel)?.frame_starts()
}

/// A filter graph from `buffer` sources named after `inputs` to a
/// `buffersink` named `out`. `spec` refers to them as `[name]` and `[out]`.
fn filter_graph(inputs: &[(&str, String)], spec: &str) -> Result<filter::Graph> {
    let mut graph = filter::Graph::new();
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

/// A source read through a filter graph from `[in]` to `[out]`, one
/// filtered frame at a time.
struct Filtered {
    source: Source,
    graph: filter::Graph,
    /// The next decoded frame, not yet given to the filters.
    next: frame::Video,
    input_open: bool,
}

impl Filtered {
    /// `source` through `spec`, or `None` when it has no frames: the
    /// filters' input takes its format from the first.
    fn new(mut source: Source, spec: &str) -> Result<Option<Filtered>> {
        let mut next = frame::Video::empty();
        if !source.next(&mut next)? {
            return Ok(None);
        }
        let graph = filter_graph(&[("in", source.buffer_args(&next))], spec)?;
        Ok(Some(Filtered { source, graph, next, input_open: true }))
    }

    fn time_base(&mut self) -> Rational {
        self.graph.get("out").expect("added").sink().time_base()
    }

    /// The next filtered frame, into `frame`, decoding as much as that
    /// takes: false once the input is used up and the filters drained.
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
        return Err(Error::InvalidOptions(format!(
            "unknown encoder options: {}",
            unused.join(", ")
        )));
    }
    Ok(ff::encoder::video::Encoder(encoder))
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
            .ok_or_else(|| Error::Libav("this ffmpeg has no libvpx-vp9 encoder".into()))?;
        let mut output = output
            .map(|path| format::output_as(path, "webm"))
            .transpose()
            .map_err(libav("creating the output"))?;

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

        let mut options = Dictionary::new();
        // the ffmpeg command line does this too; the library default is one thread
        options.set("threads", "auto");
        options.set("deadline", "good");
        options.set("cpu-used", &plan.speed.cpu_used().to_string());
        options.set("row-mt", "1");
        match params.rate {
            Rate::Bitrate(kbps) => encoder.set_bit_rate((kbps * 1000.0).round() as usize),
            Rate::Crf(crf) => {
                encoder.set_bit_rate(0);
                options.set("crf", &crf.to_string());
            }
            Rate::Lossless => options.set("lossless", "1"),
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
                let stats = std::fs::read(pass_log_file(log))?;
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

        for (name, value) in &plan.encoder_options {
            options.set(name, value);
        }
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
            std::fs::write(pass_log_file(log), stats)?;
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
    let read = (total + 1) as f64 / params.fps;
    let source = Source::open(plan, read, cancel)?;
    let pix_fmt = if plan.alpha { "yuva420p" } else { "yuv420p" };
    let chain = video_filter(plan, params.fps, params.length, pix_fmt);
    let spec = format!("[in]{}{chain}[out]", plan.source.orientation.filters());
    let Some(mut filtered) = Filtered::new(source, &spec)? else {
        return Err(Error::Libav(format!("{} has no frames to encode", plan.input.display())));
    };
    let mut encoder = Encoder::open(plan, params, pass, output, filtered.time_base())?;
    let frame_duration = encoder.frame_duration;

    let mut frame = frame::Video::empty();
    let mut sent = 0;
    // fewer when the input is shorter
    while sent < total && filtered.next(&mut frame, cancel)? {
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
/// scaled to `size`, as straight RGBA.
pub(crate) fn frames(
    input: &Path,
    probe: &Probe,
    (start, fps): (f64, f64),
    (width, height): (u32, u32),
    count: usize,
    cancel: &CancelToken,
) -> Result<Vec<Vec<u8>>> {
    init()?;
    let read = (count + 1) as f64 / fps;
    let source = Source::open_window(input, probe, start, read, cancel)?;
    // one frame is the first from the start, which resampling can skip
    let resample =
        if probe.still_image || count == 1 { String::new() } else { format!("fps={fps},") };
    let spec = format!(
        "[in]{}{resample}scale={width}:{height}:flags=bilinear,setsar=1,format=rgba[out]",
        probe.orientation.filters()
    );
    let Some(mut filtered) = Filtered::new(source, &spec)? else { return Ok(Vec::new()) };
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
    let source = Source::open(plan, 2.0 * length, cancel)?;
    let no_frame = || Error::NoFrame { path: plan.input.clone(), at: plan.start };
    // the first frame from the start, as it is: resampling to a frame rate
    // can skip a short one
    let chain = picture_filter(plan, "rgba");
    let spec = format!("[in]{}{chain}[out]", plan.source.orientation.filters());
    let mut filtered = Filtered::new(source, &spec)?.ok_or_else(no_frame)?;
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
        Source::open(plan, (frames + 1) as f64 / plan.fps, cancel)?,
        Source::open_file(candidate, None, cancel)?,
    ];
    let mut next = [frame::Video::empty(), frame::Video::empty()];
    let mut open = [false; 2];
    for i in 0..2 {
        open[i] = sources[i].next(&mut next[i])?;
    }
    if !open[0] || !open[1] {
        return Err(Error::Libav("nothing to compare".into()));
    }

    let retime = |fps: f64| format!("format=yuv420p,settb=AVTB,setpts=N/({fps}*TB)");
    let reference = video_filter(plan, plan.fps, plan.length, "yuv420p");
    let spec = format!(
        "[source]{}{reference},{}[s];[attempt]{}[a];\
         [s][a]ssim=eof_action=repeat[out]",
        plan.source.orientation.filters(),
        retime(plan.fps),
        retime(fps),
    );
    let inputs = [
        ("source", sources[0].buffer_args(&next[0])),
        ("attempt", sources[1].buffer_args(&next[1])),
    ];
    let mut graph = filter_graph(&inputs, &spec)?;

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
        return Err(Error::Libav("the ssim filter reported nothing".into()));
    }
    Ok(total / count as f64)
}
