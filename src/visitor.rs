use std::{
    borrow::Cow,
    io::{self, Write},
    mem,
    ops::AddAssign,
    time::Duration,
};

use arrayvec::ArrayVec;
use indicatif::ProgressBar;
use pgn_reader::{RawComment, RawHeader, SanPlus, Skip, Visitor};
use rustc_hash::FxHashMap;

#[derive(Default, Debug, Clone)]
pub struct Rating(usize);

impl AddAssign for Rating {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0
    }
}

#[derive(Default, Debug, Clone)]
struct Player {
    username: String,
    rating: Rating,
}

impl Player {
    fn to_tuple(self) -> (String, Rating) {
        (self.username, self.rating)
    }
}

#[derive(Default, Debug, Clone)]
struct Players {
    white: Player,
    black: Player,
}

impl Players {
    fn add_name(&mut self, key: &[u8], value: String) {
        if key == b"White" {
            self.white.username = value
        } else {
            self.black.username = value
        }
    }

    fn into_iter(self) -> [(String, Rating); 2] {
        [self.white.to_tuple(), self.black.to_tuple()]
    }

    fn add_rating(&mut self, key: &[u8], value: String) {
        if key == b"WhiteElo" {
            self.white.rating = Rating(value.parse().unwrap())
        } else {
            self.black.rating = Rating(value.parse().unwrap())
        }
    }
}

#[derive(Default, Debug)]
pub struct TimeSpent {
    pub nb_games: usize,
    pub total_rating: Rating,
    pub time_spent_exact: Duration,
    ///  in seconds
    /// computed with formula  (clock initial time in seconds) + 40 × (clock increment)
    pub time_spent_approximate: usize,
}

impl TimeSpent {
    fn add_game(
        &mut self,
        game_exact_duration: Duration,
        game_approximate_duration: usize,
        rating: Rating,
    ) {
        self.nb_games += 1;
        self.total_rating += rating;
        self.time_spent_exact += game_exact_duration;
        self.time_spent_approximate += game_approximate_duration;
    }

    fn to_csv(&self, w: &mut impl Write) -> io::Result<()> {
        // nb_game, average, accurate
        if self.nb_games > 0 && !self.time_spent_exact.is_zero() && self.time_spent_approximate > 0
        {
            write!(
                w,
                ",{},{},{},{}",
                self.nb_games,
                self.total_rating.0 / self.nb_games,
                self.time_spent_approximate,
                self.time_spent_exact.as_secs()
            )
        } else {
            write!(w, ",,,")
        }
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
    fn add_game(&mut self, game_exact_duration: Duration, avg_time: usize, rating: Rating) {
        // https://lichess.org/faq#time-controls
        if avg_time <= 29 {
            self.ultrabullet
                .add_game(game_exact_duration, avg_time, rating)
        } else if avg_time <= 179 {
            self.bullet.add_game(game_exact_duration, avg_time, rating)
        } else if avg_time <= 479 {
            self.blitz.add_game(game_exact_duration, avg_time, rating)
        } else if avg_time <= 1499 {
            self.rapid.add_game(game_exact_duration, avg_time, rating)
        } else {
            self.classical
                .add_game(game_exact_duration, avg_time, rating)
        }
    }

    // start with a leadinb colon, so need to be predecessed by `username`
    pub fn to_csv(&self, w: &mut impl Write) -> io::Result<()> {
        self.ultrabullet.to_csv(w)?;
        self.bullet.to_csv(w)?;
        self.blitz.to_csv(w)?;
        self.rapid.to_csv(w)?;
        self.classical.to_csv(w)
    }
}

pub struct PgnVisitor {
    pub games: usize,
    pub users: FxHashMap<String, TimeSpents>,
    pub pb: ProgressBar,
    game: Game, // storing temporary variable
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
    players: Players,
    plies: u64,
    link: String, // for debugging purpose
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
    fn game_duration(self) -> (Players, Option<Duration>) {
        // base time - finish time + increment * nb_plies
        (
            self.players,
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

fn decode<'a>(value: RawHeader<'a>, field: &str, g: &Game) -> Cow<'a, str> {
    value
        .decode_utf8()
        .unwrap_or_else(|e| panic!("Error {e} decoding {field} at game: {g:?}"))
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
            let username = decode(value, "username", &self.game).to_string();
            self.game.players.add_name(key, username);
        } else if key == b"WhiteElo" || key == b"BlackElo" {
            let rating = decode(value, "rating", &self.game).to_string();
            self.game.players.add_rating(key, rating);
        } else if key == b"TimeControl" {
            let tc = decode(value, "tc", &self.game);
            if tc != "-" {
                self.game.tc = tc_to_tuple(&tc).unwrap_or_else(|| {
                    panic!("could not convert tc {tc:?} at game {:?}", self.game)
                })
            }
        } else if key == b"Site" {
            self.game.link = decode(value, "link", &self.game).to_string();
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
    fn end_headers(&mut self) -> Skip {
        // avoiding games without clocks
        Skip(self.game.tc == Tc::default())
    }

    fn end_game(&mut self) -> Self::Result {
        let finished_game = mem::take(&mut self.game);
        let plies = finished_game.plies;
        let avg_time = finished_game.tc.average_time();
        let (players, exact_duration_opt) = finished_game.game_duration();
        if plies >= 4 {
            if let Some(exact_duration) = exact_duration_opt {
                for (username, rating) in players.into_iter() {
                    let mut time_spents = self
                        .users
                        .remove(&username)
                        .unwrap_or_else(TimeSpents::default);
                    time_spents.add_game(exact_duration, avg_time, rating);
                    self.users.insert(username, time_spents);
                }
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
        g.last_two_comments.push("[%clk 0:01:00]".to_string());
        g.last_two_comments.push("[%clk 0:01:00]".to_string());
        g.tc = Tc::new((60, 2));
        g.plies = 2;
        let (_, d) = g.game_duration();
        assert_eq!(d.unwrap(), Duration::from_secs(4))
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
