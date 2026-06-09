# UI/UX Fix Plan

Tracking file for UI/UX issues found during the full-codebase review.
Work top to bottom. Check off items as completed.

Legend: `[ ]` todo · `[~]` in progress · `[x]` done

---

## 🔴 P0 — Critical (dangerous defaults, data loss, broken modals)

- [x] **P0.1 Delete confirm: remove `Enter` as confirm** — `src/app/dialogs.rs:10`
  `Enter` is bound alongside `y`/`Y` and immediately deletes, while `j/k/↑/↓` scroll the
  same list. Drop `KeyCode::Enter` from the confirm arm; require explicit `y`.

- [x] **P0.2 Conflict dialog: safe default (Skip, not Overwrite)** — `src/app/polling.rs:56`, `src/app/dialogs.rs:70-71`
  `conflict_selected = 0` and index 0 = `Overwrite`. Initialize selection to the Skip
  index so a reflexive Enter does not clobber files.

- [x] **P0.3 Second paste orphans first conflict channel** — `src/app/file_ops.rs:153`
  Done: `conflict_rx: Option<_>` → `conflict_rxs: Vec<_>`, polled round-robin in
  `poll_conflicts`. (Note: not a true deadlock — `ask_conflict` aborts gracefully on a
  dropped receiver — but the old code made the first paste silently skip conflicts.)
  `self.conflict_rx` is overwritten on each paste; a second paste started while the first
  awaits a conflict response deadlocks the first background task forever. Queue conflict
  channels per task, or block starting a new paste while one awaits conflict resolution.

- [x] **P0.4 Quit warning while tasks run** — `src/main.rs:225`, `src/app/file_ops.rs`, `src/app/task_manager.rs`
  Done: added `Mode::ConfirmQuit` + `request_quit()`; all quit paths (`q`, F10, tree `q`,
  `:q`/`:quit`) route through it; `:q!` force-quits. Also fixed a latent bug where
  `handle_command` reset mode to Normal *after* execute_command, breaking
  `:find`/`:bulkrename`/`:bookmarks`.
  Quit proceeds unconditionally even with `active_count() > 0`, silently aborting
  copy/move/delete. Prompt "N tasks still running — quit anyway?".

- [x] **P0.5 Task-completion notification lost on next keypress** — `src/app/polling.rs:35-40`, `src/app/mod.rs:840`
  `handle_key` clears `task_notification` every keypress; `poll_tasks` sets it only when
  `active_count()==0` and immediately `remove_finished()`. Completion summaries (incl.
  failures) can vanish in one frame. Keep notification until dismissed / for a timed
  duration; don't `remove_finished()` in the same poll that first surfaces the summary.

---

## 🟠 P1 — Significant (wide-char rendering, broken modals on small terminals, inconsistencies)

### Rendering / wide-char (CJK, emoji, nerd-font)

- [x] **P1.1 Tree truncation by char count** — `src/ui/tree.rs:62-104`
  Uses `chars().count()` instead of display width; wide names overflow and misalign.
  Switch to `util::truncate_to_width` / `display_width`. Also pad cursor line to full width.

- [x] **P1.2 Find-overlay truncation by char count** — `src/ui/find_overlay.rs:136-144`, `:227-260`
  Same char-count issue for results, hex preview, and title. Use display-width helpers.

- [x] **P1.3 Text-input horizontal scroll by char count** — `src/app/input.rs:94`, `src/app/command.rs:47`, `src/app/search.rs:65`, `src/app/bulk_rename.rs:120`
  Visible-window math mixes char count with column width; cursor/text spill on CJK/emoji.
  Compute the visible window by display width.

- [x] **P1.4 Narrow-terminal layout has no minimum widths** — `src/ui/mod.rs:238`, `src/ui/panel.rs:81`
  Columns collapse / meta column overflows and gets clipped with no graceful degradation.
  Add min-width guard (collapse tree / drop meta column when too narrow).

### Broken modals on short terminals

- [x] **P1.5 Conflict popup clips buttons** — `src/ui/overlays/conflict.rs:154-180`
  Fixed height 14, all lines in one `Paragraph` with no scroll → action buttons clipped on
  ~12-row terminals, making the blocking modal unusable. Reserve button rows at a fixed
  bottom position / guard against tiny `inner.height`.

- [x] **P1.6 Find-overlay scroll-height mismatch** — `src/ui/mod.rs:72-77` vs `src/ui/find_overlay.rs:103`
  Scroll clamp uses `inner_h-5`, render uses `inner-4`; selected result can scroll
  off-screen. Compute results height once and share it.

### Interaction consistency

- [x] **P1.7 `Ctrl+l`/`Ctrl+h` don't wrap but `Tab` does** — `src/app/navigation.rs:5-16`
  `focus_next`/`focus_prev` clamp at boundary; `Tab` (`cycle_panel`) wraps. Make focus
  movement wrap too (or document).

- [x] **P1.8 Select-mode `d`/`D` leaves stale marks & stays in mode** — `src/app/visual.rs:189-197`
  Verified: the delete flow goes Select→Confirm and `handle_confirm` already clears all
  marks on `y`, so no stale marks remain. Added a "Selection cleared (N)" message to the
  Select-mode Esc for consistency with P1.9.

- [x] **P1.9 Esc clears selection silently** — `src/app/input.rs:53`
  Esc wipes `marked` with no message. Set a "Selection cleared" status when clearing a
  non-empty selection (align with `Space n`).

- [x] **P1.10 Incremental search: no "no match" feedback** — `src/app/search.rs:33-52`
  Cursor freezes silently when nothing matches; `n`/`N` already report "No match". Surface
  a status / highlight when incremental search finds nothing. Also indicate active search
  pattern after returning to Normal.

- [x] **P1.11 Mark cycle has no direct unmark** — `src/app/marks.rs:13-35`
  `m` cycles 0→1→2→3→0; removing a level-1 mark needs 3 more presses. Add a direct
  unmark key/modifier.

### Operations without feedback / progress

- [x] **P1.12 Archive ops: no confirm, no overwrite handling, no progress** — `src/app/archive.rs:305-438`
  Done (pragmatic subset): extract-all (`X`) now needs a double-press to confirm; create/
  extract/extract-all set an immediate status ("Extracting...", etc.) so the UI isn't silent.
  DEFERRED to X.3: full streaming progress + conflict-routing for archive ops would require
  archive::* to emit progress/conflict callbacks like ops::paste (large refactor).

- [x] **P1.13 `:archive` overwrites existing archive silently** — `src/app/command.rs:264`, `src/app/archive.rs:407-439`
  No check whether output exists. Prompt overwrite/abort.

- [x] **P1.14 Bulk-rename: cursor not moved to first conflict** — `src/app/bulk_rename.rs:356-362`
  On rejected apply, jump `cursor` to first `conflict_indices()`. Also: empty edit silently
  discarded (`:266-277`) — give feedback or keep empty so conflict highlight triggers.

- [x] **P1.15 chown applies with no summary** — `src/app/chmod.rs:266-302`
  Privileged op applied on Enter with no "change owner of N items to user:group?" affordance.

### Missing scroll indicators

- [x] **P1.16 Add scroll indicators + hint keys** — `confirm.rs`, `bookmarks.rs`, `info.rs`, `which_key.rs`
  Done via shared `scroll_separator`/`separator_with_indicator` helpers: info/confirm/bookmarks
  show a `N%` indicator; info advertises `j/k` only when scrollable; which_key shows `+N more`
  when truncated. NOTE: `chown.rs` (two-column user/group lists) deferred — see X.4.

---

## 🟡 P2 — Minor (text, cosmetics)

- [ ] **P2.1 Bookmark empty-state names wrong key** — `src/app/bookmarks.rs:6`
  "Use **B** to add one" but `B` opens the list; `b` adds. Change to "Use **b** to add one."

- [x] **P2.2 Confirm dialog dead row highlight** — `src/ui/overlays/confirm.rs:69-74`
  Both `if/else` branches return `t.bg_light`; cursor row has no real highlight.

- [ ] **P2.3 Panel name truncation loses extension** — `src/ui/panel.rs:117,144`
  Right-side ellipsis drops the extension. Consider middle-ellipsis keeping head + ext.

- [ ] **P2.4 Hex span underflow panic risk** — `src/ui/preview.rs:31-34`
  `pipe_start - offset_end` can underflow (usize panic) on malformed hex. Use `saturating_sub`.

- [ ] **P2.5 Help table truncation/alignment** — `src/ui/overlays/help.rs:251,284`
  Descriptions truncated with no `…`; hardcoded `key_width=14` misaligns long keys.

- [ ] **P2.6 select_pattern: `*` default + backspace exits** — `src/app/select_pattern.rs:10,41-47`
  Default `*` selects all on stray Enter; backspace on empty field abruptly exits mode.

- [ ] **P2.7 chmod input feedback** — `src/app/chmod.rs:149-200,255`
  Rejected keys (`8`, >4 chars) silently ignored; Enter hint dimmed until 3 digits though
  shorter octals are valid.

---

## Cross-cutting follow-ups (after the above)

- [ ] **X.1** Standardize all truncation on `util::truncate_to_width` / `display_width`
  (tree, find_overlay, inputs, conflict `icon.len()`).
- [ ] **X.2** Separate progress/spinner area from `status_message` (currently one line,
  cleared every keypress → du-progress flicker, lost messages).
- [ ] **X.3** Task-manager overlay: view all tasks, cancel, see per-task success/failure.
- [ ] **X.4** Small-terminal degradation: minimum widths/heights, "terminal too small"
  messages instead of empty boxes.

---

## Progress log

- 2026-06-09: Plan created from full-codebase UI/UX review.
- 2026-06-09: **P0 complete** (P0.1–P0.5). 605 tests pass, clippy clean.
  Added regression tests for delete-Enter, quit-with-tasks, conflict-channel handling.
- 2026-06-09: **P1 wide-char group complete** (P1.1–P1.4). Added util helpers
  `truncate_to_width_left` / `visible_input_tail` and shared `input_field_line`;
  tree/find/input fields now display-width aware; panel drops meta column on narrow
  panels. 613 tests pass, clippy clean.
- 2026-06-09: **P1 complete** (P1.1–P1.16). Modal rendering, scroll indicators,
  focus/selection/search/mark interactions, archive/bulk-rename/chown feedback. 619 tests,
  clippy clean. P1.12 progress-streaming and chown scroll indicators deferred to X.3/X.4.
