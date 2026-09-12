# Show HN post (#110)

Post it yourself from your own account (HN dislikes third-party/marketing
submissions). Best days: Tue–Thu, ~8–10am ET. Have the repo, the honest status,
and the demo GIF (#109) ready first — the GIF is what makes this land.

---

**Title** (80 char limit; lead with the honest hook, no hype):

```
Show HN: Lifeline – an open-source mesh messenger that works without the internet
```

Alternatives if you want the honest-alpha angle up front:
- `Show HN: A DTN mesh messenger with cryptographic proof-of-delivery (alpha)`
- `Show HN: Offline-first encrypted messaging that "comes alive" on one connection`

---

**URL:** `https://github.com/matrix-share/matrix`

---

**First comment** (post immediately after submitting — this is where you set the
honest frame; HN will trust you *more* for it, not less):

> Author here. Lifeline is an open-source, end-to-end-encrypted mesh messenger
> built as a Delay/Disruption-Tolerant Network: every phone is a node, messages
> hop device-to-device and get physically carried between partitions until they
> reach the recipient, and delivery comes back with an offline-verifiable
> cryptographic receipt — no blockchain, no accounts, no server that holds who
> talked to whom.
>
> I want to be upfront about what's real vs. aspirational, because it's a
> life-safety-shaped tool and I'd rather under-claim:
>
> **What works today** (runnable right now — `./quickstart.sh`, or
> `docker compose up`): the whole decentralized core. Self-certifying identity,
> sealed-sender E2E crypto (Ed25519/X25519/XChaCha20/BLAKE3 — no custom crypto),
> store-carry-forward routing with binary spray-and-wait, custody transfer, CRDT
> state merge after a partition, erasure coding across lossy links, gateway
> "one connection lights the whole mesh," SOS/geocast, group messaging, ~292
> tests, and a deterministic acceptance sim that hits its ≥95%-delivery bar.
> Nodes currently mesh over a local relay/LAN.
>
> **What's NOT shipped yet** (and I say so everywhere): the native
> phone-to-phone *radios* — Bluetooth LE / Wi-Fi Aware / ultrasound. The
> platform-independent BLE stack is built and a desktop backend is wired, but
> the on-device mobile radio isn't done — so "works with zero internet,
> phone-to-phone" is the goal, not yet a turnkey fact. It also has **not** had a
> third-party security audit. Please don't bet a life on it yet.
>
> The part I'd most like feedback on is a result that fell out of building it:
> metering a scarce shared resource (internet egress through a gateway) with an
> offline bearer capability across a partition is *literally offline
> double-spending* — so a global usage budget is unenforceable without
> consensus, and the right move is Chaum–Fiat–Naor-style detect-and-attribute
> on merge, not pretend-to-prevent. Writeup in the whitepaper.
>
> Happy to go deep on the DTN routing, the crypto, the CRDT/GC design, or the
> BLE bearer work (help very welcome there). What would you want to see before
> trusting something like this?

---

**Prep checklist before posting**
- [ ] Demo GIF recorded and in the README (#109) — *do not post without this*
- [ ] `./quickstart.sh` verified on a clean machine
- [ ] README status disclaimer visible above the fold (done)
- [ ] You have ~2 hours free to answer comments in the first hour (this decides ranking)
- [ ] SECURITY.md present so people can report issues responsibly (it is)
