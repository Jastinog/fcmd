# Archive module — improvement & hardening plan

Self-contained plan for improving `src/archive/`. Written so a fresh session needs
no prior context. Work top-to-bottom; tasks are ordered by value and dependency.

## Current architecture (orientation)

Two layers, one-way dependency (`app` → `archive`, never the reverse):

- **`src/archive/`** — pure library, synchronous, UI-agnostic. Talks to the outside
  world only through callbacks and a cancel flag.
  - `mod.rs` — shared types: `ArchiveFormat` (Zip, Tar, TarGz, TarBz2, TarXz),
    `ArchiveEntry`, `is_archive`, `ConflictDecision`, `ExtractOutcome`,
    `ProgressFn`/`ConflictFn` type aliases, `never_cancel()`.
  - `extract.rs` — `list_archive`, `extract_entry`, `extract_all`, `extract_stream`
    (the real path: progress + conflict resolution + cancel), plus zip/tar internals
    and `sanitize_archive_path` (zip-slip guard).
  - `create.rs` — `create_archive`, `create_stream`; `CompWriter` (Gz/Bz2/Xz/plain)
    and `ArchiveSink` (zip vs tar) abstract the output format.
- **`src/app/archive.rs`** — UI/controller. Runs the library in `spawn_blocking`,
  bridges its callbacks onto Tokio channels and the task-manager: throttled progress
  via `try_send`, conflicts via `ConflictInfo` + `oneshot` reply, cancel via
  `Arc<AtomicBool>`.

Key invariants to preserve in any change:
- Progress is **lossy/throttled** (`PROGRESS_INTERVAL`); only `Finished` carries
  authoritative counts.
- Cancellation is checked **between entries** (an in-flight file is not interrupted) —
  Task 4 changes this deliberately.
- Each task owns its own conflict channel (`app/archive.rs:418`).
- zip-slip is neutralized by `sanitize_archive_path` before `dest.join`.

---

## Task 1 — Preserve permissions; handle symlinks/special entries  🔴 correctness (data corruption)

**Problem.** Both zip and tar extraction write every non-dir entry with
`File::create` + `io::copy` (`extract.rs:272-273` and `:422-423`), ignoring the
entry's mode bits and type. Consequences:
- Executable bit is lost → extracted scripts/binaries won't run.
- In **tar**, a symlink/hardlink entry is matched by the `else` branch and becomes an
  **empty regular file** (symlink payload is empty). Device/fifo entries too.
- The create side is symmetric: zip packs with `SimpleFileOptions::default()`
  (`create.rs:126,143`) so modes are dropped; symlinks are followed and stored as
  content on both zip and tar (`add_file` opens + copies, `create.rs:141-153`).

**Do NOT** replace the extraction loop with `tar::Entry::unpack()` — that would bypass
the per-entry conflict callback, progress reporting, and cancel checks. Keep the loop;
extend it.

**Approach (extract):**
- *tar* (`extract_tar_stream`, `extract.rs:360`): branch on
  `entry.header().entry_type()`:
  - `Symlink`/`GNULongLink` → `entry.link_name()` then
    `std::os::unix::fs::symlink(target, out_path)` (guard the symlink target through
    `sanitize_archive_path` semantics / reject absolute escapes).
  - `Hardlink` → resolve against `dest`, `std::fs::hard_link`.
  - regular file → after `io::copy`, apply mode via
    `std::fs::set_permissions(out_path, Permissions::from_mode(entry.header().mode()?))`.
  - keep dir branch; optionally apply dir mode.
- *zip* (`extract_zip_stream`, `extract.rs:207`): after writing the file, if
  `entry.unix_mode()` is `Some(m)` → `set_permissions(..., from_mode(m))`. zip also
  encodes symlinks via unix mode `S_IFLNK`; detect and recreate as symlink.
- Gate all unix-specific code behind `#[cfg(unix)]`; on non-unix just skip mode/symlink
  handling (current behavior).

**Approach (create, optional symmetry):**
- zip: set `SimpleFileOptions::default().unix_permissions(mode)` per entry from
  source metadata; store symlinks as symlink entries instead of following.
- tar: `append_file` already derives mode from the open file's metadata; add explicit
  symlink handling via `Header::set_entry_type(Symlink)` + `append_link` so links round-trip.

**Files:** `src/archive/extract.rs` (primary), `src/archive/create.rs` (symmetry),
maybe a small `#[cfg(unix)]` helper in `mod.rs`.

**Tests:** unix-only round-trip tests — pack a tree containing an executable file and a
symlink → extract → assert mode bits preserved and symlink is a symlink pointing at the
right target. Add a "symlink does not escape dest" test.

**Acceptance:** extracted executables keep `+x`; tar symlinks survive as symlinks;
no regression in existing extract tests; non-unix builds still compile.

---

## Task 2 — Deduplicate conflict-resolution logic  🟡 quality

**Problem.** `fs/ops.rs` already has clean reusable pieces — `ConflictPolicy`
(`fs/ops.rs:103`) and `resolve_file_conflict()` (`:225`) implementing
overwrite_all / skip_all / OverwriteNewer. The archive extract path reimplements the
exact same state machine inline (`app/archive.rs:439-485`), translating
`ConflictChoice` ↔ `ConflictDecision` by hand.

**Approach.** Extract a shared helper that both the paste path and the archive path use.
Note the two paths differ slightly in the channel type (`ProgressMsg`/`ConflictInfo` vs
`ArchiveMsg`), so factor the **policy/decision** core, not the whole closure:
- Option A: make `resolve_file_conflict` (or a new `ConflictPolicy::decide(choice, src_mod, dst_mod)`)
  public within `crate::fs::ops` and have `app/archive.rs` call it, mapping the returned
  bool/decision to `ConflictDecision`.
- Keep `ConflictDecision` (library-facing) and `ConflictChoice` (UI-facing) as distinct
  types; add one small `From`/mapping fn in `app/archive.rs`.

**Files:** `src/fs/ops.rs` (expose helper), `src/app/archive.rs` (consume it).

**Tests:** existing `extract_stream_skip_vs_overwrite_on_conflict`,
`extract_stream_resolver_only_called_on_existing`, `extract_stream_abort_stops_early`
must still pass; add a unit test for the shared decide() covering OverwriteNewer both ways.

**Acceptance:** the inline state machine in `app/archive.rs:439-485` is gone; behavior
identical; one source of truth for conflict policy.

---

## Task 3 — Replace hand-rolled date conversion with `chrono`  🟡 quality

**Problem.** `zip_datetime_to_systemtime` (`extract.rs:179-204`) is hand-written day-count
arithmetic, explicitly commented "not used for any correctness-sensitive comparison",
with shaky leap-year handling. `chrono = "0.4"` is **already a dependency** and already
used in `archive/mod.rs` and elsewhere.

**Approach.** Build a `chrono::NaiveDate`/`NaiveDateTime` from the zip `DateTime`
fields and convert to `SystemTime` (UTC). Delete the manual arithmetic.

**Files:** `src/archive/extract.rs`.

**Tests:** add a test asserting a known zip DateTime maps to the expected `SystemTime`
(e.g. 2021-06-15 12:00:00). Confirm `list_and_extract_zip` still passes.

**Acceptance:** ~25 lines of bespoke math removed; dates correct across leap years.

---

## Task 4 — Cancel mid-file for large entries  🔴 responsiveness

**Problem.** `cancel` is only checked between entries (`extract.rs:226,377`; create
between files). A single huge file can't be cancelled.

**Approach.** Replace `io::copy(&mut entry, &mut out_file)` with a manual buffered loop
(e.g. 64–256 KiB buffer): read → check `cancel.load(Relaxed)` → write. On cancel, set
`outcome.cancelled = true`, optionally remove the partial output file, and break. Apply
the same pattern in `create.rs` `add_file`. Consider a small
`copy_with_cancel(reader, writer, cancel)` helper shared by both files.

**Files:** `src/archive/extract.rs`, `src/archive/create.rs`.

**Tests:** extract/create a large-ish file with a cancel flag flipped after the first
chunk; assert it stops promptly and reports `cancelled`.

**Acceptance:** flipping cancel during a multi-hundred-MB entry stops within one buffer
read; no partial garbage left behind.

---

## Task 5 — Reduce `extract_stream` arg count  🟢 polish

**Problem.** `extract_stream` / `extract_zip_stream` / `extract_tar_stream` take 7–8 args
and carry `#[allow(clippy::too_many_arguments)]` (`extract.rs:206`).

**Approach.** Introduce `struct ExtractRequest<'a> { archive_path, filter, dest, total }`
(plain data) and pass `&mut ProgressFn`, `&mut ConflictFn`, `&AtomicBool` separately
(they're not data). Thread it through the three fns; drop the clippy allow.

**Files:** `src/archive/extract.rs`, callers in `src/app/archive.rs`.

**Acceptance:** no clippy allow needed; call sites read clearly.

---

## Task 6 — Format detection by magic bytes (fallback)  🟢 feature

**Problem.** `ArchiveFormat::from_path` keys only on the filename extension; an archive
without/with a wrong extension won't open or pack-detect.

**Approach.** Add `ArchiveFormat::sniff(path)` reading the first few bytes:
`50 4B 03 04` → Zip, `1F 8B` → gzip, `42 5A 68` → bzip2, `FD 37 7A` → xz, tar magic
`ustar` at offset 257. Use extension first, fall back to sniff in `list_archive` /
`extract_stream` / `is_archive`.

**Files:** `src/archive/mod.rs`, `src/archive/extract.rs`.

**Tests:** rename a `.zip` to `.bin`, assert it still lists/extracts.

**Acceptance:** extension-less / misnamed archives open correctly; detection prefers
extension, sniff only as fallback.

---

## Task 7 — Add zstd support (`.tar.zst`)  🟢 feature (optional)

**Problem.** No zstd, a very common modern format. Architecture is ready:
`TarCompression` (`extract.rs:283`) and `CompWriter` (`create.rs:76`) just need a variant.

**Approach.** Add `zstd` crate dependency; add `ArchiveFormat::TarZst` + `.tar.zst`/`.tzst`
to `from_path`/`label`; add `TarCompression::Zst` (decoder in `open_tar_reader`) and
`CompWriter::Zst` (encoder). Wire into the format match arms.

**Files:** `Cargo.toml`, `src/archive/mod.rs`, `extract.rs`, `create.rs`.

**Tests:** round-trip a `.tar.zst`.

**Acceptance:** create/list/extract `.tar.zst` works; 7z/rar explicitly out of scope.

---

## Suggested sequencing

1. **Task 1** (data-corruption bug) — highest value, do first.
2. **Task 2** (dedup) — small, removes a foot-gun before further edits.
3. **Task 3** (chrono) — quick, independent.
4. **Task 4** (mid-file cancel) — independent of 1–3.
5. **Task 5** (arg struct) — cosmetic, do whenever touching extract.
6. **Task 6 / 7** (features) — only if desired.

Tasks 1–5 are well covered by the existing test suite (664 tests, archive round-trip /
zip-slip / conflict / progress / cancel), so regressions surface immediately. Run
`cargo test` and `cargo clippy` after each task. Commit per task (no `Co-Authored-By`
lines — see CLAUDE.md).

## Quick start for the next session

> Read `docs/archive-improvements-plan.md` and implement Task 1 (preserve permissions and
> handle symlinks on extract). Keep the streaming loop; don't switch to `unpack()`.
