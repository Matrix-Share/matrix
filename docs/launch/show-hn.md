# Show HN draft (#110)

**Channel:** news.ycombinator.com · **When:** Tue–Thu, ~8–9am US Eastern ·
**Who:** posted by the maintainer account, present all day to reply.

---

## Title (pick one, keep it plain)

- `Show HN: Lifeline – offline, end-to-end-encrypted mesh messenger (Rust)`
- `Show HN: Lifeline – messaging that works when the network doesn't`

HN strips marketing. State what it is. No exclamation marks, no "revolutionary."

## URL

Submit the **GitHub repo** (https://github.com/matrix-share/matrix), not the
marketing site — HN prefers source. Put the site + demo in the first comment.

## First comment (post immediately after submitting)

> Hi HN — I'm the author. Lifeline is an offline mesh messenger: when there's no
> internet or cell service, your phone passes end-to-end-encrypted messages
> directly to nearby phones over Bluetooth/Wi-Fi, and they hop device-to-device
> until they reach the recipient. If the next hop is out of range, the message
> waits and rides along in someone's pocket (store-carry-forward), so it can
> cross gaps no single radio link could. The instant any one phone gets a sliver
> of connectivity, it can bridge the whole mesh's queued messages to the internet.
>
> **Honest status:** it's alpha and not yet independently security-audited. What
> runs today: nodes mesh over a local WebSocket relay / LAN, which stands in for
> the transport so browsers and servers can talk. The native phone-to-phone radio
> bearers (BLE / Wi-Fi Aware) are designed and partly built but **not shipped** —
> so "no internet at all, phone-to-phone" is the goal, not yet the one-tap reality.
> I'd rather say that up front.
>
> It's Rust (15 crates, ~292 tests), Apache-2.0, with a documented protocol and a
> whitepaper. There's also some actual theory behind when a mesh like this
> actually delivers (a mean-field ln N law; percolation near the critical density).
>
> How it differs from things you'll rightly mention:
> - **Briar** — great, but built around synchronous contact (or Tor); Lifeline
>   centers delay-tolerant carry across strangers + an internet bridge.
> - **bitchat** — closest in spirit (BLE mesh); Lifeline adds store-carry-forward
>   across gaps, SOS + location, groups, and the opportunistic gateway.
> - **Meshtastic** — excellent, but needs dedicated LoRa hardware; Lifeline is
>   phone-only.
> - **Nostr** — needs the internet/relays; different problem.
>
> Site + a short demo: <LINK>. Whitepaper: <LINK>. Happy to go deep on the
> crypto, the routing, or the threat model — ask away.

## Prep checklist
- [ ] Demo GIF/video ready and linked (#109)
- [ ] Repo README polished (#107) with comparison table (#108)
- [ ] `cargo install` / binaries available (#117) so people can actually try it
- [ ] Clear the day; set up notifications
- [ ] Have answers ready for: threat model, metadata leakage, Sybil/relay abuse,
      battery, iOS background BLE limits, "why not just Meshtastic/Briar", licensing

## Do / don't
- **Do** engage every critical comment technically and without defensiveness.
- **Do** thank people who point out flaws; file issues live and link them.
- **Don't** vote-ring or ask others to upvote (bannable).
- **Don't** argue about the name; redirect to substance.
