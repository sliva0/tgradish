//! Changing a WebM's metadata in place, as spoofing its duration does.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use super::ebml::{self, Element, ids};
use super::inspect::{
    DurationTag, Segment, duration_tags_in, parse_info, parse_segment, tag_track_uids,
    void_signature,
};
use super::{DEFAULT_TIMESTAMP_SCALE, MAX_FAKE_DURATION, Result, WebmError};

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
    /// shown by ffprobe. Must start with [`SIGNATURE_PREFIX`](super::SIGNATURE_PREFIX) to be found by
    /// [`inspect`](super::inspect). Written only if there is enough padding left.
    pub signature: Option<String>,
    /// A new UID for the video track, written over the old one when that
    /// takes 8 bytes, as ffmpeg's do.
    pub track_uid: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PatchReport {
    pub old_duration: Option<f64>,
    pub new_duration: Option<f64>,
    pub duration_tags_patched: usize,
    /// `DURATION` tags with too little space for the new value.
    pub duration_tags_skipped: usize,
    pub info_moved: bool,
    pub signature_written: bool,
}

/// Formats seconds the way ffmpeg writes `DURATION` tags.
pub(super) fn format_tag_duration(secs: f64) -> String {
    let total_ns = (secs * 1e9).round() as u64;
    let (secs, ns) = (total_ns / 1_000_000_000, total_ns % 1_000_000_000);
    format!("{:02}:{:02}:{:02}.{:09}", secs / 3600, secs / 60 % 60, secs % 60, ns)
}

/// The first video track in `tracks`, and its UID element.
fn video_track_uid(buf: &[u8], tracks: &Element) -> Result<Option<(Element, Element)>> {
    for entry in tracks.children(buf) {
        let entry = entry?;
        if entry.id != ids::TRACK_ENTRY {
            continue;
        }
        let mut video = false;
        let mut uid = None;
        for field in entry.children(buf) {
            let field = field?;
            match field.id {
                ids::TRACK_TYPE => video = ebml::read_uint(buf, &field)? == 1,
                ids::TRACK_UID => uid = Some(field),
                _ => {}
            }
        }
        if video {
            return Ok(uid.map(|uid| (entry, uid)));
        }
    }
    Ok(None)
}

/// Turns a CRC-32 child of `parent` into a Void of the same size. Used after
/// changing bytes the checksum covers.
fn neutralise_crc(buf: &mut [u8], parent: &Element) -> Result<()> {
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
            neutralise_crc(buf, &seek)?;
            patched = true;
        }
        if patched {
            neutralise_crc(buf, seek_head)?;
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
    // a float Duration holds any up to MAX_FAKE_DURATION, at any scale
    let duration_units = patch.duration.map(|secs| secs * 1e9 / scale);

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
                neutralise_crc(buf, parent)?;
            }
            tags_patched += 1;
        }
    }
    if let Some(uid) = patch.track_uid {
        for tracks in segment.children.iter().filter(|el| el.id == ids::TRACKS) {
            if let Some((entry, field)) = video_track_uid(buf, tracks)?
                && field.end - field.data_start == 8
            {
                let old = ebml::read_uint(buf, &field)?;
                buf[field.data_start..field.end].copy_from_slice(&uid.to_be_bytes());
                neutralise_crc(buf, &entry)?;
                neutralise_crc(buf, tracks)?;
                // tags name their track by its UID
                for tags in segment.children.iter().filter(|el| el.id == ids::TAGS) {
                    for (target, parents) in tag_track_uids(buf, tags)? {
                        if ebml::read_uint(buf, &target)? == old
                            && target.end - target.data_start == 8
                        {
                            buf[target.data_start..target.end].copy_from_slice(&uid.to_be_bytes());
                            for parent in &parents {
                                neutralise_crc(buf, parent)?;
                            }
                        }
                    }
                }
            }
        }
    }
    // a Segment checksum covers everything changed above
    neutralise_crc(buf, &segment.el)?;

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
/// Without `replace`, an existing output is
/// [`Error::OutputExists`](crate::Error::OutputExists).
pub fn patch_file(
    input: &Path,
    output: &Path,
    changes: &Patch,
    replace: bool,
) -> crate::Result<PatchReport> {
    let mut buf = crate::art::read(input)?;
    let report = patch(&mut buf, changes)?;
    crate::fsutil::write_output(output, &buf, replace)?;
    Ok(report)
}
