# wordl

A Wordle-style word game for the terminal, written in bash.

The game fills the whole terminal and rescales when the window is resized:
on a large terminal the tiles and keys are drawn as big block-art letters, on
a small one they shrink down to single characters (minimum 39x12). Wide
terminals put the keyboard beside the board, tall ones put it underneath.

## Run

    ./wordl

Needs bash 4.4 or newer and a UTF-8 terminal. macOS ships bash 3.2, so
install a current one there first (`brew install bash`).

    wordl --daily            today's puzzle instead of a random word
    wordl --theme neon       midnight, daylight, neon, contrast, terminal
    wordl --hard             or --ultra / --normal: the difficulty (see below)
    wordl --no-animation
    wordl --help

## Play

Type a five-letter word and press Enter. Green: right letter, right spot.
Yellow: right letter, wrong spot. Gray: not in the word. Six tries.

| Key         | Action                                      |
| ----------- | ------------------------------------------- |
| `A`-`Z`     | type a letter                               |
| `Enter`     | submit the guess                            |
| `Backspace` | delete a letter                             |
| `?` / `F1`  | help                                        |
| `Ctrl-N`    | new game with a random word                 |
| `Ctrl-D`    | today's daily puzzle (same word for everyone) |
| `Ctrl-S`    | statistics                                  |
| `Ctrl-T`    | next color theme                            |
| `Ctrl-X`    | next difficulty                             |
| `Ctrl-L`    | redraw                                      |
| `Ctrl-Q`    | quit                                        |

## Difficulty

`Ctrl-X` cycles through three levels. A game that is under way can be made
easier at any time; a harder level applies from the next game.

- **Normal** - guesses must be valid dictionary words.
- **Hard** - Wordle's hard mode: green letters must stay fixed and yellow
  letters must be reused.
- **Ultra Hard** - stricter still: yellow letters must also move away from
  the spot where they were clued, and gray clues must be obeyed (a gray
  letter can't be played again, beyond the copies of it already shown as
  green or yellow).

Shared results mark Hard with `*` and Ultra Hard with `**`.

The on-screen keyboard, the footer buttons and the dialog buttons can all be
clicked with the mouse.

The `terminal` theme uses the terminal's own 16 colors, so it follows your
terminal theme. `contrast` swaps green/yellow for orange/blue.

Every launch starts a new random word. The daily puzzle (`Ctrl-D`) is the
one thing that resumes, because there is only one word a day.

Statistics, the daily puzzle's progress and the chosen settings are saved in
`$XDG_STATE_HOME/wordl` (default `~/.local/state/wordl`).

## Files

    wordl                  the game
    words/answers.txt      words a puzzle can be
    words/allowed.txt      words accepted as guesses
    tools/build-words.py   regenerates both lists from SCOWL

The script looks for the word lists in `$WORDL_DATA_DIR`, then `words/` next
to itself, then `../share/wordl`.

## Credits

Word lists are derived from [SCOWL](http://wordlist.aspell.net/) by Kevin
Atkinson; see `words/SCOWL-COPYRIGHT`. The game itself is MIT licensed.
