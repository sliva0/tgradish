//! Reading what a WebM holds: its streams, its metadata and tgradish's
//! signature.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use super::ebml::{self, Element, ids};
use super::{DEFAULT_TIMESTAMP_SCALE, EbmlError, Result, SIGNATURE_PREFIX, WebmError};
use crate::source::unreadable;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct VideoTrack {
    pub number: u64,
    pub codec_id: String,
    pub width: u64,
    pub height: u64,
    pub alpha: bool,
    /// Frame duration declared by the track, in nanoseconds.
    pub default_duration_ns: Option<u64>,
    /// The track's UID.
    pub uid: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
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
    /// tgradish signature found in padding, see [`Patch::signature`](super::Patch::signature).
    pub signature: Option<String>,
    pub video: Option<VideoTrack>,
    pub video_frames: u64,
    pub audio_tracks: u32,
    pub other_tracks: u32,
    /// Some element claimed to be longer than the file.
    pub truncated: bool,
}

impl WebmInfo {
    /// Whether the header says it is shorter than its frames last, as
    /// spoofing makes it. Unspoofed, the two differ by less than a frame,
    /// as the header is rounded and the last frame's length guessed.
    pub fn spoofed(&self) -> bool {
        matches!(
            (self.header_duration, self.content_duration),
            (Some(header), Some(content)) if content > header + 0.05
        )
    }

    /// What it would be: emoji are the only video stickers of their box's
    /// size, square 100 px.
    pub fn target(&self) -> crate::telegram::Target {
        use crate::telegram::Target;
        let (width, height) = Target::Emoji.box_size();
        let emoji = (u64::from(width), u64::from(height));
        match &self.video {
            Some(video) if (video.width, video.height) == emoji => Target::Emoji,
            _ => Target::Sticker,
        }
    }

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

pub(super) struct Segment {
    pub(super) el: Element,
    /// Children, with unknown-size Clusters resolved to their real end.
    pub(super) children: Vec<Element>,
}

/// Finds the Segment and its children. Also returns the EBML DocType.
pub(super) fn parse_segment(buf: &[u8]) -> Result<(String, Segment)> {
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
pub(super) struct InfoFields {
    pub(super) timestamp_scale: Option<u64>,
    pub(super) duration: Option<f64>,
    pub(super) title: Option<String>,
    pub(super) muxing_app: Option<String>,
    pub(super) writing_app: Option<String>,
}

pub(super) fn parse_info(buf: &[u8], info: &Element) -> Result<InfoFields> {
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
        let mut uid = None;
        let (mut width, mut height, mut alpha) = (0, 0, false);
        for field in entry.children(buf) {
            let field = field?;
            match field.id {
                ids::TRACK_NUMBER => number = ebml::read_uint(buf, &field)?,
                ids::TRACK_TYPE => kind = ebml::read_uint(buf, &field)?,
                ids::CODEC_ID => codec_id = ebml::read_string(buf, &field),
                ids::DEFAULT_DURATION => default_duration_ns = Some(ebml::read_uint(buf, &field)?),
                ids::TRACK_UID => uid = Some(ebml::read_uint(buf, &field)?),
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
                    uid,
                });
            }
            2 => result.audio += 1,
            _ => result.other += 1,
        }
    }
    Ok(result)
}

/// TagString of a `DURATION` tag, with the elements containing it.
pub(super) struct DurationTag {
    pub(super) value: Element,
    /// Tags, Tag and SimpleTag, whose checksums cover the value.
    pub(super) parents: [Element; 3],
}

pub(super) fn duration_tags_in(buf: &[u8], tags: &Element) -> Result<Vec<DurationTag>> {
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

/// The TagTrackUIDs in `tags`, with their parents: Tags, Tag, Targets.
pub(super) fn tag_track_uids(buf: &[u8], tags: &Element) -> Result<Vec<(Element, [Element; 3])>> {
    let mut found = Vec::new();
    for tag in tags.children(buf) {
        let tag = tag?;
        if tag.id != ids::TAG {
            continue;
        }
        for targets in tag.children(buf) {
            let targets = targets?;
            if targets.id != ids::TARGETS {
                continue;
            }
            for field in targets.children(buf) {
                let field = field?;
                if field.id == ids::TAG_TRACK_UID {
                    found.push((field, [*tags, tag, targets]));
                }
            }
        }
    }
    Ok(found)
}

/// Signature text at the start of a Void element, see [`Patch::signature`](super::Patch::signature).
pub(super) fn void_signature(buf: &[u8], el: &Element) -> Option<String> {
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

/// How a WebM file starts: the EBML header's ID.
const EBML_MAGIC: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];

/// The largest file [`inspect_file`] reads, in bytes: far more than any
/// sticker, so a video it is pointed at isn't read into memory whole.
pub const MAX_INSPECTED_BYTES: u64 = 256 << 20;

/// [`inspect`] of the file at `path`, which is read only if it starts as
/// WebM does and has at most [`MAX_INSPECTED_BYTES`].
pub fn inspect_file(path: &Path) -> crate::Result<WebmInfo> {
    starts_as_webm(path)?;
    let bytes = crate::fsutil::read_at_most(path, MAX_INSPECTED_BYTES)
        .map_err(|err| unreadable(path, err))?
        .ok_or(WebmError::TooLarge)?;
    Ok(inspect(&bytes)?)
}

/// Fails unless the file at `path` starts with the EBML header's ID.
fn starts_as_webm(path: &Path) -> crate::Result<()> {
    let mut start = [0u8; 4];
    let read = std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut start));
    match read {
        Ok(()) if start == EBML_MAGIC => Ok(()),
        Ok(()) => Err(WebmError::NotEbml.into()),
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
            Err(WebmError::NotEbml.into())
        }
        Err(err) => Err(unreadable(path, err)),
    }
}

/// What the start of a WebM file says of the app that made it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Head {
    pub(crate) muxing_app: Option<String>,
    pub(crate) writing_app: Option<String>,
    /// See [`WebmInfo::signature`].
    pub(crate) signature: Option<String>,
}

/// How much of a file [`head_file`] reads: ffmpeg writes Info and the
/// padding beside it in the first few hundred bytes.
const HEAD_BYTES: u64 = 64 << 10;

/// The [`Head`] of the WebM file at `path`, from no more of it than
/// [`HEAD_BYTES`].
pub(crate) fn head_file(path: &Path) -> crate::Result<Head> {
    starts_as_webm(path)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| {
            std::io::Read::read_to_end(&mut std::io::Read::take(file, HEAD_BYTES), &mut bytes)
        })
        .map_err(|err| unreadable(path, err))?;
    Ok(head(&bytes)?)
}

/// The [`Head`] of WebM `buf`, which may stop part of the way: what comes
/// before the first Cluster.
pub(crate) fn head(buf: &[u8]) -> Result<Head> {
    let mut top = ebml::Children::new(buf, 0, buf.len());
    match top.next() {
        Some(Ok(el)) if el.id == ids::EBML => {}
        Some(Err(err)) => return Err(err.into()),
        _ => return Err(WebmError::NotEbml),
    }
    let segment = top.find_id(ids::SEGMENT)?.ok_or(WebmError::NoSegment)?;
    let mut head = Head::default();
    for child in segment.children(buf) {
        // where the part read ends
        let Ok(child) = child else { break };
        match child.id {
            ids::CLUSTER => break,
            ids::INFO if !child.truncated => {
                let fields = parse_info(buf, &child)?;
                (head.muxing_app, head.writing_app) = (fields.muxing_app, fields.writing_app);
            }
            ids::VOID if head.signature.is_none() => head.signature = void_signature(buf, &child),
            _ => {}
        }
    }
    Ok(head)
}
