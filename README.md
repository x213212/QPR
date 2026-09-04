# QPR — Quick Project Report

Point it at a source folder and it produces a browsable HTML report explaining
what the code does, using an LLM. It is meant for the moment you inherit an
unfamiliar repository and need a map before you start reading files.

Written in Rust; serves the report over HTTP so you can read it in a browser
rather than in a terminal.

## How it works

1. Lists the directories under the target folder.
2. Asks the model which of them are likely to hold **hand-written source** (as
   opposed to vendored dependencies, build output or assets), and keeps those.
3. Walks the kept directories for known source extensions:
   `rs, py, js, ts, java, cpp, c, go, sh, rb, bat, cs, resx, h, md`
4. Summarises each file concurrently (`futures::join_all`).
5. Serves the assembled report with `warp` on port `3030`.

Step 2 is the part that matters — filtering before summarising is what keeps a
large repository from turning into thousands of pointless API calls.

## Two backends

| File | Backend |
|---|---|
| `src/main.rs` | OpenAI API |
| `src/main_llama3.rs` | local Llama 3 |

## Setup

```bash
cp .env.example .env   # then edit
```

`.env`:

```
OPENAI_API_KEY=sk-...
```

The committed `.env` holds a `----` placeholder, not a real key.

## Run

```bash
cargo run --release
# then open http://127.0.0.1:3030
```

## Configuration

The tunables are constants at the top of `src/main.rs`:

- `SERVER_PORT` — HTTP port (default `3030`)
- `CODE_FILE_EXTENSIONS` — which files count as source
- `FOLDER_ANALYSIS_PROMPT` — the directory-filtering prompt

## License

Apache-2.0. See [LICENSE](LICENSE).
