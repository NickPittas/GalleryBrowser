# GalleryBrowser Design

Rust workspace (`crates/*`, `apps/*`). Dependencies point downward only; lower layers never touch egui.

## Layering

| Layer | Crate | Responsibility |
| --- | --- | --- |
| L0 | `core` | `Entry`, media classification, folder scan, sequence detection, natural sort, `Settings`, `AppDirs` |
| L1 | `fileops` | File-operation worker, conflict policy, rename planning, undo |
| L1 | `media` | Video/still/PSD access, EXIF, thumbnails, ComfyUI PNG metadata |
| L2 | `library` | Tags, collections, ratings on `lightcraft-catalog`; legacy SQLite import |
| L3 | `player` | Playback engine (decode-ahead, RAM-budgeted cache, audio clock) |
| L5 | `ui-egui` | egui frontend |
| app | `apps/gallerybrowser` | Binary, single-instance forwarding over a unix socket (`AppDirs::instance_socket`) |

`media` uses the ArtCraft pure-Rust decoders (no ffmpeg/GStreamer): video via `filmcraft-codecs` with streaming
readers (a whole video is never read into RAM), stills via `filmcraft-media`/`photocraft-codecs`, PSD/PSB via
`photocraft-psd`, EXIF via `lightcraft-meta`, thumbnails via `lightcraft-preview`.

## Entry identity

Every browsable item is an `Entry { path, name, kind, size, modified, sequence }`. `path` is the stable identity
(selection, library keys, texture/thumbnail keys). For an image sequence it is the *pattern path*
(`dir/shot_####.exr`), never a frame; `size` is the sum and `modified` the newest over frames. Anything that
touches files MUST use `Entry::member_paths()` (every frame of a sequence), so operations never act on one frame only.

## Actions and the refresh pipeline

Panels never mutate domain state. They read `AppState` and push `Action`s; `actions::dispatch_pending` applies
queued actions (a bounded number of rounds per frame, since actions may enqueue more). Keyboard shortcuts are
read in `shortcuts::handle`, and the file pane and fullscreen viewer handle their own keys.
`actions::refresh_view` is the single pipeline that rebuilds the visible list:
scan -> scope/library filter -> search/kind filter -> sort -> selection reconcile.
Every change to the folder, filters, sort or file-op results goes through it.

## Worker threads

| Thread(s) | Work |
| --- | --- |
| fileops worker | copy/move/trash/delete/rename; reports moved/created/removed paths via `OpEvent`/`OpOutcome` |
| thumbnailer pool | `Settings::thumbnail_threads` decoder threads, priority queue, results polled by the UI |
| metadata | EXIF / ComfyUI / media info for the info pane, off the UI thread |
| preview opener (`Loader`) | `media::open`/`open_sequence` or a full still decode; newest request wins (generation counters) |
| player decode | decode-ahead workers per `Player`; generation counters drop stale frames after `load`/`seek` |
| audio | cpal output stream; the audio device is the master clock when the source has audio |

The UI thread only polls results and uploads textures.

## Caches

- **Thumbnails**: in-memory cache (256 MiB) plus a disk cache under `~/.cache/gallerybrowser/thumbnails`
  (budget `max_cache_mb`), keyed by path, size, mtime and edge.
- **Textures**: egui texture LRU keyed by (`Entry::path`, max edge), bounded to 2000 textures / 384 MiB of RGBA.
- **Player frames**: RAM-budgeted cache keyed by frame index (`ram_preview_mb`). Eviction is by distance from the
  playhead, behind before ahead; frames in the wanted window are never evicted.

## Playback

`Player::tick(now)` returns a frame only when the displayed frame changes. With audio at normal speed the audio
device is the clock; at speed != 1 audio is muted and a system clock drives playback. Device failure degrades to
silent playback and sets `PlayerState::error`. Volume, mute, speed and loop survive `load`. At the end playback
loops or pauses on the last frame.

## Library persistence

Tags (catalog keywords), collections (catalog albums) and ratings (0-5) are keyed by `Entry::path` and stored with
`lightcraft-catalog` (journal + snapshots) in `~/.local/share/gallerybrowser/library`. Files enter the catalog only
when annotated. Each mutation is one catalog `Op` (a `Batch` for several steps), so it is atomic and crash-safe.
Tag names cannot contain `|`. When file operations finish, the UI applies `Library::relink` for moves/renames
(directories relink everything below) and `Library::forget` for removed paths. A one-time, read-only import brings in
the Python-era SQLite database; missing tables or undecodable rows are skipped.

## Settings (`settings.json`)

Unknown keys are ignored, missing keys default, wrong-typed values keep their default, out-of-range values are
clamped. Saves are atomic (temp file, fsync, rename).

| Key | Default | Range / note |
| --- | --- | --- |
| `thumbnail_size` | 128 | 64, 128, 256, 512 |
| `show_hidden` | false | |
| `video_preview` | true | auto-play on selection |
| `thumbnail_threads` | 2 | 1..=16 |
| `max_cache_mb` | 1000 | disk thumbnail cache |
| `ram_preview_mb` | 2048 | player frame cache |
| `collapse_sequences` | true | |
| `sequence_fps` | 24.0 | 1..=240 |
| `view_mode` | Grid | Grid / List |
| `sort_key`, `sort_ascending` | Name, true | |
| `show_preview` | true | |
| `theme` | Dark | Dark / Light / System |
| `volume`, `muted` | 1.0, false | volume 0..=1 |
| `loop_playback` | true | |
| `favorites` | [] | deduplicated |
| `recents`, `recents_max` | [], 15 | newest first; max 1..=100 |
| `last_folder` | none | |

## Error handling

- No `unwrap`/`expect`/`panic`/`unreachable`/`unimplemented` (denied by workspace clippy lints); `unsafe` is forbidden.
- Library crates return typed errors (`thiserror`); the UI surfaces them as status messages, never crashes.
- Settings load never fails (corrupt file -> defaults, logged).
- File operations never overwrite: `ConflictPolicy::Skip` reports the skipped source; `KeepBoth` picks a free name
  (`name (1).ext`). Failures are reported per path in the op outcome.
- Stale async results (thumbnails, previews, frames) are discarded by generation counters rather than shown.
