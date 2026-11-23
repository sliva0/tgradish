/* Empty stub for MinGW-w64 releases before 12, which lack d3d12video.h.
 * ffmpeg's hwcontext_d3d12va.h includes it, and ffmpeg-sys-next defines all
 * required declarations itself, so the header only needs to exist on
 * bindgen's include path (see xtask/src/build.rs). */
