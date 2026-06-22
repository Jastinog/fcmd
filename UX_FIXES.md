⏺ Сделал свежий проход по коду. TODO/FIXME нет (проект чистый), но видны конкретные пробелы. Вот что осталось, по
  убыванию ценности.

  🔝 Самое полезное

  1. «Открыть системным приложением» (open/xdg-open). Сейчас o/F4 открывают только $EDITOR, а Enter на не-тексте (PDF,
  картинка, видео, .docx) показывает hex-дамп (src/main.rs:90 — единственный внешний запуск). Не хватает действия
  «открыть в дефолтном приложении ОС»: open (macOS), xdg-open (Linux), start (Windows). Классическая и очень частая
  операция. Дёшево, кросс-платформенно, ложится на существующий паттерн.

  2. Файл конфигурации. Настраивается только тема (config.toml, один ключ theme, src/theme/mod.rs:461). Логично вынести
  туда дефолты, которые сейчас захардкожены: стартовый layout, show-hidden, дефолтная сортировка, нужно ли подтверждать
  удаление, ширина дерева/панелей, дефолтный редактор. Один раз сделать каркас чтения — и снять кучу
  «магических» дефолтов.

  🟠 Среднее

  3. Превью изображений (Kitty graphics / sixel) — сейчас картинки и PDF идут как hex. Для современных терминалов сильно
  поднимает ценность side-preview и viewer.

  4. Сравнение/синхронизация каталогов между панелями: подсветить, чего нет в другой панели / что новее. Естественное
  продолжение swap/equalize, классика dual-panel (MC Ctrl-x d).

  5. Кастомизация клавиш — всё захардкожено в input.rs. Для vim-аудитории частый запрос; но это бо́льшая работа (парсер
  маппингов + диспетчеризация).

  🟡 Мелочи / надёжность

  6. ~462 .unwrap() вне тестов. Большинство наверняка безопасны (lock/инфраструктура), но стоит прогнать аудит на те,
  что в путях с пользовательскими/ФС-данными (могут паниковать при гонке/спецсимволах). Могу прогнать code-review именно
  на panic-риски.

  7. [ВЫПИЛЕНО] Корзина/восстановление — режим корзины удалён целиком. Осталось только чистое
  удаление через подтверждение (`dd`); в корзину файлы больше не отправляются и не восстанавливаются.

  ---
  Мои рекомендации по порядку: #1 (open-with) — максимальная отдача за минимум кода; затем #2 (конфиг) — снимает
  системный долг; #3 (превью картинок) — самый заметный «вау».

  За что взяться? Могу также сначала прогнать аудит panic-рисков (#6), если хочется сперва укрепить, а потом расширять.

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
  Full streaming progress + conflict-routing for archive ops (archive::* emitting
  progress/conflict callbacks like ops::paste) was tracked as X.5 and is now DONE.

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

- [x] **P2.1 Bookmark empty-state names wrong key** — `src/app/bookmarks.rs:6`
  "Use **B** to add one" but `B` opens the list; `b` adds. Change to "Use **b** to add one."

- [x] **P2.2 Confirm dialog dead row highlight** — `src/ui/overlays/confirm.rs:69-74`
  Both `if/else` branches return `t.bg_light`; cursor row has no real highlight.

- [ ] **P2.3 Panel name truncation loses extension** — `src/ui/panel.rs:117,144`
  Right-side ellipsis drops the extension. Consider middle-ellipsis keeping head + ext.
  DEFERRED: middle-ellipsis changes the look of every truncated filename (incl. dirs) —
  a stylistic call left to the maintainer rather than imposed.

- [x] **P2.4 Hex span underflow panic risk** — `src/ui/preview.rs:31-34`
  `pipe_start - offset_end` can underflow (usize panic) on malformed hex. Use `saturating_sub`.

- [x] **P2.5 Help table truncation/alignment** — `src/ui/overlays/help.rs:251,284`
  Descriptions truncated with no `…`; hardcoded `key_width=14` misaligns long keys.

- [x] **P2.6 select_pattern: `*` default + backspace exits** — `src/app/select_pattern.rs:10,41-47`
  Default `*` selects all on stray Enter; backspace on empty field abruptly exits mode.

- [x] **P2.7 chmod input feedback** — `src/app/chmod.rs:149-200,255`
  Rejected keys (`8`, >4 chars) silently ignored; Enter hint dimmed until 3 digits though
  shorter octals are valid.

---

## Cross-cutting follow-ups (after the above)

- [x] **X.1** Standardize all truncation on `util::truncate_to_width` / `display_width`
  (tree, find_overlay, inputs, conflict `icon.len()`).
  Done for every popup that renders real user data (filenames/paths/values), which is
  where wide chars actually appear: `confirm`, `chmod` (file ctx + input + rwx breakdown),
  `archive` (tree names), `info` (values), `bookmarks` (paths), `theme_picker` preview-file
  panel, plus the shared `separator_with_indicator`. `bulk_rename`'s local
  `truncate_with_ellipsis`/`char_width` now delegate to the util helpers, fixing all its
  call sites at once.
  INTENTIONALLY LEFT (char-count == display-width, no wide chars possible / out of scope):
  static ASCII chrome in `help`, `which_key`, and the `theme_picker` mock UI; `archive`
  search-prefix/indent; `chown` user/group lists (deferred to X.4); and the `preview`
  in-overlay search bar, whose mid-string cursor scroll needs its own width-aware rework
  rather than a truncation swap.
- [x] **X.2** Separate progress/spinner area from `status_message` (currently one line,
  cleared every keypress → du-progress flicker, lost messages).
  Done: added a dedicated `App::background_progress: Option<String>` field. The `du`
  calculation now writes its progress there (start + per-poll), and `poll_du` clears it on
  finish while putting the final summary into `status_message` as a transient result.
  Rendered in the **tab bar** info segment with an animated spinner (priority: running
  file-task → background_progress → task_notification), alongside file-task progress —
  all background activity lives in one place. `status_message` (bottom bar) is now purely
  user messages + ambient context and is no longer clobbered by progress polls, so the
  per-keypress flicker and message loss are gone. `main.rs` `snapshot()` tracks the new
  field and the tick loop animates the spinner while it's active.
  Left intentionally: `status_message`'s per-keypress clear (by design; long-lived notices
  already use the timed `task_notification` mechanism), and archive's one-shot
  "Extracting..."/"Creating..." messages (single status writes, not flickering polls).
- [x] **X.3** Task-manager overlay: view all tasks, cancel, see per-task success/failure.
  Done: new `Mode::Tasks` overlay (open with `Space j` or `:tasks`/`:jobs`) lists every
  copy/move/delete task with a live spinner + `[████░░] %` progress bar for running tasks
  and a ✓/✗/⊘ glyph + summary for finished/failed/cancelled ones. `x`/`d` cancels the
  selected task, `c` clears finished, `j/k/g/G` navigate, esc/q close.
  Cancellation is real: each task carries a shared `Arc<AtomicBool>` checked between items
  in `ops::paste_in_background` and the delete loop (a single in-flight file isn't
  interrupted); partial records are still pushed to the undo stack. Added `cancelled` flags
  to `ProgressMsg::Finished`/`DeleteMsg::Finished`.
  Finished tasks are now retained (so the overlay shows history) instead of being purged on
  completion; `poll_tasks` surfaces the completion notification from the finishing event and
  prunes finished history to `MAX_FINISHED` (50). 628 tests, clippy clean.
  NOTE: the P1.12 deferral (full *archive* op streaming/conflict-routing into the task
  manager) is a separate `archive::*` refactor and is NOT part of this overlay work — it
  is tracked below as X.5 (now done).

- [x] **X.5** Stream archive create/extract through the task manager (progress + conflict
  routing), the way `ops::paste` does. Carried over from P1.12.
  Done: `archive::*` gained callback-driven streaming functions — `extract_stream`
  (progress + per-file conflict resolver + `AtomicBool` cancel) and `create_stream`
  (progress + cancel, returns written count). The old `extract_entry`/`extract_all`/
  `create_archive` are kept as thin no-op-callback wrappers (still cover the round-trip
  tests). `create_stream` flattens directories into a pre-ordered entry list and appends
  one file at a time (replacing tar's opaque `append_dir_all`) so progress + between-file
  cancellation work for both zip and tar; cancelled archives are still finalized to a valid
  partial file. Extract conflicts route through the **existing** `ConflictInfo`/
  `ConflictChoice`/`conflict_rxs`/`Mode::Conflict` machinery (same dialog as paste), so
  extract no longer overwrites silently. New `ArchiveMsg` + `TaskKind::Archive` +
  `add_archive`/`poll_all` arm + `TaskEvent::ArchiveFinished`; archive tasks appear in the
  X.3 task overlay (`Space j`) with live progress bars, ✓/✗/⊘ status, and real cancellation,
  and in the tab-bar info segment (magenta). The fire-and-forget `FileOpResult::Archive*`
  path is removed. 647 tests, clippy clean.
- [x] **X.4** Small-terminal degradation: minimum widths/heights, "terminal too small"
  messages instead of empty boxes.
  Done: added `MIN_TERM_WIDTH`/`MIN_TERM_HEIGHT` (24×6) + `is_too_small()` guard at the top
  of `render()` — below the floor it draws a centered, width-clamped "Terminal too small /
  min 24×6 / now W×H" notice and skips the panel/overlay pipeline entirely (no garbled
  boxes). Popups already clamp via `centered_rect` and have per-overlay panic guards
  (P1.5/P1.6), and the global guard means they only ever render at ≥24×6.
  Also finished the **chown** items deferred from X.1/P1.16: width-aware name truncation
  (`display_width`/`truncate_to_width`) and ↑/↓ scroll indicators in the User/Group column
  headers when a list overflows its window.

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
- 2026-06-09: **P2 done** (P2.1,2.2,2.4,2.5,2.6,2.7); P2.3 left as a design decision.
  619 tests, clippy clean.
- 2026-06-09: **X.1 done** — width-aware truncation standardized across
  confirm/chmod/archive/info/bookmarks/theme_picker-preview overlays and the shared
  separator; bulk_rename helpers now delegate to util. ASCII-only chrome, chown (→X.4),
  and the preview search input left intentionally (see X.1 note). 619 tests, clippy clean.
- 2026-06-09: **X.4 done** — global "terminal too small" notice (24×6 floor) replaces the
  broken layout below the minimum; chown gains width-aware truncation + ↑/↓ scroll
  indicators (the X.1/P1.16 deferral). 621 tests, clippy clean.
- 2026-06-09: **X.3 done** — task-manager overlay (`Space j` / `:tasks`) with live
  progress bars, per-task success/fail/cancel status, real between-items cancellation
  (`Arc<AtomicBool>` through ops/delete), retained finished history with pruning. Split the
  archive-streaming remainder of P1.12 into a new **X.5**. 628 tests, clippy clean.
- 2026-06-09: **X.2 done** — dedicated `background_progress` field for du (and future
  background work), rendered with a spinner in the tab bar separate from `status_message`;
  no more du-progress flicker or message clobbering. 628 tests, clippy clean.
- 2026-06-09: **X.5 done** — archive create/extract now stream through the task manager.
  New callback-driven `archive::extract_stream`/`create_stream` (progress + cancel; extract
  also routes per-file overwrite conflicts through the existing paste conflict dialog, so
  extract no longer overwrites silently). `create_stream` walks dirs into a flat entry list
  and appends per-file (replacing tar `append_dir_all`) for real progress + cancellation.
  Added `ArchiveMsg`/`TaskKind::Archive`/`TaskEvent::ArchiveFinished`; archive jobs show in
  the X.3 overlay + tab bar. Removed the old `FileOpResult::Archive*` path. This closes the
  P1.12 archive-streaming remainder. 647 tests, clippy clean.
