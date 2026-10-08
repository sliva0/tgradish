//! WebM inspection and in-place metadata patching.
//!
//! Patching never changes the file length. The Segment Info element is
//! rebuilt inside the space it already occupies plus any padding (Void
//! elements) right next to it. ffmpeg leaves such padding after the SeekHead,
//! so Info can grow by a few dozen bytes. If Info has to move, the SeekHead
//! entry pointing at it is updated. Nothing after Info moves.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use thiserror::Error;

use crate::ebml::{self, EbmlError, Element, ids};

/// Default TimestampScale: durations are in milliseconds.
const DEFAULT_TIMESTAMP_SCALE: u64 = 1_000_000;

#[derive(Debug, Error)]
pub enum WebmError {
    #[error("malformed WebM: {0}")]
    Ebml(#[from] EbmlError),
    #[error("not a WebM file: no EBML header")]
    NotEbml,
    #[error("no Segment element found")]
    NoSegment,
    #[error("no Info element found in the Segment")]
    NoInfo,
    #[error("fake duration must be more than 0 and at most {MAX_FAKE_DURATION} seconds, got {0}")]
    InvalidDuration(f64),
    #[error("the file has an invalid TimestampScale")]
    InvalidTimestampScale,
    #[error("{field} is too long ({len} bytes)")]
    ValueTooLong { field: &'static str, len: usize },
    #[error(
        "not enough padding around the Info element for the new metadata: \
         need {needed} bytes, have {available}"
    )]
    NoRoom { needed: usize, available: usize },
    #[error("the SeekHead entry for Info is too narrow to store its new position")]
    SeekPositionTooNarrow,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WebmError>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoTrack {
    pub number: u64,
    pub codec_id: String,
    pub width: u64,
    pub height: u64,
    pub alpha: bool,
    /// Frame duration declared by the track, in nanoseconds.
    pub default_duration_ns: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WebmInfo {
    pub doc_type: String,
    pub file_size: u64,
    pub timestamp_scale_ns: u64,
    /// Duration from the Segment Info header, in seconds. This is the value
    /// that duration spoofing changes.
    pub header_duration: Option<f64>,
    /// Duration of the video frames, in seconds, measured from block
    /// timestamps.
    pub content_duration: Option<f64>,
    pub title: Option<String>,
    pub muxing_app: Option<String>,
    pub writing_app: Option<String>,
    /// Values of `DURATION` tags, which ffmpeg writes for each track.
    pub duration_tags: Vec<String>,
    /// tgradish signature found in padding, see [`Patch::signature`].
    pub signature: Option<String>,
    pub video: Option<VideoTrack>,
    pub video_frames: u64,
    pub audio_tracks: u32,
    pub other_tracks: u32,
    /// Some element claimed to be longer than the file.
    pub truncated: bool,
}

impl WebmInfo {
    pub fn fps(&self) -> Option<f64> {
        if let Some(ns) = self.video.as_ref().and_then(|v| v.default_duration_ns)
            && ns > 0
        {
            return Some(1e9 / ns as f64);
        }
        let duration = self.content_duration.filter(|&d| d > 0.0)?;
        (self.video_frames > 0).then(|| self.video_frames as f64 / duration)
    }
}

/// Longest fake duration [`patch`] accepts, in seconds (a year).
pub const MAX_FAKE_DURATION: f64 = 365.0 * 24.0 * 3600.0;

/// Prefix of the signature text written into padding.
pub const SIGNATURE_PREFIX: &str = "tgradish";

/// Metadata changes applied by [`patch`]. `None` fields are left as they are.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Patch {
    /// Fake duration in seconds, written to the Info header and `DURATION`
    /// tags.
    pub duration: Option<f64>,
    pub title: Option<String>,
    pub muxing_app: Option<String>,
    pub writing_app: Option<String>,
    /// Text hidden in the padding next to Info, ignored by players and not
    /// shown by ffprobe. Must start with [`SIGNATURE_PREFIX`] to be found by
    /// [`inspect`]. Written only if there is enough padding left.
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatchReport {
    pub old_duration: Option<f64>,
    pub new_duration: Option<f64>,
    pub duration_tags_patched: usize,
    /// `DURATION` tags with too little space for the new value.
    pub duration_tags_skipped: usize,
    pub info_moved: bool,
    pub signature_written: bool,
}

struct Segment {
    el: Element,
    /// Children, with unknown-size Clusters resolved to their real end.
    children: Vec<Element>,
}

/// Finds the Segment and its children. Also returns the EBML DocType.
fn parse_segment(buf: &[u8]) -> Result<(String, Segment)> {
    let mut top = ebml::Children::new(buf, 0, buf.len());
    let header = match top.next() {
        Some(Ok(el)) if el.id == ids::EBML => el,
        Some(Err(err)) => return Err(err.into()),
        _ => return Err(WebmError::NotEbml),
    };
    let doc_type = header
        .children(buf)
        .find_id(ids::DOC_TYPE)?
        .map(|el| ebml::read_string(buf, &el))
        .unwrap_or_default();

    let el = top.find_id(ids::SEGMENT)?.ok_or(WebmError::NoSegment)?;
    let mut children = Vec::new();
    let mut iter = el.children(buf);
    while let Some(child) = iter.next() {
        let mut child = child?;
        if child.id == ids::CLUSTER && child.unknown_size {
            child.end = unknown_cluster_end(buf, &child)?;
            iter.seek(child.end);
        }
        children.push(child);
    }
    Ok((doc_type, Segment { el, children }))
}

/// A Cluster of unknown size ends where the next Segment-level element starts.
fn unknown_cluster_end(buf: &[u8], cluster: &Element) -> Result<usize> {
    let mut pos = cluster.data_start;
    while pos < cluster.end {
        let (id, _) = ebml::read_id(buf, pos)?;
        if ids::SEGMENT_CHILDREN.contains(&id) || id == ids::EBML {
            return Ok(pos);
        }
        pos = ebml::read_header(buf, pos, cluster.end)?.end;
    }
    Ok(cluster.end)
}

#[derive(Default)]
struct InfoFields {
    timestamp_scale: Option<u64>,
    duration: Option<f64>,
    title: Option<String>,
    muxing_app: Option<String>,
    writing_app: Option<String>,
}

fn parse_info(buf: &[u8], info: &Element) -> Result<InfoFields> {
    let mut fields = InfoFields::default();
    for child in info.children(buf) {
        let child = child?;
        match child.id {
            ids::TIMESTAMP_SCALE => fields.timestamp_scale = Some(ebml::read_uint(buf, &child)?),
            ids::DURATION => fields.duration = Some(ebml::read_float(buf, &child)?),
            ids::TITLE => fields.title = Some(ebml::read_string(buf, &child)),
            ids::MUXING_APP => fields.muxing_app = Some(ebml::read_string(buf, &child)),
            ids::WRITING_APP => fields.writing_app = Some(ebml::read_string(buf, &child)),
            _ => {}
        }
    }
    Ok(fields)
}

#[derive(Default)]
struct TrackStats {
    frames: u64,
    first: Option<i64>,
    last: Option<i64>,
    /// BlockDuration of the block at `last`, in timestamp units.
    last_duration: Option<u64>,
    /// Number of frames laced into the block at `last`.
    last_frames: u64,
}

impl TrackStats {
    fn add(&mut self, timestamp: i64, frames: u64, duration: Option<u64>) {
        self.frames = self.frames.saturating_add(frames);
        self.first = Some(self.first.map_or(timestamp, |first| first.min(timestamp)));
        if self.last.is_none_or(|last| timestamp >= last) {
            self.last = Some(timestamp);
            self.last_duration = duration;
            self.last_frames = frames;
        }
    }

    /// Time from the first frame to the end of the last one, in timestamp
    /// units, given the track's frame duration in timestamp units.
    fn duration(&self, frame_duration: Option<f64>) -> Option<f64> {
        let (first, last) = (self.first?, self.last?);
        let span = (i128::from(last) - i128::from(first)) as f64;
        let last_block = match (self.last_duration, frame_duration) {
            (Some(units), _) => units as f64,
            (None, Some(frame)) => frame * self.last_frames as f64,
            // without any declared durations, assume evenly spaced frames
            (None, None) if self.frames > self.last_frames => {
                span / (self.frames - self.last_frames) as f64 * self.last_frames as f64
            }
            (None, None) => 0.0,
        };
        Some(span + last_block)
    }
}

/// Reads track number, relative timestamp and frame count from a
/// (Simple)Block payload.
fn parse_block(data: &[u8], offset: usize) -> Result<(u64, i16, u64)> {
    let (track, len) = ebml::read_vint(data, 0).map_err(|_| EbmlError::InvalidVint(offset))?;
    let header = data.get(len..len + 3).ok_or(EbmlError::Truncated(offset + data.len()))?;
    let timestamp = i16::from_be_bytes([header[0], header[1]]);
    let lacing = header[2] & 0x06;
    let frames = if lacing == 0 {
        1
    } else {
        let count = data.get(len + 3).ok_or(EbmlError::Truncated(offset + data.len()))?;
        u64::from(*count) + 1
    };
    Ok((track, timestamp, frames))
}

/// Adds the blocks of a Cluster to `stats`. Returns whether any element in it
/// was truncated.
fn parse_cluster(
    buf: &[u8],
    cluster: &Element,
    stats: &mut HashMap<u64, TrackStats>,
) -> Result<bool> {
    // Timestamp should come first, but blocks before it still need it
    let cluster_ts = match cluster.children(buf).find_id(ids::TIMESTAMP)? {
        Some(el) => i64::try_from(ebml::read_uint(buf, &el)?).unwrap_or(i64::MAX),
        None => 0,
    };
    let mut truncated = false;
    for child in cluster.children(buf) {
        let child = child?;
        truncated |= child.truncated;
        match child.id {
            ids::SIMPLE_BLOCK => {
                let (track, rel, frames) = parse_block(child.data(buf), child.data_start)?;
                let ts = cluster_ts.saturating_add(i64::from(rel));
                stats.entry(track).or_default().add(ts, frames, None);
            }
            ids::BLOCK_GROUP => {
                let mut block = None;
                let mut duration = None;
                for item in child.children(buf) {
                    let item = item?;
                    truncated |= item.truncated;
                    match item.id {
                        ids::BLOCK => block = Some(item),
                        ids::BLOCK_DURATION => duration = Some(ebml::read_uint(buf, &item)?),
                        _ => {}
                    }
                }
                if let Some(block) = block {
                    let (track, rel, frames) = parse_block(block.data(buf), block.data_start)?;
                    let ts = cluster_ts.saturating_add(i64::from(rel));
                    stats.entry(track).or_default().add(ts, frames, duration);
                }
            }
            _ => {}
        }
    }
    Ok(truncated)
}

struct Tracks {
    video: Option<VideoTrack>,
    audio: u32,
    other: u32,
}

fn parse_tracks(buf: &[u8], tracks: &Element) -> Result<Tracks> {
    let mut result = Tracks { video: None, audio: 0, other: 0 };
    for entry in tracks.children(buf) {
        let entry = entry?;
        if entry.id != ids::TRACK_ENTRY {
            continue;
        }
        let mut number = 0;
        let mut kind = 0;
        let mut codec_id = String::new();
        let mut default_duration_ns = None;
        let (mut width, mut height, mut alpha) = (0, 0, false);
        for field in entry.children(buf) {
            let field = field?;
            match field.id {
                ids::TRACK_NUMBER => number = ebml::read_uint(buf, &field)?,
                ids::TRACK_TYPE => kind = ebml::read_uint(buf, &field)?,
                ids::CODEC_ID => codec_id = ebml::read_string(buf, &field),
                ids::DEFAULT_DURATION => default_duration_ns = Some(ebml::read_uint(buf, &field)?),
                ids::VIDEO => {
                    for video in field.children(buf) {
                        let video = video?;
                        match video.id {
                            ids::PIXEL_WIDTH => width = ebml::read_uint(buf, &video)?,
                            ids::PIXEL_HEIGHT => height = ebml::read_uint(buf, &video)?,
                            ids::ALPHA_MODE => alpha = ebml::read_uint(buf, &video)? != 0,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        match kind {
            1 if result.video.is_none() => {
                result.video = Some(VideoTrack {
                    number,
                    codec_id,
                    width,
                    height,
                    alpha,
                    default_duration_ns,
                });
            }
            2 => result.audio += 1,
            _ => result.other += 1,
        }
    }
    Ok(result)
}

/// TagString of a `DURATION` tag, with the elements containing it.
struct DurationTag {
    value: Element,
    /// Tags, Tag and SimpleTag, whose checksums cover the value.
    parents: [Element; 3],
}

fn duration_tags_in(buf: &[u8], tags: &Element) -> Result<Vec<DurationTag>> {
    let mut found = Vec::new();
    for tag in tags.children(buf) {
        let tag = tag?;
        if tag.id != ids::TAG {
            continue;
        }
        for simple in tag.children(buf) {
            let simple = simple?;
            if simple.id != ids::SIMPLE_TAG {
                continue;
            }
            let mut name = None;
            let mut value = None;
            for field in simple.children(buf) {
                let field = field?;
                match field.id {
                    ids::TAG_NAME => name = Some(ebml::read_string(buf, &field)),
                    ids::TAG_STRING => value = Some(field),
                    _ => {}
                }
            }
            if name.as_deref() == Some("DURATION")
                && let Some(value) = value
            {
                found.push(DurationTag { value, parents: [*tags, tag, simple] });
            }
        }
    }
    Ok(found)
}

/// Signature text at the start of a Void element, see [`Patch::signature`].
fn void_signature(buf: &[u8], el: &Element) -> Option<String> {
    if el.id != ids::VOID {
        return None;
    }
    let text = el.data(buf).split(|&b| b == 0).next()?;
    text.starts_with(SIGNATURE_PREFIX.as_bytes())
        .then(|| String::from_utf8_lossy(text).into_owned())
}

/// Reads stream properties and metadata of a WebM (or Matroska) file.
pub fn inspect(buf: &[u8]) -> Result<WebmInfo> {
    let (doc_type, segment) = parse_segment(buf)?;

    let mut fields = InfoFields::default();
    let mut tracks = Tracks { video: None, audio: 0, other: 0 };
    let mut stats = HashMap::new();
    let mut duration_tags = Vec::new();
    let mut truncated = segment.el.truncated;

    for child in &segment.children {
        truncated |= child.truncated;
        match child.id {
            ids::INFO => fields = parse_info(buf, child)?,
            ids::TRACKS => tracks = parse_tracks(buf, child)?,
            ids::CLUSTER => truncated |= parse_cluster(buf, child, &mut stats)?,
            ids::TAGS => {
                for tag in duration_tags_in(buf, child)? {
                    duration_tags.push(ebml::read_string(buf, &tag.value));
                }
            }
            _ => {}
        }
    }

    let scale = fields.timestamp_scale.unwrap_or(DEFAULT_TIMESTAMP_SCALE);
    let units_to_secs = |units: f64| units * scale as f64 / 1e9;

    let video_stats = tracks.video.as_ref().and_then(|video| stats.get(&video.number));
    let content_duration = video_stats.and_then(|s| {
        let frame_ns = tracks.video.as_ref()?.default_duration_ns;
        let frame_units = frame_ns.filter(|_| scale > 0).map(|ns| ns as f64 / scale as f64);
        s.duration(frame_units).map(units_to_secs)
    });

    Ok(WebmInfo {
        doc_type,
        file_size: buf.len() as u64,
        timestamp_scale_ns: scale,
        header_duration: fields.duration.map(units_to_secs),
        content_duration,
        title: fields.title,
        muxing_app: fields.muxing_app,
        writing_app: fields.writing_app,
        duration_tags,
        signature: segment.children.iter().find_map(|el| void_signature(buf, el)),
        video_frames: video_stats.map_or(0, |s| s.frames),
        video: tracks.video,
        audio_tracks: tracks.audio,
        other_tracks: tracks.other,
        truncated,
    })
}

pub fn inspect_file(path: &Path) -> Result<WebmInfo> {
    inspect(&std::fs::read(path)?)
}

/// Formats seconds the way ffmpeg writes `DURATION` tags.
fn format_tag_duration(secs: f64) -> String {
    let total_ns = (secs * 1e9).round() as u64;
    let (secs, ns) = (total_ns / 1_000_000_000, total_ns % 1_000_000_000);
    format!("{:02}:{:02}:{:02}.{:09}", secs / 3600, secs / 60 % 60, secs % 60, ns)
}

/// Turns a CRC-32 child of `parent` into a Void of the same size. Used after
/// changing bytes the checksum covers.
fn neutralize_crc(buf: &mut [u8], parent: &Element) -> Result<()> {
    // a CRC-32 element must be the first child
    let crc = parent.children(buf).next().transpose()?.filter(|el| el.id == ids::CRC32);
    if let Some(crc) = crc
        && let Some(void) = ebml::encode_void(crc.end - crc.start, b"")
    {
        buf[crc.start..crc.end].copy_from_slice(&void);
    }
    Ok(())
}

/// Builds the new Info element data.
fn build_info_data(
    buf: &[u8],
    info: &Element,
    patch: &Patch,
    duration_units: Option<f64>,
) -> Result<Vec<u8>> {
    /// Keeps the float width of an existing Duration, new ones use 8 bytes.
    fn duration_el(units: f64, width: usize) -> Vec<u8> {
        let bytes = match width {
            4 => (units as f32).to_be_bytes().to_vec(),
            _ => units.to_be_bytes().to_vec(),
        };
        ebml::encode_element(ids::DURATION, &bytes, 1).expect("8 bytes always fit")
    }

    let strings = [
        ("title", ids::TITLE, &patch.title),
        ("muxing app", ids::MUXING_APP, &patch.muxing_app),
        ("writing app", ids::WRITING_APP, &patch.writing_app),
    ];
    let mut new_strings = HashMap::new();
    for (field, id, value) in strings {
        if let Some(value) = value {
            let el = ebml::encode_string(id, value)
                .ok_or(WebmError::ValueTooLong { field, len: value.len() })?;
            new_strings.insert(id, el);
        }
    }

    let mut data = Vec::new();
    let mut has_duration = false;
    for child in info.children(buf) {
        let child = child?;
        let replacement = match child.id {
            // the checksum would no longer match, and padding is reclaimed
            ids::CRC32 | ids::VOID => continue,
            ids::DURATION => {
                has_duration = true;
                duration_units.map(|units| duration_el(units, child.data(buf).len()))
            }
            id => new_strings.remove(&id),
        };
        match replacement {
            Some(bytes) => data.extend(bytes),
            None => data.extend_from_slice(&buf[child.start..child.end]),
        }
    }

    if let Some(units) = duration_units
        && !has_duration
    {
        data.extend(duration_el(units, 8));
    }
    // fields that did not exist before, in a stable order
    for (_, id, _) in strings {
        if let Some(el) = new_strings.remove(&id) {
            data.extend(el);
        }
    }
    Ok(data)
}

/// Writes the new Info into `buf[span]`, together with padding. Returns the
/// new Info start and whether the signature fit.
fn layout_info(
    buf: &mut [u8],
    span: (usize, usize),
    old_start: usize,
    info_data: &[u8],
    signature: &[u8],
) -> Result<(usize, bool)> {
    let (span_start, span_end) = span;
    let available = span_end - span_start;
    let lead = old_start - span_start;

    // Each candidate is (Info start, padding before, padding after). Keeping
    // Info in place avoids touching the SeekHead, so it is tried first.
    // A gap of 1 byte cannot hold a Void, so the Info size VINT is widened
    // by one byte to swallow it.
    let too_long = WebmError::ValueTooLong { field: "Info", len: info_data.len() };
    let min_size_len = ebml::encode_size(info_data.len() as u64, 1).ok_or(too_long)?.len();
    for size_len_extra in 0..=1 {
        let info = ebml::encode_element(ids::INFO, info_data, min_size_len + size_len_extra)
            .ok_or(WebmError::NoRoom { needed: usize::MAX, available })?;
        if info.len() > available {
            return Err(WebmError::NoRoom { needed: info.len(), available });
        }
        let in_place = (lead + info.len() <= available)
            .then(|| (old_start, lead, available - lead - info.len()));
        let aligned = (span_end - info.len(), available - info.len(), 0);

        for (start, before, after) in in_place.into_iter().chain([aligned]) {
            if before == 1 || after == 1 {
                continue;
            }
            // the signature goes into whichever padding has room for it
            let (sig_before, sig_after) = match (before, after) {
                (b, _) if ebml::encode_void(b, signature).is_some() => (signature, &b""[..]),
                (_, a) if ebml::encode_void(a, signature).is_some() => (&b""[..], signature),
                _ => (&b""[..], &b""[..]),
            };
            let mut out = Vec::with_capacity(available);
            if before > 0 {
                out.extend(ebml::encode_void(before, sig_before).unwrap());
            }
            out.extend_from_slice(&info);
            if after > 0 {
                out.extend(ebml::encode_void(after, sig_after).unwrap());
            }
            debug_assert_eq!(out.len(), available);
            buf[span_start..span_end].copy_from_slice(&out);
            let signature_written = !signature.is_empty() && sig_before.len() + sig_after.len() > 0;
            return Ok((start, signature_written));
        }
    }
    Err(WebmError::NoRoom { needed: available + 1, available })
}

/// Points SeekHead entries for Info at its new position.
fn update_seek_head(buf: &mut [u8], segment: &Segment, info_start: usize) -> Result<()> {
    let position = (info_start - segment.el.data_start) as u64;
    let info_id = ebml::encode_id(ids::INFO);
    for seek_head in segment.children.iter().filter(|el| el.id == ids::SEEK_HEAD) {
        let mut patched = false;
        for seek in seek_head.children(buf).collect::<ebml::Result<Vec<_>>>()? {
            if seek.id != ids::SEEK {
                continue;
            }
            let fields: Vec<_> = seek.children(buf).collect::<ebml::Result<_>>()?;
            let is_info = fields.iter().any(|f| f.id == ids::SEEK_ID && f.data(buf) == info_id);
            let Some(pos_el) = fields.iter().find(|f| f.id == ids::SEEK_POSITION) else {
                continue;
            };
            if !is_info {
                continue;
            }
            let width = pos_el.end - pos_el.data_start;
            let bytes = position.to_be_bytes();
            if !(1..=8).contains(&width) || bytes[..8 - width].iter().any(|&b| b != 0) {
                return Err(WebmError::SeekPositionTooNarrow);
            }
            buf[pos_el.data_start..pos_el.end].copy_from_slice(&bytes[8 - width..]);
            neutralize_crc(buf, &seek)?;
            patched = true;
        }
        if patched {
            neutralize_crc(buf, seek_head)?;
        }
    }
    Ok(())
}

/// Applies `changes` to a WebM file in memory. The length never changes, and
/// on error `buf` is left untouched.
pub fn patch(buf: &mut [u8], changes: &Patch) -> Result<PatchReport> {
    let mut copy = buf.to_vec();
    let report = patch_in_place(&mut copy, changes)?;
    buf.copy_from_slice(&copy);
    Ok(report)
}

fn patch_in_place(buf: &mut [u8], patch: &Patch) -> Result<PatchReport> {
    if let Some(duration) = patch.duration
        && !(duration.is_finite() && duration > 0.0 && duration <= MAX_FAKE_DURATION)
    {
        return Err(WebmError::InvalidDuration(duration));
    }

    let (_, segment) = parse_segment(buf)?;
    let info_index =
        segment.children.iter().position(|el| el.id == ids::INFO).ok_or(WebmError::NoInfo)?;
    let info = segment.children[info_index];
    let fields = parse_info(buf, &info)?;
    let scale = match fields.timestamp_scale.unwrap_or(DEFAULT_TIMESTAMP_SCALE) {
        0 => return Err(WebmError::InvalidTimestampScale),
        scale => scale as f64,
    };
    let duration_units = patch.duration.map(|secs| secs * 1e9 / scale);
    if let Some(units) = duration_units
        && units > f64::from(f32::MAX)
    {
        // would not fit a 4-byte Duration
        return Err(WebmError::InvalidTimestampScale);
    }

    // Info plus the padding right before and after it
    let is_void = |el: &&Element| el.id == ids::VOID;
    let before: Vec<_> =
        segment.children[..info_index].iter().rev().take_while(is_void).copied().collect();
    let after: Vec<_> =
        segment.children[info_index + 1..].iter().take_while(is_void).copied().collect();
    let span = (
        before.last().map_or(info.start, |el| el.start),
        after.last().map_or(info.end, |el| el.end),
    );

    // the padding is rewritten, so keep a signature that is already there
    let signature = patch
        .signature
        .clone()
        .or_else(|| before.iter().chain(&after).find_map(|el| void_signature(buf, el)))
        .unwrap_or_default();
    let info_data = build_info_data(buf, &info, patch, duration_units)?;
    let (info_start, signature_written) =
        layout_info(buf, span, info.start, &info_data, signature.as_bytes())?;
    let info_moved = info_start != info.start;
    if info_moved {
        update_seek_head(buf, &segment, info_start)?;
    }

    let mut tags_patched = 0;
    let mut tags_skipped = 0;
    if let Some(secs) = patch.duration {
        let text = format_tag_duration(secs);
        let mut edits = Vec::new();
        for tags in segment.children.iter().filter(|el| el.id == ids::TAGS) {
            edits.extend(duration_tags_in(buf, tags)?);
        }
        for DurationTag { value, parents } in edits {
            let range = value.data_start..value.end;
            if range.len() < text.len() {
                tags_skipped += 1;
                continue;
            }
            buf[range.clone()].fill(0);
            buf[range.start..range.start + text.len()].copy_from_slice(text.as_bytes());
            for parent in &parents {
                neutralize_crc(buf, parent)?;
            }
            tags_patched += 1;
        }
    }
    // a Segment checksum covers everything changed above
    neutralize_crc(buf, &segment.el)?;

    Ok(PatchReport {
        old_duration: fields.duration.map(|units| units * scale / 1e9),
        new_duration: patch.duration,
        duration_tags_patched: tags_patched,
        duration_tags_skipped: tags_skipped,
        info_moved,
        signature_written,
    })
}

/// Patches `input` and writes the result to `output`, which may be the same
/// file. The output is written to a temporary file first and then renamed.
/// Without `replace`, an existing output is an
/// [`AlreadyExists`](std::io::ErrorKind::AlreadyExists) error.
pub fn patch_file(
    input: &Path,
    output: &Path,
    changes: &Patch,
    replace: bool,
) -> Result<PatchReport> {
    let mut buf = std::fs::read(input)?;
    let report = patch(&mut buf, changes)?;
    crate::fsutil::write_file(output, &buf, replace)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ebml::build::*;

    /// Options for the synthetic WebM files used in tests.
    struct Fixture {
        void_after_seek_head: usize,
        frames: u64,
        unknown_sizes: bool,
    }

    impl Default for Fixture {
        fn default() -> Self {
            Self { void_after_seek_head: 89, frames: 75, unknown_sizes: false }
        }
    }

    impl Fixture {
        /// Builds a file laid out like ffmpeg's: SeekHead, Void, Info, Tracks,
        /// Tags, Cluster. 25 fps, 40 ms frames.
        fn build(&self) -> Vec<u8> {
            let info = el(
                ids::INFO,
                &concat(&[
                    uint(ids::TIMESTAMP_SCALE, 1_000_000),
                    el(ids::MUXING_APP, b"Lavf63.1.102"),
                    el(ids::WRITING_APP, b"Lavf63.1.102"),
                    f64_el(ids::DURATION, self.frames as f64 * 40.0),
                ]),
            );
            let tracks = el(
                ids::TRACKS,
                &el(
                    ids::TRACK_ENTRY,
                    &concat(&[
                        uint(ids::TRACK_NUMBER, 1),
                        uint(ids::TRACK_TYPE, 1),
                        el(ids::CODEC_ID, b"V_VP9"),
                        uint(ids::DEFAULT_DURATION, 40_000_000),
                        el(
                            ids::VIDEO,
                            &concat(&[
                                uint(ids::PIXEL_WIDTH, 512),
                                uint(ids::PIXEL_HEIGHT, 384),
                                uint(ids::ALPHA_MODE, 1),
                            ]),
                        ),
                    ]),
                ),
            );
            let tags = el(
                ids::TAGS,
                &el(
                    ids::TAG,
                    &el(
                        ids::SIMPLE_TAG,
                        &concat(&[
                            el(ids::TAG_NAME, b"DURATION"),
                            el(ids::TAG_STRING, b"00:00:03.000000000\0"),
                        ]),
                    ),
                ),
            );
            let mut blocks = vec![uint(ids::TIMESTAMP, 0)];
            for i in 0..self.frames {
                let ts = (i * 40) as i16;
                let mut block = vec![0x81];
                block.extend(ts.to_be_bytes());
                block.extend([0x80, 0xAA, 0xBB]);
                blocks.push(el(ids::SIMPLE_BLOCK, &block));
            }
            let cluster = if self.unknown_sizes {
                el_unknown_size(ids::CLUSTER, &concat(&blocks))
            } else {
                el(ids::CLUSTER, &concat(&blocks))
            };

            // the SeekHead has a fixed size, so Info's position is known
            let seek_head_len = self.seek_head(0).len();
            let void = if self.void_after_seek_head > 0 {
                ebml::encode_void(self.void_after_seek_head, b"").unwrap()
            } else {
                vec![]
            };
            let info_pos = (seek_head_len + void.len()) as u64;
            let body = concat(&[self.seek_head(info_pos), void, info, tracks, tags, cluster]);

            let segment = if self.unknown_sizes {
                el_unknown_size(ids::SEGMENT, &body)
            } else {
                el(ids::SEGMENT, &body)
            };
            concat(&[el(ids::EBML, &el(ids::DOC_TYPE, b"webm")), segment])
        }

        fn seek_head(&self, info_pos: u64) -> Vec<u8> {
            let seek = concat(&[
                el(ids::SEEK_ID, &ebml::encode_id(ids::INFO)),
                el(ids::SEEK_POSITION, &info_pos.to_be_bytes()[6..]),
            ]);
            el(ids::SEEK_HEAD, &el(ids::SEEK, &seek))
        }
    }

    fn info_position_from_seek_head(buf: &[u8]) -> usize {
        let (_, segment) = parse_segment(buf).unwrap();
        let seek_head = segment.children.iter().find(|el| el.id == ids::SEEK_HEAD).unwrap();
        let seek = seek_head.children(buf).find_id(ids::SEEK).unwrap().unwrap();
        let pos = seek.children(buf).find_id(ids::SEEK_POSITION).unwrap().unwrap();
        segment.el.data_start + ebml::read_uint(buf, &pos).unwrap() as usize
    }

    fn assert_close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("value is missing");
        assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
    }

    #[test]
    fn inspects_stream_properties() {
        let info = inspect(&Fixture::default().build()).unwrap();
        assert_eq!(info.doc_type, "webm");
        assert_eq!(info.timestamp_scale_ns, 1_000_000);
        assert_close(info.header_duration, 3.0);
        assert_close(info.content_duration, 3.0);
        assert_close(info.fps(), 25.0);
        assert_eq!(info.video_frames, 75);
        assert_eq!(info.muxing_app.as_deref(), Some("Lavf63.1.102"));
        assert_eq!(info.duration_tags, ["00:00:03.000000000"]);
        assert_eq!(info.signature, None);
        assert!(!info.truncated);

        let video = info.video.unwrap();
        assert_eq!((video.width, video.height, video.alpha), (512, 384, true));
        assert_eq!(video.codec_id, "V_VP9");
    }

    #[test]
    fn inspects_unknown_size_segment_and_cluster() {
        let info = inspect(&Fixture { unknown_sizes: true, ..Default::default() }.build()).unwrap();
        assert_eq!(info.video_frames, 75);
        assert_close(info.content_duration, 3.0);
    }

    #[test]
    fn inspects_truncated_files() {
        let mut buf = Fixture::default().build();
        buf.truncate(buf.len() - 20);
        let info = inspect(&buf);
        // the last block is cut in half, which is reported as an error or as
        // truncation, but never panics
        if let Ok(info) = info {
            assert!(info.truncated);
        }
    }

    #[test]
    fn rejects_non_webm() {
        assert!(matches!(inspect(b"\x00\x00\x00\x18ftypisom"), Err(WebmError::Ebml(_))));
        assert!(matches!(inspect(&el(ids::INFO, b"")), Err(WebmError::NotEbml)));
    }

    #[test]
    fn spoofs_duration_in_place() {
        let mut buf = Fixture::default().build();
        let len = buf.len();
        let report =
            patch(&mut buf, &Patch { duration: Some(0.42069), ..Default::default() }).unwrap();

        assert_eq!(buf.len(), len);
        assert_close(report.old_duration, 3.0);
        assert!(!report.info_moved);
        assert_eq!(report.duration_tags_patched, 1);

        let info = inspect(&buf).unwrap();
        assert_close(info.header_duration, 0.42069);
        assert_close(info.content_duration, 3.0);
        assert_eq!(info.duration_tags, ["00:00:00.420690000"]);
    }

    #[test]
    fn rejects_invalid_durations() {
        let mut buf = Fixture::default().build();
        for duration in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let result = patch(&mut buf, &Patch { duration: Some(duration), ..Default::default() });
            assert!(matches!(result, Err(WebmError::InvalidDuration(_))));
        }
    }

    #[test]
    fn grows_info_into_padding_and_moves_it() {
        let mut buf = Fixture::default().build();
        let changes = Patch {
            duration: Some(0.42069),
            title: Some("pig".into()),
            muxing_app: Some("tgradish 2.0.0".into()),
            writing_app: Some("tgradish 2.0.0 (libvpx-vp9)".into()),
            signature: Some("tgradish 2.0.0 was here".into()),
        };
        let report = patch(&mut buf, &changes).unwrap();
        assert!(report.info_moved);
        assert!(report.signature_written);

        let info = inspect(&buf).unwrap();
        assert_eq!(info.title.as_deref(), Some("pig"));
        assert_eq!(info.muxing_app.as_deref(), Some("tgradish 2.0.0"));
        assert_eq!(info.writing_app.as_deref(), Some("tgradish 2.0.0 (libvpx-vp9)"));
        assert_eq!(info.signature.as_deref(), Some("tgradish 2.0.0 was here"));
        assert_close(info.header_duration, 0.42069);
        assert_eq!(info.video_frames, 75);

        let (_, segment) = parse_segment(&buf).unwrap();
        let info_el = segment.children.iter().find(|el| el.id == ids::INFO).unwrap();
        assert_eq!(info_position_from_seek_head(&buf), info_el.start);
    }

    #[test]
    fn patching_twice_is_stable() {
        let mut buf = Fixture::default().build();
        let changes = Patch {
            duration: Some(1.0),
            writing_app: Some("x".repeat(40)),
            signature: Some("tgradish test".into()),
            ..Default::default()
        };
        patch(&mut buf, &changes).unwrap();
        let once = buf.clone();
        patch(&mut buf, &changes).unwrap();
        assert_eq!(buf, once);
    }

    #[test]
    fn fails_without_room() {
        let mut buf = Fixture { void_after_seek_head: 0, ..Default::default() }.build();
        let original = buf.clone();
        let changes = Patch { title: Some("a long title".into()), ..Default::default() };
        assert!(matches!(patch(&mut buf, &changes), Err(WebmError::NoRoom { .. })));
        assert_eq!(buf, original);

        // same-size changes still work without padding
        let changes = Patch { duration: Some(0.5), ..Default::default() };
        patch(&mut buf, &changes).unwrap();
        assert_close(inspect(&buf).unwrap().header_duration, 0.5);
    }

    #[test]
    fn handles_every_padding_size() {
        // growing Info by 1..=20 bytes into 0..=24 bytes of padding covers the
        // 1-byte gap cases
        for void in [0, 2, 3, 4, 5, 10, 21, 22, 23, 24] {
            for extra in 1..=20 {
                let mut buf = Fixture { void_after_seek_head: void, ..Default::default() }.build();
                let changes = Patch { title: Some("t".repeat(extra)), ..Default::default() };
                match patch(&mut buf, &changes) {
                    Ok(_) => {
                        let info = inspect(&buf).unwrap();
                        assert_eq!(info.title.as_deref(), Some("t".repeat(extra).as_str()));
                        assert_eq!(info.video_frames, 75);
                        let (_, segment) = parse_segment(&buf).unwrap();
                        let info_el = segment.children.iter().find(|el| el.id == ids::INFO);
                        assert_eq!(info_position_from_seek_head(&buf), info_el.unwrap().start);
                    }
                    Err(WebmError::NoRoom { .. }) => assert!(extra + 3 > void),
                    Err(err) => panic!("void {void}, extra {extra}: {err}"),
                }
            }
        }
    }

    #[test]
    fn formats_tag_durations() {
        assert_eq!(format_tag_duration(0.42069), "00:00:00.420690000");
        assert_eq!(format_tag_duration(3725.5), "01:02:05.500000000");
    }

    /// Minimal file with the given Segment children: Info with TimestampScale
    /// `scale`, one 40 ms video track, then `rest`.
    fn minimal_webm(scale: u64, rest: &[Vec<u8>], unknown_sizes: bool) -> Vec<u8> {
        let info = el(
            ids::INFO,
            &concat(&[uint(ids::TIMESTAMP_SCALE, scale), f64_el(ids::DURATION, 1000.0)]),
        );
        let track = el(
            ids::TRACK_ENTRY,
            &concat(&[
                uint(ids::TRACK_NUMBER, 1),
                uint(ids::TRACK_TYPE, 1),
                el(ids::CODEC_ID, b"V_VP9"),
                uint(ids::DEFAULT_DURATION, 40_000_000),
            ]),
        );
        let mut children = vec![info, el(ids::TRACKS, &track)];
        children.extend_from_slice(rest);
        let body = concat(&children);
        let segment = if unknown_sizes {
            el_unknown_size(ids::SEGMENT, &body)
        } else {
            el(ids::SEGMENT, &body)
        };
        concat(&[el(ids::EBML, &el(ids::DOC_TYPE, b"webm")), segment])
    }

    /// SimpleBlock for track 1. `laced` extra frames use fixed-size lacing.
    fn block(timestamp: i16, laced: u8) -> Vec<u8> {
        let mut data = vec![0x81];
        data.extend(timestamp.to_be_bytes());
        if laced == 0 {
            data.extend([0x80, 0xAA]);
        } else {
            data.extend([0x84, laced]);
            data.extend(std::iter::repeat_n(0xAA, usize::from(laced) + 1));
        }
        el(ids::SIMPLE_BLOCK, &data)
    }

    #[test]
    fn counts_laced_frames_in_duration() {
        let cluster = el(ids::CLUSTER, &concat(&[uint(ids::TIMESTAMP, 0), block(0, 2)]));
        let info = inspect(&minimal_webm(1_000_000, &[cluster], false)).unwrap();
        assert_eq!(info.video_frames, 3);
        assert_close(info.content_duration, 0.12);
    }

    #[test]
    fn uses_cluster_timestamp_written_after_blocks() {
        let first = el(ids::CLUSTER, &concat(&[uint(ids::TIMESTAMP, 0), block(0, 0)]));
        let second = el(ids::CLUSTER, &concat(&[block(0, 0), uint(ids::TIMESTAMP, 1000)]));
        let info = inspect(&minimal_webm(1_000_000, &[first, second], false)).unwrap();
        assert_close(info.content_duration, 1.04);
    }

    #[test]
    fn survives_extreme_timestamps() {
        let first = el(ids::CLUSTER, &concat(&[uint(ids::TIMESTAMP, 0), block(-1, 0)]));
        let last = el(ids::CLUSTER, &concat(&[uint(ids::TIMESTAMP, u64::MAX), block(i16::MAX, 0)]));
        let info = inspect(&minimal_webm(1_000_000, &[first, last], false)).unwrap();
        assert!(info.content_duration.unwrap() > 0.0);
    }

    #[test]
    fn reports_truncation_inside_unknown_size_clusters() {
        let mut cut_block = vec![0xA3, 0x80 | 100, 0x81, 0x00, 0x00, 0x80, 0xAA];
        cut_block.truncate(7);
        let cluster = el_unknown_size(ids::CLUSTER, &concat(&[uint(ids::TIMESTAMP, 0), cut_block]));
        let info = inspect(&minimal_webm(1_000_000, &[cluster], true)).unwrap();
        assert!(info.truncated);
    }

    #[test]
    fn rejects_unusable_durations_and_scales() {
        let mut buf = Fixture::default().build();
        let huge = Patch { duration: Some(f64::MAX), ..Default::default() };
        assert!(matches!(patch(&mut buf, &huge), Err(WebmError::InvalidDuration(_))));

        let mut buf = minimal_webm(0, &[], false);
        let changes = Patch { duration: Some(1.0), ..Default::default() };
        assert!(matches!(patch(&mut buf, &changes), Err(WebmError::InvalidTimestampScale)));
    }

    #[test]
    fn clears_segment_checksum() {
        let crc = el(ids::CRC32, &[1, 2, 3, 4]);
        let mut buf = minimal_webm(1_000_000, &[], false);
        // put the CRC-32 first in the Segment by rebuilding it
        let (_, segment) = parse_segment(&buf).unwrap();
        let body = concat(&[crc, segment.el.data(&buf).to_vec()]);
        buf = concat(&[el(ids::EBML, &el(ids::DOC_TYPE, b"webm")), el(ids::SEGMENT, &body)]);

        patch(&mut buf, &Patch { duration: Some(0.5), ..Default::default() }).unwrap();
        let (_, segment) = parse_segment(&buf).unwrap();
        assert_eq!(segment.children[0].id, ids::VOID);
    }

    #[test]
    fn keeps_existing_signature() {
        let mut buf = Fixture::default().build();
        let signed = Patch { signature: Some("tgradish was here".into()), ..Default::default() };
        patch(&mut buf, &signed).unwrap();
        patch(&mut buf, &Patch { duration: Some(0.5), ..Default::default() }).unwrap();
        assert_eq!(inspect(&buf).unwrap().signature.as_deref(), Some("tgradish was here"));
    }

    #[test]
    fn patch_file_leaves_other_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.webm");
        let bystander = dir.path().join("out.webm.tgradish-tmp");
        std::fs::write(&bystander, b"keep me").unwrap();
        let input = dir.path().join("in.webm");
        std::fs::write(&input, Fixture::default().build()).unwrap();

        let changes = Patch { duration: Some(0.5), ..Default::default() };
        patch_file(&input, &output, &changes, false).unwrap();
        assert_eq!(std::fs::read(&bystander).unwrap(), b"keep me");
        let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 3, "no temporary files left behind");
    }
}
