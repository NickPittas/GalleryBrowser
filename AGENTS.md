# GalleryBrowser — rules for agents and contributors

GalleryBrowser is a Linux asset browser for video, images and image sequences, written in Rust on the
ArtCraft stack (egui/eframe 0.36 + wgpu, pure-Rust media from FilmCraft/PhotoCraft/LightCraft). These
rules follow the ArtCraft apps' `AGENTS.md`.

## 0. Never crash

A malformed video, an unreadable folder, a full disk or an odd keystroke must produce an error the user
can act on (status bar or dialog), never a panic and never lost data.

1. Fail with `Result`; propagate with `?`; add context. In the UI, report and carry on.
2. No `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!` outside tests
   (enforced by `[workspace.lints.clippy]`).
3. No `unsafe` (`unsafe_code = "forbid"`).
4. Every number derived from a file is hostile: `get()` for indexing, `checked_*`/`saturating_*`
   arithmetic, never divide by a frame rate/size that can be zero, cap allocations sized by input.
5. Lock poisoning is not fatal: `lock().unwrap_or_else(PoisonError::into_inner)`. Every worker thread
   runs its jobs under `catch_unwind` and reports failures as errors.
6. Never read a whole video into memory: open containers through streaming readers.

## 1. Layering

| Layer | Crate | May depend on |
|---|---|---|
| L0 | `gallerybrowser-core` (classification, scan, sequences, settings, dirs) | std, small utils |
| L1 | `gallerybrowser-fileops`, `gallerybrowser-media`, `gallerybrowser-library` | core, ArtCraft crates |
| L2 | `gallerybrowser-player` | media |
| L5 | `gallerybrowser-ui-egui` | everything above, egui |
| app | `apps/gallerybrowser` | ui-egui, core |

Nothing below L5 touches egui. UI panels read `AppState` and push `Action`s; `actions::dispatch`
applies them; `actions::refresh_view` is the only path that rebuilds the visible entry list.

## 2. Identity and file operations

- An `Entry::path` is the identity of a browsable item; for image sequences it is the pattern path
  (`dir/shot_####.exr`). File operations act on `Entry::member_paths()` (every frame).
- File operations run on the fileops worker thread, never the UI thread, and never silently overwrite.
- Every move/rename result is applied to the library with `Library::relink`; deletions with `forget`.

## 3. Third-party code

ArtCraft crates are pinned git dependencies (MIT OR Apache-2.0); see `NOTICE`. Code adapted from
ArtCraft sources (e.g. the monitor zoom/pan math) keeps a comment naming the source file. Never copy
ArtCraft brand assets. Dependencies must be permissively licensed.

## 4. Checks

`cargo fmt --all`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`.
