/* Empty stand-in for MinGW-w64 releases before 12, which lack this header.
 * ffmpeg's hwcontext_d3d12va.h includes it, and ffmpeg-sys-next's bindings
 * define everything they need from it themselves, so only the file has to
 * exist. Used for Windows release builds, see .github/workflows/release.yml. */
