# lichess-time-spent

Extracting time spent playing on Lichess by player

## Usage 

To extract the data, you need a working rust installation: `cargo run --release -- <PATH_TO_PGN> <NUMBER_OF_GAMES_IN_PGN>`

`PATH_TO_PGN` can lead to a compressed file that will be decompressed on the fly.
`NUMBER_OF_GAMES_IN_PGN` is just used for the progress bar and compute approximate duration of operation. You can use any number if you don't know or care.
The results are stored in `time-spent.csv`.

Some data analysis can be found in `data-analysis.ipynb`, you can run them with `jupyter notebook`.

## Proofreading

To produce a PDF file for easier proof-reading: `rm -f data-analysis.pdf && jupyter nbconvert data-analysis.ipynb --no-input --to pdf && open data-analysis.pdf`