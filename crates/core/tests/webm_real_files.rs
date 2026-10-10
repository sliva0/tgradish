//! Checks WebM inspection and patching on files written by a real ffmpeg.
//! Skipped when ffmpeg or the local `references/` media is missing.

use std::path::{Path, PathBuf};
use std::process::Command;

use tgradish_core::webm::{self, Patch};

fn reference(name: &str) -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../references").join(name);
    let ffmpeg = Command::new("ffmpeg").arg("-version").output().is_ok();
    if !(ffmpeg && path.exists()) {
        eprintln!("skipped: needs ffmpeg on PATH and references/{name}");
        return None;
    }
    Some(path)
}

fn encode(input: &Path, output: &Path, seconds: &str) {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-t", seconds, "-i"])
        .arg(input)
        .args(["-an", "-vf", "scale=512:512:force_original_aspect_ratio=decrease,format=yuva420p"])
        .args(["-c:v", "libvpx-vp9", "-b:v", "300k", "-cpu-used", "8", "-row-mt", "1"])
        .arg(output)
        .status()
        .unwrap();
    assert!(status.success());
}

fn ffprobe(path: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", entries, "-of", "default=nw=1:nk=1"])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn decodes_cleanly(path: &Path) -> bool {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-c:v", "libvpx-vp9", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    out.status.success() && out.stderr.is_empty()
}

#[test]
fn spoofs_and_watermarks_ffmpeg_output() {
    for name in ["pig.mp4", "uhh.mp4"] {
        let Some(input) = reference(name) else { return };
        let dir = tempfile::tempdir().unwrap();
        let encoded = dir.path().join("encoded.webm");
        let patched = dir.path().join("patched.webm");
        encode(&input, &encoded, "4");

        let before = webm::inspect_file(&encoded).unwrap();
        assert_eq!(before.doc_type, "webm");
        let video = before.video.as_ref().unwrap();
        assert_eq!(video.codec_id, "V_VP9");
        assert!(video.alpha);
        assert_eq!(video.width.max(video.height), 512);
        assert_eq!(before.video_frames, 100, "{name}: 4 s at 25 fps");
        let header = before.header_duration.unwrap();
        let content = before.content_duration.unwrap();
        assert!((header - 4.0).abs() < 0.05, "{name}: header duration {header}");
        assert!((content - 4.0).abs() < 0.05, "{name}: content duration {content}");

        let changes = Patch {
            duration: Some(0.42069),
            title: Some(format!("{name} sticker")),
            muxing_app: Some(tgradish_core::TOOL_ID.into()),
            writing_app: Some(tgradish_core::TOOL_ID.into()),
            signature: Some(format!("{} test signature", tgradish_core::TOOL_ID)),
            track_uid: Some(0x0123_4567_89ab_cdef),
        };
        let report = webm::patch_file(&encoded, &patched, &changes, false).unwrap();
        assert!(report.signature_written);
        let uid = webm::inspect_file(&patched).unwrap().video.unwrap().uid;
        assert_eq!(uid, Some(0x0123_4567_89ab_cdef), "{name}");
        assert_eq!(report.duration_tags_patched, 1);

        let after = webm::inspect_file(&patched).unwrap();
        assert_eq!(after.file_size, before.file_size);
        assert_eq!(after.video_frames, before.video_frames);
        assert_eq!(after.content_duration, before.content_duration);
        assert!((after.header_duration.unwrap() - 0.42069).abs() < 1e-6);
        assert_eq!(after.writing_app.as_deref(), Some(tgradish_core::TOOL_ID));
        assert!(after.signature.unwrap().ends_with("test signature"));

        // other tools see the patched metadata and can still read the file
        let duration: f64 = ffprobe(&patched, "format=duration").trim().parse().unwrap();
        assert!((duration - 0.42069).abs() < 0.001, "{name}: ffprobe duration {duration}");
        assert!(ffprobe(&patched, "format_tags=title").contains(&format!("{name} sticker")));
        assert!(ffprobe(&patched, "stream_tags=DURATION").contains("00:00:00.420690000"));
        assert!(decodes_cleanly(&patched), "{name}: patched file has decoding errors");

        // patching in place gives the same result
        webm::patch_file(&encoded, &encoded, &changes, true).unwrap();
        assert_eq!(std::fs::read(&encoded).unwrap(), std::fs::read(&patched).unwrap());
    }
}
