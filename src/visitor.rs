use std::{
    format,
    io::{self, Write},
    mem,
    ops::Add,
    str::FromStr,
    time::Duration,
    unimplemented,
};

use arrayvec::{ArrayString, ArrayVec};
use indicatif::ProgressBar;
use pgn_reader::{RawComment, RawHeader, SanPlus, Skip, Visitor};
use rustc_hash::FxHashMap;

// Small string. capped by max username length, 30.
type SString = ArrayString<50>;

// #[derive(Default, Debug, PartialEq, Eq, Hash)]
// struct Duration(u64);

// impl Duration {
//     fn from_secs(x: u64) -> Self {
//         Self(x)
//     }
// }

// impl Add for Duration {
//     type Output = Self;

//     fn add(self, rhs: Self) -> Self::Output {
//         Self(self.0 + rhs.0)
//     }
// }

type Usernames = ArrayVec<SString, 2>;

#[derive(Default, Debug)]
pub struct TimeSpent {
    pub nb_games: usize,
    /// in seconds
    pub time_spent_exact: Duration,
    ///  in seconds
    /// computed with formula  (clock initial time in seconds) + 40 × (clock increment)
    pub time_spent_approximate: usize,
}

impl TimeSpent {
    fn add_game(&mut self, game_exact_duration: Duration, game_approximate_duration: usize) {
        self.nb_games += 1;
        self.time_spent_exact += game_exact_duration;
        self.time_spent_approximate += game_approximate_duration;
    }

    fn to_csv(&self, w: &mut impl Write) -> io::Result<()> {
        // nb_game, average, accurate
        write!(
            w,
            "{},{},{}",
            self.nb_games,
            self.time_spent_approximate,
            self.time_spent_exact.as_secs()
        )
    }
}

#[derive(Default, Debug)]
pub struct TimeSpents {
    ultrabullet: TimeSpent,
    bullet: TimeSpent,
    blitz: TimeSpent,
    rapid: TimeSpent,
    classical: TimeSpent,
}

impl TimeSpents {
    fn add_game(&mut self, game_exact_duration: Duration, avg_time: usize) {
        // https://lichess.org/faq#time-controls
        if avg_time <= 29 {
            self.ultrabullet.add_game(game_exact_duration, avg_time)
        } else if avg_time <= 179 {
            self.bullet.add_game(game_exact_duration, avg_time)
        } else if avg_time <= 479 {
            self.blitz.add_game(game_exact_duration, avg_time)
        } else if avg_time <= 1499 {
            self.rapid.add_game(game_exact_duration, avg_time)
        } else {
            self.classical.add_game(game_exact_duration, avg_time)
        }
    }

    fn to_csv(&self, w: &mut impl Write) -> io::Result<()> {
        self.ultrabullet.to_csv(w)?;
        write!(w, ",")?;
        self.bullet.to_csv(w)?;
        write!(w, ",")?;
        self.blitz.to_csv(w)?;
        write!(w, ",")?;
        self.rapid.to_csv(w)?;
        write!(w, ",")?;
        self.classical.to_csv(w)
    }
}

pub struct PgnVisitor {
    pub games: usize,
    pub users: FxHashMap<SString, TimeSpents>,
    pub pb: ProgressBar,
    game: Game, // storing temporary variables
}

impl PgnVisitor {
    pub fn new(pb: ProgressBar) -> Self {
        Self {
            games: 0,
            pb,
            users: FxHashMap::default(),
            game: Game::default(),
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq, Hash, Clone, Copy)]
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
    fn average_time(&self) -> usize {
        (self.base + 40 * self.increment) as usize
    }
}

#[derive(Default, Debug, Clone)]
struct Game {
    usernames: Usernames,
    plies: u64,
    link: SString, // for debugging purpose
    // needed in case of berserk
    first_two_clocks: ArrayVec<Duration, 2>,
    // sliding of the last two clock
    last_two_comments: ArrayVec<SString, 2>,
    // the initial time, in seconds, with the increment, in seconds
    tc: Tc,
}

impl Game {
    fn add_comment(&mut self, comment: SString) {
        // first if there's still room we add to the first two clocks
        if !self.first_two_clocks.is_full() {
            self.first_two_clocks.push(
                comment_to_duration(&comment).unwrap_or_else(|| {
                    panic!("could not read comment {comment:?} for game {self:?}")
                }),
            );
        }
        // if the last two_clock is full, we need to displace the sliding-window
        if let Err(e) = self.last_two_comments.try_push(comment) {
            self.last_two_comments[0] = self
                .last_two_comments
                .pop()
                .expect("last comment empty, game {self:?}");
            self.last_two_comments.push(e.element())
        }
    }

    // The use of the +15s button can break the game duration calculation
    // then the game is skipped
    fn game_duration(self) -> (Usernames, Option<Duration>) {
        // base time - finish time + increment * nb_plies
        (
            self.usernames,
            (self.first_two_clocks.into_iter().sum::<Duration>()
                + Duration::from_secs(self.plies * self.tc.increment))
            .checked_sub(
                self.last_two_comments
                    .into_iter()
                    .map(|x| {
                        comment_to_duration(&x).unwrap_or_else(|| {
                            panic!("could not read comment {x:?}, game: {:?}", self.link)
                        })
                    })
                    .sum(),
            ),
        )
    }
}

fn tc_to_tuple(tc: &str) -> Option<Tc> {
    tc.split_once("+")
        .and_then(|(base, increment)| base.parse().ok().zip(increment.parse().ok()))
        .map(Tc::new)
}

fn comment_to_duration(comment: &str) -> Option<Duration> {
    let (_, clock_str) = comment.split_once("[%clk ")?;
    let (h_str, m_str, s_str_with_rest) = clock_str
        .split_once(":")
        .and_then(|(h, m_and_s)| m_and_s.split_once(":").map(|(m, s)| (h, m, s)))?;
    let s_str = s_str_with_rest.split_once("]").map(|x| x.0)?;
    let (h, m, s): (u64, u64, u64) = (
        h_str.parse().ok()?,
        m_str.parse().ok()?,
        s_str.parse().ok()?,
    );
    Some(Duration::from_secs(h * 3600 + m * 60 + s))
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
            let username = SString::from(&value.decode_utf8_lossy()).unwrap_or_else(|e| {
                panic!("Error {} decoding username at game: {:?}", e, self.game)
            });
            self.game.usernames.push(username)
        } else if key == b"TimeControl" {
            let tc = value
                .decode_utf8()
                .unwrap_or_else(|e| panic!("Error {} decoding tc at game: {:?}", e, self.game));
            if tc != "-" {
                self.game.tc = tc_to_tuple(&tc).unwrap_or_else(|| {
                    panic!("could not convert tc {tc:?} at game {:?}", self.game)
                })
            }
        } else if key == b"Site" {
            self.game.link = SString::from(&value.decode_utf8_lossy())
                .unwrap_or_else(|e| panic!("Error {} decoding link at game: {:?}", e, self.game))
        }
    }
    fn san(&mut self, _: SanPlus) {
        self.game.plies += 1;
    }

    fn comment(&mut self, c: RawComment<'_>) {
        self.game.add_comment(
            SString::from_str(&String::from_utf8_lossy(c.as_bytes()))
                .unwrap_or_else(|e| panic!("Error {} decoding link at game: {:?}", e, self.game)),
        )
    }
    fn begin_variation(&mut self) -> Skip {
        Skip(true)
    }
    fn end_headers(&mut self) -> Skip {
        // avoiding games without clocks
        Skip(self.game.tc == Tc::default())
    }

    fn end_game(&mut self) -> Self::Result {
        let finished_game = mem::take(&mut self.game);
        let avg_time = finished_game.tc.average_time();
        let (usernames, exact_duration_opt) = finished_game.game_duration();
        if let Some(exact_duration) = exact_duration_opt {
            for username in usernames.into_iter() {
                let mut time_spents = self
                    .users
                    .remove(&username)
                    .unwrap_or_else(TimeSpents::default);
                time_spents.add_game(exact_duration, avg_time)
            }
        }
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
    fn test_comment_to_duration2() {
        assert_eq!(
            comment_to_duration(" [%clk 0:03:00] "),
            Some(Duration::from_secs(180))
        )
    }
    #[test]
    fn test_tc_to_duration() {
        assert_eq!(tc_to_tuple("60+3"), Some(Tc::new((60, 3))))
    }

    #[test]
    fn game_duration_calculation() {
        let mut g = Game::default();
        g.first_two_clocks.push(Duration::from_secs(60));
        g.first_two_clocks.push(Duration::from_secs(60));
        g.last_two_comments.push(
            SString::from_byte_string(b"[%clk 0:01:00]                                    ")
                .unwrap(),
        );
        g.last_two_comments.push(
            SString::from_byte_string(b"[%clk 0:01:00]                                    ")
                .unwrap(),
        );
        g.tc = Tc::new((60, 2));
        g.plies = 2;
        let (_, d) = g.game_duration();
        assert_eq!(d.unwrap(), Duration::from_secs(4))
    }

    #[test]
    fn test_sliding_window_clock() {
        let ss_2 = SString::from_byte_string(b"[%clk 0:00:02]                                    ")
            .unwrap();
        let ss_3 = SString::from_byte_string(b"[%clk 0:00:03]                                    ")
            .unwrap();
        let mut game = Game::default();
        game.add_comment(
            SString::from_byte_string(b"[%clk 0:00:01]                                    ")
                .unwrap(),
        );
        game.add_comment(ss_2.clone());
        game.add_comment(ss_3.clone());
        assert_eq!(
            game.first_two_clocks.into_inner().unwrap(),
            [Duration::from_secs(1), Duration::from_secs(2)]
        );
        assert_eq!(
            game.last_two_comments.into_inner().unwrap(),
            [ss_2.clone(), ss_3.clone()]
        );
    }
}
