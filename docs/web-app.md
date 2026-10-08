# Telegram bot and web app

Goal: use tgradish from inside Telegram, as a Mini App (a web page Telegram
opens in its own WebView) together with a bot, and from a normal browser.
Ideally the bot also puts the results straight into the user's sticker
packs.

Investigated 2026-10-08. Nothing here is built yet.

## What the platform allows

### Mini Apps

From <https://core.telegram.org/bots/webapps>:

- A Mini App is a normal web page shown in a WebView in every client
  (Android, iOS, Desktop, web). The bot backend authenticates it by
  checking the HMAC of `initData` with the bot token.
- **Files in:** Telegram has no file API; the page uses a normal
  `<input type="file">`. Telegram's bug tracker has reports of it not
  working on Android (fixed in 2025) and of Android ignoring `multiple` and
  `capture` ([report](https://bugs.telegram.org/c/33777)). Must be tested
  on current clients.
- **Files out:**
  - `downloadFile` (Bot API 8.0+) shows a native download prompt, but only
    for an HTTPS URL whose server sends `Content-Disposition: attachment` and
    allows CORS from `https://web.telegram.org`. A result made in the page
    has to be uploaded somewhere first.
  - `sendData` closes the app and sends at most 4096 bytes to the bot: too
    small for a sticker.
  - The practical way to hand over a sticker is through the bot: the page
    uploads the file to the backend, and the bot sends it to the chat or
    adds it to a pack.
- **Storage:** CloudStorage (1024 items of up to 4096 characters per user),
  DeviceStorage (5 MB per user, like `localStorage`), SecureStorage
  (10 items, OS keychain).

### Bot API sticker sets

From <https://core.telegram.org/bots/api#stickers>:

- A bot can create a set owned by a user (`createNewStickerSet` with
  `user_id`) and keeps the right to edit it. The set name must end in
  `_by_<bot_username>`.
- 1–50 stickers in the creating call; regular sets hold up to 120
  stickers, custom emoji sets up to 200. Formats can be mixed per sticker
  (static, animated `.tgs`, video `.webm`).
- Animated and video stickers can't be given by URL; they are uploaded with
  `uploadStickerFile` or as multipart. Bot uploads are limited to 50 MB
  and downloads of user files (`getFile`) to 20 MB, both far above sticker
  sizes but relevant for source videos sent to the bot.

### Telegram Serverless

New platform for bot backends on Telegram's infrastructure
(<https://core.telegram.org/bots/serverless>):

- plain JavaScript ES modules in V8 isolates, no npm packages, no
  filesystem; WebAssembly is not mentioned, and no CPU, memory or time
  limits are documented;
- Bot API access (uploads up to 50 MB, `getFile` up to 20 MB), a SQLite
  database, outbound `fetch` (responses capped at 30 MB);
- hosts the Mini App's static files at `app<id>.tgcloud.ai` with custom
  headers, and validates `initData` automatically for
  `Telegram.WebApp.Serverless.call` endpoints.

Good fit for the bot part (upload stickers, create and edit packs, send
results), and possibly for hosting the page. Not a place to run video
encoding.

## Converting in the browser

- **`.tgs`:** `tgradish-tgs` is planned as pure Rust that builds for
  WebAssembly, so pixel art to `.tgs` runs in the page at near-native
  speed. No open questions beyond file input.
- **`.webm` with ffmpeg compiled to WebAssembly:** works everywhere
  WebAssembly does, but slowly. ffmpeg.wasm's own docs call even the
  multithreaded build significantly slower than native; third parties
  estimate 10–50 times, and an older libvpx VP8 port measured about 8
  times. A sticker that takes 5–15 s natively with `--fit auto` could take
  minutes. Threads need `SharedArrayBuffer`, which needs cross-origin
  isolation (COOP and COEP headers); whether Telegram's WebViews allow that
  is unknown. GitHub Pages can't send those headers (a service worker
  workaround exists); tgcloud hosting can.
- **`.webm` with WebCodecs:** the browser's own VP9 encoder, often hardware
  accelerated, so fast. But VP9 support is optional for browsers, and
  keeping transparency (`alpha: "keep"`) is not widely supported ([MDN](
  https://developer.mozilla.org/en-US/docs/Web/API/VideoEncoder/configure));
  stickers usually need it. There is also no two-pass mode, so fitting
  becomes several one-pass encodes.
- Decoding the input in the page: WebCodecs' `VideoDecoder`/`ImageDecoder`,
  or a `<video>` element drawn into a canvas.

## Converting on a server

- tgradish's library and CLI run as they are on a VPS; video goes to the
  bot directly or through the page.
- Costs money and needs abuse limits if it is public: queueing, per-user
  limits, input size and time caps. The earlier plan was a self-hosted bot
  with allowlisted users.

## Options

1. **All in the browser:** a static page; `.tgs` in WebAssembly, `.webm`
   with WebCodecs where it supports what is needed and ffmpeg in
   WebAssembly otherwise. A small bot backend (Serverless JS, or a Rust bot)
   only for putting results into packs. No conversion server.
2. **Server converts:** the page is a thin UI; a self-hosted bot does the
   work. Best quality and speed, needs hosting.
3. **Both:** browser by default, a server mode for heavy jobs when the
   operator runs one. Most work.

## To test first

A throwaway Mini App opened in current Android, iOS, Desktop and web
clients that reports:

- whether `<input type="file">` works, with and without `multiple`;
- `crossOriginIsolated` and `SharedArrayBuffer` when served with COOP/COEP;
- WebAssembly SIMD and threads;
- WebCodecs: `VideoEncoder.isConfigSupported` for VP9 with
  `alpha: "keep"`;
- how long a short libvpx encode in WebAssembly takes on a phone;
- whether `downloadFile` works with a file the backend just stored.
