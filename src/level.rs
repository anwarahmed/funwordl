//! The four levels. Easy takes any five letters as a guess, where the others want a
//! real word; Easy and Normal have hints. Normal, Hard and Ultra Hard hold a guess to
//! the clues as wordl's three difficulties do, and the game starts on Normal.

use crate::game::{Difficulty, Game};

/// The most hints one word gives: its meaning, then three of its letters.
pub const MAX_HINTS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    /// Any five letters are a guess, there are hints, and a word that is not solved
    /// costs nothing.
    Easy,
    /// A guess must be a word in the dictionary; hints.
    Normal,
    /// Green letters stay where they are, yellow letters are reused.
    Hard,
    /// Also: a yellow letter must move to another spot, and gray clues are obeyed.
    Ultra,
}

impl Level {
    pub const ALL: [Self; 4] = [Self::Easy, Self::Normal, Self::Hard, Self::Ultra];
    /// The level a new player starts on.
    pub const DEFAULT: Self = Self::Normal;

    /// The number saved as `level` in the statistics file.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % 4]
    }

    pub fn next(self) -> Self {
        Self::from_index(self.index() + 1)
    }

    pub fn name(self) -> &'static str {
        ["Easy", "Normal", "Hard", "Ultra Hard"][self.index()]
    }

    /// Whether a guess may be any five letters. Everywhere else it must be a word.
    pub fn any_letters(self) -> bool {
        self == Self::Easy
    }

    /// Whether hints can be asked for. The two hard levels are played without.
    pub fn hints(self) -> bool {
        matches!(self, Self::Easy | Self::Normal)
    }

    /// Which clues a guess is held to.
    pub fn difficulty(self) -> Difficulty {
        match self {
            Self::Easy | Self::Normal => Difficulty::Normal,
            Self::Hard => Difficulty::Hard,
            Self::Ultra => Difficulty::Ultra,
        }
    }

    /// The level of a game that says nothing more than its rules: a daily puzzle saved
    /// without one. Up to 0.1.2 an Easy game was one with eight guesses, and such a
    /// game, found in a saved file, is still Easy and keeps its eight.
    pub fn of(game: &Game) -> Self {
        match game.difficulty {
            Difficulty::Normal if game.tries > crate::game::TRIES => Self::Easy,
            Difficulty::Normal => Self::Normal,
            Difficulty::Hard => Self::Hard,
            Difficulty::Ultra => Self::Ultra,
        }
    }
}

/// The stars a solved word earns: three, less one for each hint taken, but never fewer
/// than one.
pub fn stars(hints: usize) -> usize {
    3 - hints.min(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Mode, word};

    #[test]
    fn levels_cycle_and_say_what_they_allow() {
        assert_eq!(Level::Easy.next(), Level::Normal);
        assert_eq!(Level::Ultra.next(), Level::Easy);
        assert_eq!(Level::ALL.map(Level::any_letters), [true, false, false, false]);
        assert_eq!(Level::ALL.map(Level::hints), [true, true, false, false]);
        for level in Level::ALL {
            assert_eq!(Level::from_index(level.index()), level);
        }
        // Easy and Normal hold a guess to the same clues: none.
        assert_eq!(Level::Easy.difficulty(), Level::Normal.difficulty());
    }

    #[test]
    fn a_game_saved_without_its_level_is_read_from_its_rules() {
        let game = |difficulty, tries| Game::new(Mode::Daily, 0, word("CRANE").unwrap(), difficulty).with_tries(tries);
        assert_eq!(Level::of(&game(Difficulty::Normal, 6)), Level::Normal);
        assert_eq!(Level::of(&game(Difficulty::Hard, 6)), Level::Hard);
        assert_eq!(Level::of(&game(Difficulty::Ultra, 6)), Level::Ultra);
        // An Easy game from before 0.1.3.
        assert_eq!(Level::of(&game(Difficulty::Normal, 8)), Level::Easy);
    }

    #[test]
    fn hints_cost_stars_but_never_all_of_them() {
        assert_eq!([stars(0), stars(1), stars(2), stars(MAX_HINTS)], [3, 2, 1, 1]);
    }
}
