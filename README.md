# chrime

> 🎭 **A member of the [Fraude family](https://github.com/eidos-agi/fraude-code)** — alongside
> `fraude-code` and the Fraude OS apps (Chrime · Gfail · Schemes · Extort). The family joke is
> that the brand is a costume but the work is real. Chrime is the most real of the bunch.

**A browser built for AI agents, not humans.** No GUI, no pixel pipeline. The interface is an
API; what it exposes is the DOM as a compact semantic tree with stable node-ids — the thing an
agent's decision loop actually needs.

Existing automation tools are bad at agent control because they puppeteer a *human* browser
(pixels, coordinates, flaky selectors). Chrime inverts that: the machine-native surface is
primary. And it's a **settle-and-snapshot** engine, not a real-time one — an agent never
watches a stream of frames, so Chrime drops the compositor/vsync/animation loop that eats a
human browser's memory. See [DESIGN.md](DESIGN.md).

Today (v0): fetch + parse real HTML (Servo's html5ever) — **1.9 MB binary, ~7.6 MB RSS**.
No JS yet (that's v1, embedded v8).

## Run

```sh
cargo build --release

# headed (default): a terminal render of the semantic DOM, with an address prompt
./target/release/chrime example.com

# API (for agents): one JSON command per line on stdin, one JSON result per line on stdout
printf '%s\n' \
  '{"op":"navigate","url":"news.ycombinator.com"}' \
  '{"op":"snapshot"}' \
  '{"op":"click","node_id":4}' | ./target/release/chrime --api
```

## API

| op | args | returns |
|----|------|---------|
| `navigate` | `url` | nav result (ok, url, status, title) |
| `snapshot` | — | the semantic DOM: nodes with `node_id`, `role`, `text`, `href`, `clickable` |
| `read` | — | full page text |
| `click` | `node_id` | follows the node's link (v0: href only; v1 will run JS handlers) |
| `current` | — | current URL |

Same commands back both interfaces, through one `Engine` trait — so a future v8 engine drops
in behind the identical API. That seam is the whole point.

MIT.
