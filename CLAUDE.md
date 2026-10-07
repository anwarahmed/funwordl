# funwordl

A playful Wordle-style word game for the terminal (TUI), made for children: wordl with
sounds, stars, confetti, a collection of words learned, and an Easy level with hints. It
fills the terminal and rescales with it. Rust + ratatui, targets macOS and Linux.
`README.md` is for players; this file is for whoever changes the code.

funwordl is the sibling of https://github.com/anwarahmed/wordl and is built on wordl's
library. **Read wordl's `CLAUDE.md` too** (locally `../../wordl/main/CLAUDE.md`): the
drawing, the layout, the themes, the updater and the release machinery came from there
with their reasons, and those reasons are not repeated here. This file says what
funwordl adds and where it differs.

## Commands

```sh
cargo run --release                          # play (from a checkout it never updates itself)
cargo test                                   # unit tests: levels, hints, layout sweep, drawing
cargo clippy --all-targets -- -D warnings    # CI fails on any warning
cargo fmt                                    # rustfmt.toml: max_width 160
cargo build --release && tests/e2e.sh        # the built program in tmux, install.sh, the updater
tools/screenshot.py <tmux socket> assets/screenshot.png 255,206,226   # redraw a README picture
```

`install.sh` is wordl's with the name changed: latest release binary into
`~/.local/bin` (`FUNWORDL_BIN_DIR` overrides), checksum verified, `--source`, `--link`,
`--uninstall`. `FUNWORDL_RELEASE_URL` points it, and the self-updater, at another
download base (a `file://` directory; `tests/e2e.sh` makes such directories).

## Workflow

- **`main` only accepts pull requests** (GitHub ruleset "Main"): no direct pushes, no
  force-pushes, no deletion, no bypass for anyone. A PR needs these checks to pass,
  matched by job name: `test (ubuntu-latest)`, `test (macos-latest)`, `msrv`,
  `release checklist`. Renaming a CI job means updating the ruleset or PRs wait forever.
  PRs are squash-merged.
- **Local layout.** A bare clone with one worktree per branch:
  `~/Developer/GitHub/anwarahmed/funwordl/main` plus a sibling directory per feature
  branch (`git worktree add -b <branch> <branch> origin/main` from the bare repo).
  Remove the worktree and branch after the PR merges, then fast-forward `main`.
- **Releasing** is merging a version bump; a merge without one publishes nothing.
  **Follow [RELEASING.md](RELEASING.md) every time, every step.** The release PR's
  description must carry its checklist with every line ticked, or the
  `release checklist` check fails.
- **Sibling repos:** `anwarahmed/wordl` (the library, see below) and
  `anwarahmed/homebrew-tap` (the generated formula `Formula/funwordl.rb`, written by
  its `scripts/formulae/funwordl.sh`; it takes direct pushes).

## The library: what comes from wordl

`Cargo.toml` names wordl's repository at a commit:

```toml
wordl = { git = "https://github.com/anwarahmed/wordl", rev = "<commit>" }
```

From it come `wordl::game` (scoring a guess, the three clue rules, `Game`),
`wordl::words` (both word lists and the definitions), `wordl::store` (`Stats`, the
saved daily puzzle, `read_pairs` / `write_pairs`) and `wordl::update` (the updater,
told who it works for by the `Program` in `main.rs`). `main.rs` imports them at its
top, so the modules here reach them as `crate::game` and so on, exactly as wordl's
own game does.

- **A word, a definition or a rule is never fixed here.** Fix it in wordl, merge
  there, then raise `rev` here (run any cargo command so `Cargo.lock` follows) and
  release. A release of funwordl is the only way a fix made in wordl reaches
  funwordl's players; the release checklist has a line for it.
- **Trying a wordl change before it merges:** point the dependency at the wordl
  worktree for the moment (`wordl = { path = "../../wordl/<branch>" }`), build and
  test, and put the `git` line back before committing.
- **`SCOWL-COPYRIGHT` here is a copy** of wordl's `words/SCOWL-COPYRIGHT`, because the
  AUR package installs it from this repository. A test fails if the two differ.
- wordl's version number means nothing here: the library is taken by commit, and
  wordl makes commits to its library without releasing.

## Architecture

Single binary crate, no async. One file per concern in `src/`:

| File        | Role |
|-------------|------|
| `main.rs`   | CLI options, the `Program` the updater works for, terminal setup/teardown, event loop |
| `app.rs`    | `App` state, all key and mouse handling, what each action does, hints, stars, the collection, animation timing |
| `ui.rs`     | All drawing, and the geometry that says what a click landed on. Pure functions of `&App` |
| `level.rs`  | The four levels and how they map onto wordl's rules; `stars` |
| `sound.rs`  | The cues, their notes made into WAV files, and handing a file to the system's audio player |
| `layout.rs` | `layout(cols, rows, tries)`: sizes and positions for a terminal size and a board of six or eight rows |
| `font.rs`   | Two bitmap fonts and `glyph` (wordl's, unchanged) |
| `theme.rs`  | Color themes as roles (wordl's), plus the party colors and the default theme |

### What is funwordl's own

- **Levels are a thin layer over wordl's rules** (`level.rs`). A `Level` is a
  `Difficulty` plus a number of guesses: Easy is `Difficulty::Normal` with eight,
  the others are wordl's three with six. Nothing else is stored: `Level::of(&game)`
  reads the level back from the game (more than six guesses means Easy), which is why
  the saved daily puzzle needs no new field. The chosen level is `level=0..3` in the
  statistics file (0 is Easy); absent means `Level::DEFAULT`, which is Normal.
- **Easier at once, harder next game.** `Ctrl-X` goes Easy, Normal, Hard, Ultra Hard
  and round again. A game under way takes the new level only if it is easier or
  nothing has happened yet (no guess, no hint): earlier guesses were not held to
  stricter rules, and hints may have been used.
- **Sound** (`sound.rs`, since 0.1.1). A `Cue` is a list of notes
  (start, frequency, length); `Cue::samples` mixes them into 16-bit mono at 22050 Hz,
  each note with a quick rise and a bell-like decay, and `wav` wraps that as a file.
  Both are pure and unit-tested. `Sound::play` writes the file into a directory of
  the run's own under the system temp directory (removed when the game ends) and
  starts the first player found on `PATH` of `afplay`, `pw-play`, `paplay`,
  `aplay -q`, with nothing of the player's reaching the terminal. It never waits and
  never fails; at most four play at once. Things to keep:
  - **The tests must stay silent.** `find_player` returns nothing under `cfg(test)`,
    every `Options` in a test has `sound: false`, and every start of the game in
    `tests/e2e.sh` sets `FUNWORDL_NO_SOUND=1`. A test that wants to see cues sets
    `app.sound.on` and reads `app.sound.log`.
  - **A guess is one cue, not five.** `Cue::Reveal(marks, step)` holds a note per
    tile spaced by the flip's length, so the notes stay in step with the animation
    without a process per tile. (That is also why typing makes no sound: a player
    takes tens of milliseconds to start, and a key press cannot wait.)
  - **The end of a game sounds after the reveal** (`App::end_sound`, from `tick`),
    or in its place when animations are off.
  - The switch is `Ctrl-A` (`Action::Sound`), saved as `sound=0|1`; `--no-sound` and
    `FUNWORDL_NO_SOUND` are for one run. It has no footer item: there is no room.
- **Hints** (`Action::Hint`, Easy only). `App::hints` counts them; the first is the
  meaning, each later one pushes a spot onto `hint_letters`, up to `MAX_HINTS`. The
  spot is the first of 0, 2, 4, 1, 3 that is neither given already nor green in a
  guess, so a hint never tells what is known. Tab in the game takes the first hint or
  re-opens the dialog; only Tab inside the dialog takes another, so looking again is
  free. For the daily puzzle they are saved as `daily_hints=<day>,<count>,<spot>,...`
  in the statistics file. Hinted letters are drawn faintly in the row being typed
  (`draw_tile`); they are not typed for the player.
- **Stars.** `level::stars`: three, less one per hint, at least one. Added to `stars`
  in the statistics file when a word is solved.
- **The collection** (`App::learned`, the `learned` file, `Modal::Words`). `word=n`
  lines, where `n` is the most stars the word has earned and `0` means met but not
  solved. It is a `key=value` file read with wordl's `read_pairs`, so it is sorted by
  word and parsed strictly. The dialog shows one word at a time and always three
  lines of meaning, so its buttons stay put.
- **A word not solved on Easy costs nothing.** `App::finish` calls `Stats::record`
  only for a win or on the other levels, so on Easy a loss is not counted as played
  and does not end a run. The stars dialog therefore shows "Solved", "In a row",
  "Best" and "Stars", and no win percentage and no guess distribution (the `d1`..`d8`
  counts are still recorded by wordl's `record`, unused).
- **Confetti** (`App::confetti`, `ui::draw_confetti`). An animation like the others:
  a start time, and every piece's place worked out from the word and the elapsed
  time, nothing kept between frames. `Canvas::pixel` colors half a cell and leaves
  the other half alone, and only touches cells that are empty or already two pixels,
  so confetti never lands on text; it stops above the footer. The stars dialog waits
  for it (`CONFETTI`), and keys wait in the queue as for any animation.
  `FUNWORDL_NO_ANIM` and `--no-animation` skip it.
- **The board has six or eight rows**, so everything that sizes or draws it takes
  `game.tries`: `layout::layout`, `layout::min_rows` (12 or 14), the tile loop in
  `ui::draw`. `ui::layout_for(app, ..)` is the one place that passes it. The layout
  test sweeps every size for both.
- **The title is eight letters** in the party colors (`Theme::party`: the same eight
  colors in every theme, the terminal's own six in `terminal`). It is wider than the
  board, so on a narrow terminal there is no room for it and the info line together
  as tiles; `draw_title` then writes it small, a colored letter per cell, to keep the
  info. `layout::TITLE_LETTERS` is used wherever the title's width matters.
- **The footer has nine items** and must show its labels at 80 columns; they add up to
  77 with "Normal", the longest level name the footer shows. The collection and the
  sound switch have no footer item for that reason: they are `Ctrl-W` (also a button
  in the stars dialog) and `Ctrl-A`, both in the help.
- **The help is exactly 20 lines**, which with its frame and button is all of a
  24-row terminal. Adding a line means taking one out.
- **The result's buttons are `N New`, `C Copy`, `W Words`, `Esc`**: exactly 35
  columns. A longer label does not fit; a test checks every dialog.
- **Its own daily word** (`app::daily_answer`, `DAILY_BASE`): a different formula from
  wordl's, so that playing one game does not give away the other. A test checks they
  rarely coincide.

Everything under "Patterns to keep" in wordl's `CLAUDE.md` holds here too: the state /
view split, layout recomputed every frame, sizes as levels, pixels as half cells,
clipped `i32` coordinates, clicks using the drawing's geometry, letters never being
commands, animations as functions of time, themes as roles, frames in one write,
dialog lines of at most 35 characters, tests beside the code.

## Decisions and why

- **A separate game, not a rename and not a mode.** The user first asked whether wordl
  could be renamed funwordl, then chose to keep wordl as it is and have a second, more
  playful game beside it. So funwordl has its own repository, binary, state directory
  (`~/.local/state/funwordl`), environment variables, release assets, formula and AUR
  package, and both games can be installed at once. Nothing is migrated from wordl.
- **The core is shared as wordl's library** (asked for by the user: "is there a way to
  sync the shared core"). See wordl's `CLAUDE.md`, "A library and a game", for what
  was considered instead.
- **The game starts on Normal; Easy is a level to choose** (since 0.1.1; the user,
  after trying 0.1.0: "Start the game on normal difficulty"). 0.1.0 started on Easy, because the
  first request was for rules "easier by default, but have the option for more
  difficult levels". Only what an absent `level` means changed: a player who chose a
  level, Easy included, keeps it. What Easy means was proposed by Claude and accepted
  as part of "go ahead": eight guesses, hints, and no penalty for a word not solved.
  The numbers (eight, four hints, three stars) are Claude's choices and have not been
  played by a child yet.
- **Sounds, with a way to turn them off** (asked for by the user after trying 0.1.0;
  since 0.1.1).
  They are played by the system's own audio player, not by an audio library: a
  library (rodio, cpal) links ALSA on Linux, which the static musl release binaries
  cannot do, and would be the project's first dependency with system requirements.
  The terminal bell was not used: it is one sound, often muted or turned into a
  flash, and cannot tell a win from a mistake. Which notes are played is Claude's
  choice, checked by machine (length, loudness, a real player accepts the files) but
  **not heard by Claude**: only a person can say whether they are pleasant.
- **Hints teach before they tell.** The players are children up to 13 and the user
  sees the game as a way for them to learn words, so the first hint is the meaning,
  which is the thing worth learning, and letters come only after.
- **Bright by default** (`theme::DEFAULT` is `candy`). wordl's default is `midnight`.
- **Friendlier wording**: "I don't know that word", "So close! It was CRANE",
  "NICE TRY", "Show the word?" for giving up (the footer says "Reveal").
- **The player's history lives in wordl's `Stats` file format**, with funwordl's own
  keys added (`level`, `stars`, `daily_hints`), because the library already reads and
  writes it strictly and the game needs nothing more.
- **Version 0.1.0**, although the code started from wordl 0.2.8 and keeps its git
  history: it is a new program with a new name, and nobody has an older funwordl.
- Release assets, self-update, the marker file, pinned actions, packaging, minimum
  Rust (1.88) and the few dependencies are wordl's decisions, unchanged. The asset
  names are `funwordl-<rust target>` plus `VERSION`, `SHA256SUMS` and `PKGBUILD`;
  renaming them breaks installed copies, the install script, the formula and the AUR
  package at once.
- **The updater is one design in three repositories** (typeshelf, wordl, funwordl).
  funwordl gets its copy through the library, so a fix in wordl's `update.rs` arrives
  with the next `rev`; `install.sh` and `release.yml` here are copies and have to be
  changed by hand when wordl's change.
- **Homebrew:** the `TAP_TOKEN` secret here (set by the user on 2026-10-07, after
  0.1.0 was released without it) lets a release start the tap's workflow and wait for
  the formula. No release has used it yet, so the first one after 0.1.0 is its test.
  If it is missing the release run carries a "Homebrew tap not notified" warning; if
  it is rejected or has expired the run fails at that step, after the release is
  already published, and the user has to create a new token.
- **Not set up yet:** the AUR (the user has no AUR account; the `PKGBUILD` attached to
  each release is installed with `makepkg -si`).

## Verifying changes

`cargo test` covers the levels, hints, stars, collection and which sounds are asked
for when (`app.rs`, `level.rs`, `sound.rs`), the
layout at every size for six and eight rows, and drawing every theme and dialog at a
dozen sizes into ratatui's `TestBackend`. `tests/e2e.sh` runs the built binary: the
command line, `install.sh`, the updater against made-up releases, and the game in a
detached tmux session with a hint, mouse clicks, the collection, a resize, giving up,
and animations (confetti included) on.

Neither can judge how the screen looks. For that, run the game in a detached tmux
session on a private server with a throwaway state directory, and read the screen or
draw it:

```sh
X=$(mktemp -d)
tmux -L funwordl-test new-session -d -x 150 -y 46 \
  "XDG_STATE_HOME=$X COLORTERM=truecolor FUNWORDL_DEBUG_ANSWER=crane target/release/funwordl"
tmux -L funwordl-test send-keys -l slate; tmux -L funwordl-test send-keys Enter
tmux -L funwordl-test capture-pane -p                  # add -e to see colors
tools/screenshot.py funwordl-test /tmp/shot.png 255,206,226
tmux -L funwordl-test resize-window -x 39 -y 14        # then look again
tmux -L funwordl-test kill-server                      # only ever with -L
```

`FUNWORDL_DEBUG_ANSWER` fixes the practice word; `FUNWORDL_NO_ANIM=1` skips
animations; `FUNWORDL_NO_SOUND=1` keeps it quiet, which a session driven by a script
should always be. To check sound without making any, put a stand-in `pw-play` first on
`PATH` that copies its argument somewhere, play, and then run the real player on the
copies with `--volume 0`. To catch confetti in a picture, take it about 2.7 seconds after the
winning Enter. `tools/screenshot.py` needs the theme's background color as its third
argument, because tmux does not report trailing blank cells: without it the right of
every row comes out black.

The traps listed in wordl's `CLAUDE.md` apply (`send-keys Escape` followed at once by
a key arrives as Alt+key; never test copying against the real clipboard).

## Known gaps and ideas

- Not played by a child, or by any person, yet: everything was driven by Claude in
  tmux. Whether eight guesses, four hints and the star rule feel right is unknown.
- Never run by a person on a real Mac; CI runs the tests on a macOS runner.
- The Intel macOS binary is cross-built and never executed in CI.
- The sounds have not been heard by anyone but a machine (see "Sounds" above). There
  is no volume setting; the system's volume is the only one.
- Typing and the keys of the on-screen keyboard make no sound.
- No mascot. A pixel-art character that reacts to guesses was among the ideas put to
  the user and is not built; the layout has no place reserved for one.
- On Easy at 80x24 the tiles are one row high, because eight rows of the next size do
  not fit; the other levels get wordl's three-row tiles there.
- At 39 columns the title is plain colored letters, not tiles.
- The collection is alphabetical and has no search; with hundreds of words, leafing
  one at a time will be slow.
- Stars are never spent on anything.
- The README pictures are drawn by `tools/screenshot.py` from tmux's cell data, not
  captured from a terminal window.
- Dialogs taller than the terminal lose their last lines (the help is 20 lines).
- Not done: other word lengths, other languages.
