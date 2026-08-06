# Lifeline launch playbook

The run-of-show for taking Lifeline public as an open-source project. Each file
here is a ready-to-use draft; **posting to external platforms is a manual step
for a maintainer** (it publishes from your own account).

Tracking issues: #107–#121.

## The one rule
We are **alpha, honest, and technical**. We win on the *idea + the code + a
demo*, not on "download it now." Never overclaim — this audience punishes it.
The honest-status callout in the README is a feature, not a liability.

## Order of operations

1. **Prep the front door** (#107) — README brand header, badges, comparison
   table, links. Done before anything is posted.
2. **Build the demo** (#109) — the single highest-leverage asset. See
   [`demo-script.md`](demo-script.md). Everything below is stronger with it.
3. **Shrink "try it"** (#117) — at minimum `cargo install` + prebuilt binaries.
4. **Launch week** (batch, same week):
   - [`show-hn.md`](show-hn.md) — Show HN (#110), the #1 channel. Tue–Thu, ~8–9am ET.
   - [`reddit.md`](reddit.md) — per-subreddit variants (#111). Space out by a day or two.
   - [`other-channels.md`](other-channels.md) — Lobste.rs, r/opensource (#112).
   - [`social.md`](social.md) — Mastodon/Bluesky/X with the demo GIF (#116).
5. **Compounding, slow-burn:**
   - [`awesome-lists.md`](awesome-lists.md) — curated-list PRs (#113).
   - [`blog-percolation.md`](blog-percolation.md) — the theory post (#114).
   - arXiv preprint (#115).
6. **Sustain** (#118) — tagged releases, "what's new" Discussions, a public
   roadmap. Look *alive*; infrastructure people won't adopt an abandoned repo.

## Response discipline
For the first month, answer every issue/PR/comment fast. Early contributors and
early skeptics both convert if you're present and candid. On launch day, block
the calendar — being the responsive author *is* the growth strategy.

## Talking points (keep consistent everywhere)
- One line: **"Messaging that works when the network doesn't."**
- What it is: offline, end-to-end-encrypted mesh — your phone becomes the network.
- What runs today: nodes mesh over a local relay/LAN; native BT/Wi-Fi radio bearers
  are designed but not shipped. Say this plainly.
- The differentiator: **store-carry-forward across people and gaps**, plus an
  **opportunistic internet bridge** — one connected phone drains the whole mesh.
- The credibility: memory-safe Rust, ~292 tests, SSDLC + OpenSSF Scorecard,
  a whitepaper, and actual delivery theory.
