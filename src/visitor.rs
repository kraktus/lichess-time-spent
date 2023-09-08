use std::{time::Duration, unimplemented};

use arrayvec::{ArrayString, ArrayVec};
use indicatif::ProgressBar;
use pgn_reader::{RawComment, RawHeader, SanPlus, Skip, Visitor};
use rustc_hash::FxHashMap;

// Small string. capped by max username length, 30.
//type SString = ArrayString<30>;

pub struct TimeSpent {
    pub nb_games: usize,
    /// in seconds
    pub time_spent_exact: usize,
    ///  in seconds
    /// computed with formula  (clock initial time in seconds) + 40 × (clock increment)
    pub time_spent_approximate: usize,
}

pub struct TimeSpents {
    bullet: TimeSpent,
    blitz: TimeSpent,
    rapid: TimeSpent,
    classical: TimeSpent,
}

pub struct PgnVisitor {
    pub games: usize,
    pub usernames: FxHashMap<String, TimeSpents>,
    pb: ProgressBar,
    game: Game, // storing temporary variable
}

#[derive(Default, Debug, PartialEq, Eq, Hash)]
struct Tc {
    // in seconds
    base: u64,
    // in seconds
    increment: u64,
}

impl Tc {
    fn new(tc: (u64, u64)) -> Self {
        Self {
            base: tc.0,
            increment: tc.1,
        }
    }
}

#[derive(Default, Debug)]
struct Game {
    usernames: ArrayVec<String, 2>,
    plies: u64,
    // needed in case of berserk
    first_two_clocks: ArrayVec<Duration, 2>,
    // sliding of the last two clock
    last_two_comments: ArrayVec<String, 2>,
    // the initial time, in seconds, with the increment, in seconds
    tc: Tc,
}

impl Game {
    fn acc_comment(&mut self, comment: String) {
        // first if there's still room we add to the first two clocks
        if !self.first_two_clocks.is_full() {
            self.first_two_clocks
                .push(comment_to_duration(&comment).unwrap());
        }
        // if the last two_clock is full, we need to displace the sliding-window
        if let Err(e) = self.last_two_comments.try_push(comment) {
            self.last_two_comments[0] = self.last_two_comments.pop().unwrap();
            self.last_two_comments.push(e.element())
        }
    }

    fn game_duration(self) -> Duration {
        // base time - finish time - increment * nb_plies
        self.first_two_clocks.into_iter().sum::<Duration>()
            - self
                .last_two_comments
                .into_iter()
                .map(|x| comment_to_duration(&x).unwrap())
                .sum()
            - Duration::from_secs(self.plies * self.tc.increment)
    }
}

fn tc_to_tuple(tc: &str) -> Option<Tc> {
    tc.split_once("+")
        .and_then(|(base, increment)| base.parse().ok().zip(increment.parse().ok()))
        .map(Tc::new)
}

fn comment_to_duration(comment: &str) -> Option<Duration> {
    let (_, clock_str) = comment.split_once("[%clk ")?;
    let (h_str, m_str, s_str) = clock_str
        .split_once(":")
        .and_then(|(h, m_and_s)| m_and_s.split_once(":").map(|(m, s)| (h, m, s)))?;
    let (h, m, s): (u64, u64, u64) = (
        h_str.parse().ok()?,
        m_str.parse().ok()?,
        s_str[..s_str.len() - 1].parse().ok()?,
    );
    Some(Duration::from_secs(h * 3600 + m * 60 + s))
}

impl PgnVisitor {
    pub fn new(pb: ProgressBar) -> Self {
        Self {
            games: 0,
            pb,
            usernames: FxHashMap::default(),
            game: Game::default(),
        }
    }
}

impl Visitor for PgnVisitor {
    type Result = ();

    fn begin_game(&mut self) {
        self.games += 1;
        if self.games % 10_000 == 9999 {
            self.pb.inc(10_000)
        }
    }

    fn header(&mut self, key: &[u8], value: RawHeader<'_>) {
        if key == b"White" || key == b"Black" {
            let username = value
                .decode_utf8()
                .unwrap_or_else(|e| {
                    panic!(
                        "{}",
                        format!("Error {e} decoding username at game: {}", self.games)
                    )
                })
                .to_string();
            self.game.usernames.push(username)
        } else if key == b"TimeControl" {
            let tc = value.decode_utf8().unwrap_or_else(|e| {
                panic!(
                    "{}",
                    format!("Error {e} decoding tc at game: {}", self.games)
                )
            });
            self.game.tc = tc_to_tuple(&tc).unwrap()
        }
    }
    fn san(&mut self, _: SanPlus) {
        self.game.plies += 1;
    }

    fn comment(&mut self, c: RawComment<'_>) {
        self.game
            .acc_comment(String::from_utf8_lossy(c.as_bytes()).to_string())
    }
    fn begin_variation(&mut self) -> Skip {
        Skip(true)
    }

    fn end_game(&mut self) -> Self::Result {

    }
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use super::*;

    #[test]
    fn test_comment_to_duration() {
        assert_eq!(
            comment_to_duration("[%clk 0:00:01]"),
            Some(Duration::from_secs(1))
        )
    }
    #[test]
    fn test_tc_to_duration() {
        assert_eq!(tc_to_tuple("60+3"), Some(Tc::new((60, 3))))
    }

    #[test]
    fn test_sliding_window_clock() {
        let mut game = Game::default();
        game.acc_comment("[%clk 0:00:01]".to_string());
        game.acc_comment("[%clk 0:00:02]".to_string());
        game.acc_comment("[%clk 0:00:03]".to_string());
        assert_eq!(
            game.first_two_clocks.into_inner().unwrap(),
            [Duration::from_secs(1), Duration::from_secs(2)]
        );
        assert_eq!(
            game.last_two_comments.into_inner().unwrap(),
            ["[%clk 0:00:02]".to_string(), "[%clk 0:00:03]".to_string()]
        );
    }
}
