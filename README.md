# HorseTinder (Rust + vanilla JS)

Find your stable relationship. No frontend framework: a Rust server (Axum, Maud templates, SQLite) renders the pages, and one plain `swipe.js` file adds drag and keyboard swiping.

## Run

```sh
npm install && npm run build && cargo run --release
```

The frontend bundle (`public/assets/app.js`) is built from `web/app.js` with esbuild. It uses two npm packages: `jefflag` (vendored tarball in `vendor/`, since it is not on the npm registry) for local stable times and "matched X min ago", and `canvas-confetti` for the match toast.

Open http://localhost:4173. Use `PORT=4174 cargo run --release` for another port. The SQLite database is created and seeded in `data/` on first run.

## Controls

Drag the card, use the arrow keys, or click Pass / Like. Mutual likes show up under Matches. "Start over" clears your swipes.
