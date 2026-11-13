//! Inspecting and patching synthetic WebM files.

use super::ebml::build::*;
use super::ebml::ids;
use super::inspect::*;
use super::patch::*;
use super::*;
use crate::Error;

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
    let report = patch(&mut buf, &Patch { duration: Some(0.42069), ..Default::default() }).unwrap();

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
        track_uid: None,
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
    let info =
        el(ids::INFO, &concat(&[uint(ids::TIMESTAMP_SCALE, scale), f64_el(ids::DURATION, 1000.0)]));
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
    let segment =
        if unknown_sizes { el_unknown_size(ids::SEGMENT, &body) } else { el(ids::SEGMENT, &body) };
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
fn reads_the_apps_from_the_head_alone() {
    let mut buf = Fixture::default().build();
    let changes = Patch {
        muxing_app: Some("tgradish 2.0.0".into()),
        writing_app: Some("tgradish 2.0.0 (libvpx-vp9)".into()),
        signature: Some("tgradish 2.0.0 was here".into()),
        ..Default::default()
    };
    patch(&mut buf, &changes).unwrap();
    let whole = head(&buf).unwrap();
    assert_eq!(whole.muxing_app, changes.muxing_app);
    assert_eq!(whole.writing_app, changes.writing_app);
    assert_eq!(whole.signature, changes.signature);
    // as much as is read of a longer file, cut in a block
    let cluster = inspect(&buf).unwrap().file_size as usize - 20;
    assert_eq!(head(&buf[..cluster]).unwrap(), whole);
    assert!(matches!(head(b"\x00\x00ftyp"), Err(WebmError::Ebml(_) | WebmError::NotEbml)));
}

#[cfg(unix)]
#[test]
fn reads_no_more_of_large_files_than_it_needs() {
    let dir = tempfile::tempdir().unwrap();
    // without blocks on the disk, as long as a long video
    let sparse = |name: &str, start: &[u8]| {
        let path = dir.path().join(name);
        std::fs::write(&path, start).unwrap();
        std::fs::File::options().write(true).open(&path).unwrap().set_len(8 << 30).unwrap();
        path
    };
    let video = sparse("video.mp4", b"\x00\x00\x00\x18ftypisom");
    let not_ebml = |result| matches!(result, Err(Error::Webm(WebmError::NotEbml)));
    assert!(not_ebml(inspect_file(&video).map(drop)));
    assert!(not_ebml(head_file(&video).map(drop)));
    let long = sparse("long.webm", &Fixture::default().build());
    assert!(matches!(inspect_file(&long), Err(Error::Webm(WebmError::TooLarge))));
    // its start, which is a WebM's, reads
    assert_eq!(head_file(&long).unwrap().muxing_app.as_deref(), Some("Lavf63.1.102"));
    let empty = dir.path().join("empty.webm");
    std::fs::write(&empty, b"").unwrap();
    assert!(not_ebml(inspect_file(&empty).map(drop)));
}

#[test]
fn patch_file_leaves_other_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.webm");
    // named as write_file names its own temporary files
    let bystander = dir.path().join(".tgradish-AbCd12");
    std::fs::write(&bystander, b"keep me").unwrap();
    let input = dir.path().join("in.webm");
    std::fs::write(&input, Fixture::default().build()).unwrap();

    let changes = Patch { duration: Some(0.5), ..Default::default() };
    patch_file(&input, &output, &changes, false).unwrap();
    assert_eq!(std::fs::read(&bystander).unwrap(), b"keep me");
    let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(leftovers, 3, "no temporary files left behind");
}
