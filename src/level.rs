//! The four levels. Easy has more guesses, and hints. The other three are wordl's
//! difficulties, unchanged, and the game starts on the first of them, Normal.

use crate::game::{Difficulty, Game, TRIES};

/// How many guesses an Easy game allows.
pub const EASY_TRIES: usize = 8;
/// The most hints one word gives: its meaning, then three of its letters.
pub const MAX_HINTS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    /// Eight guesses, hints on request, and a word that is not solved costs nothing.
    Easy,
    /// Six guesses; any dictionary word is a valid guess.
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

    pub fn tries(self) -> usize {
        if self == Self::Easy { EASY_TRIES } else { TRIES }
    }

    /// Which clues a guess is held to.
    pub fn difficulty(self) -> Difficulty {
        match self {
            Self::Easy | Self::Normal => Difficulty::Normal,
            Self::Hard => Difficulty::Hard,
            Self::Ultra => Difficulty::Ultra,
        }
    }

    /// The level a game is being played at. Nothing more is stored than what the rules
    /// need: an Easy game is one that allows more than the usual six guesses.
    pub fn of(game: &Game) -> Self {
        match game.difficulty {
            Difficulty::Normal if game.tries > TRIES => Self::Easy,
            Difficulty::Normal => Self::Normal,
            Difficulty::Hard => Self::Hard,
            Difficulty::Ultra => Self::Ultra,
        }
    }

    /// Sets a game to this level.
    pub fn apply(self, game: &mut Game) {
        game.difficulty = self.difficulty();
        game.tries = self.tries();
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
    fn levels_cycle_and_describe_a_game() {
        assert_eq!(Level::Easy.next(), Level::Normal);
        assert_eq!(Level::Ultra.next(), Level::Easy);
        assert_eq!((Level::Easy.tries(), Level::Normal.tries(), Level::Ultra.tries()), (8, 6, 6));
        for level in Level::ALL {
            let mut game = Game::new(Mode::Practice, 0, word("CRANE").unwrap(), Difficulty::Normal);
            level.apply(&mut game);
            assert_eq!(Level::of(&game), level);
            assert_eq!(Level::from_index(level.index()), level);
        }
    }

    #[test]
    fn hints_cost_stars_but_never_all_of_them() {
        assert_eq!([stars(0), stars(1), stars(2), stars(MAX_HINTS)], [3, 2, 1, 1]);
    }
}
