//! Extracting time spent playing for each player from pgn files

use std::{env, fs::File, io};

use indicatif::{ProgressBar, ProgressStyle};
use pgn_reader::BufferedReader;

mod visitor;

pub fn get_progress_bar(nb_games: u64) -> ProgressBar {
    let pb = ProgressBar::new(nb_games);
    pb.set_style(
            ProgressStyle::with_template(
                "{msg} {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
            )
            .expect("Invalid indicatif template syntax")
            .progress_chars("#>-"),
        );
    pb
}

fn main() {
    let mut args = env::args();
    let path = args.nth(1).expect("pgn path expected");
    let nb_games = args
        .next()
        .and_then(|s| u64::from_str_radix(&s, 10).ok())
        .expect("input total number of games from the pgn, to get proper time estimate");
    let file = File::open(&path).expect("fopen");

    let uncompressed: Box<dyn io::Read> = if path.ends_with(".zst") {
        Box::new(zstd::Decoder::new(file).expect("zst decoder"))
    } else if path.ends_with(".bz2") {
        Box::new(bzip2::read::MultiBzDecoder::new(file))
    } else if path.ends_with(".xz") {
        Box::new(xz2::read::XzDecoder::new(file))
    } else if path.ends_with(".gz") {
        Box::new(flate2::read::GzDecoder::new(file))
    } else if path.ends_with(".lz4") {
        Box::new(lz4::Decoder::new(file).expect("lz4 decoder"))
    } else {
        Box::new(file)
    };
    let mut reader = BufferedReader::new(uncompressed);

    let mut visitor = visitor::PgnVisitor::new(get_progress_bar(nb_games));
    reader.read_all(&mut visitor).expect("Valid pgn file");
    // let mut usernames_vec: Vec<(String, usize)> = visitor.usernames.into_iter().collect();
    // usernames_vec.sort();
    // let usernames_vec: Vec<String> = usernames_vec
    //     .into_iter()
    //     .map(|(u, g)| format!("{u},{g}"))
    //     .collect();
    // std::fs::write("usernames-nb-games.csv", usernames_vec.join("\n")).expect("write failed");
}
