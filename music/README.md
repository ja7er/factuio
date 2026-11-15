# factuio soundtrack

Six original pieces, 12 minutes 53 seconds in all, rendered from the game's own score and
synthesizer (`src/music/`). The game plays them live and doesn't need these files; they're here
for the website and for listening on their own.

| # | Track | Length |
| --- | --- | --- |
| 1 | Morning Shift | 2:04 |
| 2 | Ore and Ember | 2:09 |
| 3 | Conveyor Waltz | 2:01 |
| 4 | Night Grid | 2:17 |
| 5 | Kingdom of Cogs | 2:10 |
| 6 | Terminal Dreams | 2:10 |

Each track comes as OGG Vorbis (`.ogg`, about 100 kbit/s) and MP3 (`.mp3`, about 115 kbit/s),
mastered to about -16 LUFS. `tracks.json` lists titles, lengths and file names.

To render them again after changing the score:

```sh
cargo run --release -- --export-music /tmp/factuio-music
cd /tmp/factuio-music
for f in *.wav; do
  ffmpeg -i "$f" -c:a libvorbis -q:a 3 "${f%.wav}.ogg"
  ffmpeg -i "$f" -c:a libmp3lame -q:a 5 "${f%.wav}.mp3"
done
```
