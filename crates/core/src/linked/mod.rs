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
use crate::backend::{Pass, pass_log_file};
use crate::convert::{Plan, frame_count};
use crate::error::{Error, Result};
use crate::events::{Params, Rate};
use crate::ffmpeg::{CancelToken, Capabilities, Output, Probe};
use crate::ffmpeg::{Orientation, pix_fmt_has_alpha, video_filter};

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
    let probe_error = |message: String| Error::Probe { path: input.to_path_buf(), message };
    let (context, stream) = (source.input(), source.stream());

    let decoder = source.decoder();
    let (mut width, mut height) = (f64::from(decoder.width()), f64::from(decoder.height()));
    if width == 0.0 || height == 0.0 {
        return Err(probe_error("video stream has no size".into()));
    }
    // like ffprobe: the stream's aspect ratio wins over the codec's
    // SAFETY: reading a plain field of the open stream
    let stream_sar = unsafe { Rational::from((*stream.as_ptr()).sample_aspect_ratio) };
    if let Some(sar) = ratio(stream_sar).or_else(|| ratio(decoder.aspect_ratio())) {
        width *= sar;
    }
    let orientation = orientation(&stream);
    if orientation.swaps_size() {
        std::mem::swap(&mut width, &mut height);
    }

    let format_name = context.format().name().to_string();
    let codec = stream.parameters().id().name().to_string();
    let pix_fmt = decoder.format().descriptor().map(|d| d.name().to_string()).unwrap_or_default();
    let webm_alpha = stream
        .metadata()
        .iter()
        .any(|(key, value)| key.eq_ignore_ascii_case("alpha_mode") && value == "1");
    let decoder_name = match codec.as_str() {
        "vp9" if webm_alpha => Some("libvpx-vp9".to_string()),
        "vp8" if webm_alpha => Some("libvpx".to_string()),
        _ => None,
    };
    let still_image = (format_name.contains("image2") || format_name.ends_with("_pipe"))
        && codec != "gif"
        && stream.frames() <= 1;
    let duration = if still_image {
        None
    } else {
        seconds(stream.duration(), stream.time_base())
            .or_else(|| seconds(context.duration(), Rational::new(1, ff::ffi::AV_TIME_BASE)))
    };

    Ok(Probe {
        format: format_name,
        codec,
        width: width.round() as u32,
        height: height.round() as u32,
        fps: ratio(stream.avg_frame_rate()).or_else(|| ratio(stream.rate())),
        duration,
        alpha: pix_fmt_has_alpha(&pix_fmt) || webm_alpha,
        still_image,
        orientation,
        decoder: decoder_name,
    })
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
    let mut source = Source::open(plan, read, cancel)?;
    let mut frame = frame::Video::empty();
    if !source.next(&mut frame)? {
        return Err(Error::Libav(format!("{} has no frames to encode", plan.input.display())));
    }

    let pix_fmt = if plan.alpha { "yuva420p" } else { "yuv420p" };
    let chain = video_filter(plan, params.fps, params.length, pix_fmt);
    let spec = format!("[in]{}{chain}[out]", plan.source.orientation.filters());
    let mut graph = filter_graph(&[("in", source.buffer_args(&frame))], &spec)?;
    let time_base = graph.get("out").expect("added").sink().time_base();
    let mut encoder = Encoder::open(plan, params, pass, output, time_base)?;
    let frame_duration = encoder.frame_duration;

    let mut filtered = frame::Video::empty();
    let mut sent = 0;
    let mut input_open = true;
    while sent < total {
        cancel.check()?;
        if input_open {
            let mut source_filter = graph.get("in").expect("added");
            let mut source_filter = source_filter.source();
            source_filter.add(&frame).map_err(libav("filtering"))?;
            if !source.next(&mut frame)? {
                source_filter.flush().map_err(libav("filtering"))?;
                input_open = false;
            }
        }
        loop {
            match graph.get("out").expect("added").sink().frame(&mut filtered) {
                Ok(()) => {}
                Err(ff::Error::Eof) => break,
                Err(err) if is_again(&err) => break,
                Err(err) => return Err(libav("filtering")(err)),
            }
            if sent == total {
                break;
            }
            filtered.set_kind(ff::picture::Type::None);
            // SAFETY: setting a plain field of a frame we own
            unsafe { (*filtered.as_mut_ptr()).duration = frame_duration };
            encoder.send(Some(&filtered))?;
            sent += 1;
            on_output(Output::Time { micros: (sent as f64 / params.fps * 1e6) as u64 });
        }
        if !input_open && sent < total {
            // the filters are flushed and drained: the input was shorter
            break;
        }
    }
    encoder.finish(pass)
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
