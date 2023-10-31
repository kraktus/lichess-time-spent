# lichess-time-spent

Extracting time spent playing on Lichess by player, and analysing it.

## Extracting time spent playing 

To extract the data, [you need a working rust installation](https://www.rust-lang.org/learn/get-started).

Then in a terminal:

- Clone the repository: `git clone https://github.com/kraktus/lichess-time-spent`

- Enter the `lichess-time-spent` directory then run: `cargo run --release -- <PATH_TO_PGN> <NUMBER_OF_GAMES_IN_PGN>`

`PATH_TO_PGN` can lead to a compressed file that will be decompressed on the fly. [You can use database.lichess.org to download compressed versions of Lichess rated games](https://database.lichess.org).

`NUMBER_OF_GAMES_IN_PGN` is just used for the progress bar and compute approximate duration of operation. You can use any number if you don't know or care.
The results are stored in `time-spent.csv` put in the current directory.

## Data analysis

Some data analysis can be found in `data-analysis.ipynb`. To run it:

- Create a `venv`: `python3 -m venv venv`
- Turn it on: `source venv/bin/activate`
- Install the dependencies: `pip3 intstall -r requirements.txt`
- In `data-analysis.ipynb`, edit the line `df = pd.read_csv(\"time-spent-2023-01.csv\",dtype=dtypes)`, replacing `time-spent-2023-01.csv` with the name of your CSV file (`time-spent.csv` if you have not renamed it).
- Run `jupyter notebook` which will open jupyter in the browser.

## Proofreading

To produce a PDF file for easier proof-reading:
* First make sure to save the document (checkpoint) in jupyter UI
* Then run `jupyter nbconvert data-analysis.ipynb --no-input --to pdf && open data-analysis.pdf`

## Export

### Produce the figures

`jupyter nbconvert data-analysis.ipynb --to python && python3 -O data-analysis.py`

### Produce the markdown

`jupyter nbconvert data-analysis.ipynb --no-input --to markdown`