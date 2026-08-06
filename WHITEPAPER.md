# Lifeline — a plain-English white paper

*Messaging that works when nothing else does.*

This is the friendly version. If you want the math, the protocol details, and the
formal analysis, read the [technical white paper](docs/whitepaper/) and the
[research notes](docs/research/). This document is for everyone else — no
cryptography background required.

> **Status:** Lifeline is **alpha** and **not yet independently security-audited.**
> It works and you can try it today, but please don't bet your life on it yet.
> It's open source precisely so it can be reviewed and hardened in the open.

---

## What is Lifeline?

Lifeline is a messaging app that keeps working when the internet and phone
networks are down, blocked, or overloaded.

Normal messaging apps need towers and servers. Take those away — a storm knocks
out the cell towers, a government pulls the plug, or 50,000 people at a festival
all fight for the same sliver of signal — and your messages simply don't send.

Lifeline doesn't rely on any of that. **Every phone running Lifeline is a tiny
relay.** Your message hops from your phone to a nearby phone, to the next, and
the next, until it reaches the person you're trying to reach. Phones even *carry*
messages as people physically walk around, ferrying them across gaps. And the
moment **any single phone in the mesh touches the internet**, it can bridge
everyone's messages out to the wider world.

No towers. No accounts. No company in the middle reading your messages.

---

## Why this matters — real situations

Lifeline is built for the moments when staying reachable actually matters.

### 1. Disasters
Floods, tsunamis, snowstorms, wildfires, earthquakes — the first thing to fail is
the network, exactly when you most need to reach family and first responders.
With Lifeline, a neighborhood of phones forms its own network. You can send an
**SOS with your GPS location**, tell everyone **"I'm safe,"** and coordinate a
rescue — even with every tower down.

### 2. Internet shutdowns
During protests and unrest, authorities often cut the internet to stop people
organizing and to hide what's happening. Lifeline has no internet to cut. It
runs person-to-person on local radio, so a crowd can keep communicating even in a
total blackout — and the messages are encrypted, so intercepting the radio buys
an eavesdropper nothing but scrambled noise.

### 3. Privacy
Sometimes you just don't want anyone — not a carrier, not a platform, not an
advertiser — reading your messages or logging who you talk to. Lifeline encrypts
every message end-to-end, needs no phone number or account, and can even hide
*who is talking to whom* (see [Privacy & security](#your-privacy-and-security)).

### 4. Crowded events where the internet chokes
At concerts, festivals, and stadiums, tens of thousands of people overwhelm the
local cell towers. Texts stall, apps spin, and you **can't find your friends in
the crowd.** Lifeline sidesteps the congestion entirely — it doesn't touch the
overloaded towers — so you can message and **share your live location** with your
group and actually find each other.

---

## Sharing your location can save your life

We think location is Lifeline's quietest superpower. A coordinate that gets
through when nothing else does can be the difference between rescue and disaster.

- **SOS with GPS.** One tap broadcasts an emergency to everyone in range at the
  highest priority, with your exact coordinates and battery level attached — so
  rescuers know *where* to come and *how long* your phone will last.
- **Share your live location** with a specific person or your group, so you can
  regroup after getting separated.
- **Geocast — alert an area, not a contact.** Send a message to *everyone within
  a radius of a point* — "evacuate the riverbank," "medic needed at the north
  gate" — addressed by **place** instead of by name. Perfect when you don't know
  who's nearby, only *where* the problem is.
- **Find each other in a crowd.** Because phones near each other can measure
  their relative position, Lifeline can help a group converge even where GPS is
  weak and the internet is useless — the festival problem, solved with the same
  network that carries your messages.

In a flood, a coordinate finds you. In a crowd, it reunites you. In a blackout,
it directs help. That's worth building well.

---

## How it works (without the jargon)

Think of a message as a note you want to pass across a crowded room where no one
can shout across the whole space.

1. **Hop.** You hand the note to whoever is standing next to you.
2. **Carry.** People move around, carrying notes in their pockets, bridging gaps
   between groups that can't see each other directly. (Engineers call this a
   *delay-tolerant network* or a *"data mule."*)
3. **Copy, carefully.** The note is copied to a few people at once so it doesn't
   die if one phone leaves — but not to *everyone*, so the room doesn't drown in
   copies.
4. **Arrive.** The note reaches the right person, who sends back a **signed
   receipt** — cryptographic proof it was delivered — that travels back to you the
   same way. You see two checkmarks. No central server ever saw the message.
5. **Come alive.** If even one phone in the mesh has a scrap of internet, it acts
   as a **gateway** and bridges everyone's messages to the outside world.

Everyone's note looks identical from the outside — sealed and opaque — so the
people carrying it can't read it, and can't even easily tell who it's *for*.

---

## Your privacy and security

Lifeline is **private by construction, not by promise** — the design makes
snooping hard, rather than asking you to trust a company's policy.

- **End-to-end encrypted.** Every message is sealed to its recipient. The phones
  relaying it carry ciphertext they can't read. (For the curious: X25519 key
  agreement + XChaCha20-Poly1305.)
- **No accounts, ever (for the mesh).** Your identity is just a key on your
  device. No phone number, no email, no sign-up, no central list of users that
  could be leaked or subpoenaed.
- **Forward secrecy.** Your keys rotate over time, so a key stolen tomorrow can't
  unlock the messages you sent today.
- **Hides who you talk to.** A *private send* addresses a rotating, disposable tag
  instead of your real address — so the phones carrying it can't build a picture
  of who's talking to whom, or track a recipient over time.
- **Panic wipe.** One deliberate action irreversibly destroys the keys, contacts,
  and history on your device — for high-risk users who might be searched or
  coerced.
- **Key rotation & revocation.** If a key is compromised, you can retire it and
  move to a new one with a signed note your contacts verify automatically — a gap
  most messengers leave wide open.
- **Proof of delivery, no blockchain.** Delivery is proven with a signed receipt
  you can check yourself, offline. No coin, no ledger, no central log.

---

## What we learned from others (and where we go further)

We're not the first to try this, and we've openly borrowed good ideas.

- **[bitchat](https://github.com/permissionlesstech/bitchat)** (a Bluetooth-mesh
  chat app) proved the core idea — accountless, phone-to-phone, no servers — and
  its **panic-wipe** feature was compelling enough that we built our own. Its
  authors also honestly flagged their **biggest weakness: stable device IDs let
  outsiders track who's receiving messages.** We took that seriously and added
  **rotating rendezvous addresses** so a private send doesn't leak the recipient.
- **[Nostr](https://nostr.com)** popularized the idea that **your identity should
  just be a keypair you own** — portable, with no registrar. We use the same
  model. But Nostr famously has **no built-in forward secrecy, no metadata
  privacy for who-talks-to-whom, and no key rotation** — three things Lifeline
  adds.
- **[Buzz](https://github.com/block/buzz)** (a Nostr-based team workspace) is a
  good reminder that the *hosted, team-oriented* layer is valuable — which is why
  Lifeline offers an optional managed dashboard on top — while keeping the
  messenger itself accountless and serverless.

Where Lifeline is different: it's a true **store-carry-forward mesh** (messages
survive when nobody is online at the same time), it minimizes **metadata**, it
has **cryptographic proof of delivery**, and it treats **location** as a
first-class, life-saving feature.

---

## What Lifeline can't do yet (being honest)

- **Not audited.** The crypto is implemented and tested, but hasn't had a
  third-party security review. Treat it as alpha.
- **Native radio isn't shipped yet.** The design targets true phone-to-phone
  radio (Bluetooth LE, Wi-Fi Aware). *Today,* phones mesh over a local relay or
  Wi-Fi network that stands in for those radios — so "works with zero internet,
  phone-to-phone, out of the box" is the goal, not yet the default.
- **Private sends cost bandwidth.** Hiding *who* a message is for means it can't
  be routed straight to them, so it spreads more widely — a deliberate trade of
  efficiency for privacy.
- **It's not magic range.** Messages travel as far as there are phones to carry
  them. In an empty field with two people a mile apart, there's no mesh to hop
  across.

We'd rather tell you this now than oversell it.

---

## The vision

A world where a message always has a way through — where a disaster, a shutdown,
or a crowd can't cut you off from the people who matter, and where sending your
location is as reliable as it is life-saving. Built in the open, owned by no one,
usable by anyone.

**Try it, break it, help us harden it.**

---

## Learn more

- **[Use cases](docs/USE-CASES.md)** — the real-world situations Lifeline is built for, each mapped to the feature that answers it.
- **[Technical white paper](docs/whitepaper/)** — the formal protocol + analysis.
- **[Architecture](ARCHITECTURE.md)** — how the code fits together.
- **[Research](docs/research/)** — the deeper theory and simulations.
- **[Security policy](SECURITY.md)** — threat model + how to report a vulnerability.
- **[Source code](https://github.com/matrix-share/matrix)** — read every line.
