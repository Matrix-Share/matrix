# Where Lifeline helps — real-world use cases

Lifeline is a messenger that keeps working when the usual network — cell towers
and the internet — is **missing, broken, blocked, or overwhelmed**. Instead of
routing every message through a tower and a data centre, each phone becomes a
node: messages **hop from device to device, get carried by people who are
moving, and are copied along the way** until they reach whoever they're for. The
moment *any one* device in the group touches connectivity, everything queued
"comes alive" and flows out — with a cryptographic proof it was delivered.

That single idea turns out to matter in a lot of situations. This page walks
through them in plain language: what goes wrong, and exactly which Lifeline
feature answers it.

> **Status: alpha, and not yet independently security-audited.** These use cases
> describe what Lifeline is *designed and built* for. The cryptography and mesh
> logic work and are tested, but Lifeline has not had a third-party security
> review, and the native phone-to-phone radios (Bluetooth/Wi-Fi Aware) are
> designed but not shipped yet. **Don't bet a life on it today** — help us get it
> audited and finished. See the [README](../README.md) for what runs right now.

A thread runs through almost all of these: **sharing your location can save your
life.** When you can't describe where you are — you're hurt, lost, panicking, or
don't know the area — a GPS coordinate that travels over the mesh does the
talking. Lifeline treats location as a first-class safety feature: a one-tap
**SOS with your GPS and battery level**, **live location sharing**, **geocast**
(alert everyone inside an area), and **find-each-other** (see how far away each
person is and which way to walk).

---

## 1. When the network breaks — disasters

**Floods, tsunamis, earthquakes, hurricanes, wildfires, blizzards.** The first
thing a major disaster takes out is the cell network: towers lose power, backhaul
lines snap, and the ones still standing jam instantly as a whole city tries to
call at once. That's precisely when people most need to say *"we're on the roof,"*
*"the bridge is out, go north,"* or *"I'm trapped, here are my coordinates."*

- A **one-tap SOS** carries your **GPS location and battery level** to everyone in
  range — and keeps spreading device-to-device even if you have zero bars.
- **Store-carry-forward** means a message doesn't need a live path to exist *right
  now*. A neighbour walking to higher ground physically carries it until it
  reaches someone who can act, or a phone that has signal.
- **One gateway lights the mesh**: if a single person anywhere in the area still
  has a working data connection (satellite, a distant tower, a car hotspot),
  everyone's queued messages drain out through them — and replies flow back in.
- **Geocast** lets responders alert *everyone within a radius* of a danger —
  "evacuate the riverbank," "gas leak on 5th" — without needing anyone's number.

## 2. When the internet is blocked — shutdowns, protests, censorship

Governments increasingly **shut off the internet or mobile data** during
protests, elections, and unrest — a deliberate blackout to stop people
organizing and to keep the outside world from seeing what's happening. A
tower-free mesh routes *around* the switch.

- Messages travel **phone-to-phone**, so there's **no central service to block,
  throttle, or subpoena**. There's no server holding a list of who talked to whom.
- **Sealed sender + rotating rendezvous addresses** mean carriers and onlookers
  can't easily build a map of who is receiving messages — the metadata that
  surveillance depends on is deliberately starved.
- **No accounts, no phone number, no email.** Your identity is a cryptographic
  key generated on your device — nothing to hand over, nothing tied to your
  real-world ID.
- If **one person** near the edge of the blackout has a satellite or foreign SIM,
  the mesh uses them as a gateway to get word — and evidence — out.

**Journalists and activists** in censored regions get the same two things at
once: a way to keep communicating when the state pulls the plug, and
**end-to-end encryption** so the messages themselves stay private even if a
device is inspected. (A **panic wipe** securely destroys keys and history in one
tap if a device is about to be seized.)

## 3. When the network is overwhelmed — crowds, concerts, festivals

You don't need a disaster to lose service — you just need **too many people in
one place**. At a concert, festival, stadium, marathon, or on New Year's Eve, the
towers are technically up but so congested that texts don't send and you
**can't find your friends** in the crowd.

- **Find each other:** everyone in your group shares their location over the mesh,
  and Lifeline shows you **how far away each person is and which direction to
  walk** ("Maya · 120 m · NE") — no working internet required, because the
  positions hop between phones directly.
- Group messages that would never squeeze through a saturated tower **go
  device-to-device** across the crowd instead.
- Because it's **peer-to-peer**, Lifeline actually works *better* the denser the
  crowd — more phones means more relays, the opposite of how cell towers fail.

This is also the friendliest way for new people to experience the mesh: it solves
a mild, everyday, universally-understood annoyance ("ugh, no signal, where
*are* you?") with the exact same technology that saves lives in a flood.

## 4. Where the network never reaches — the backcountry, the sea, the remote

Huge parts of the world simply **have no coverage**, permanently. Not broken —
just never built.

- **Hiking, climbing, skiing, camping, backpacking.** Deep in the mountains or a
  canyon there are no towers. A group can stay in touch across ridgelines, send an
  **SOS with GPS** if someone is hurt, and **find each other** if the party gets
  separated — the free, peer-to-peer answer to a satellite messenger.
- **Boating, sailing, kayaking.** Coverage ends a few miles offshore. Boats in a
  group, or a fleet at anchor, can message and share positions; a **man-overboard
  SOS with coordinates** is exactly the kind of message that has to get through.
- **Rural and remote communities**, and **regions where mobile data is patchy or
  expensive.** A village, a valley, or an island can run a standing local mesh
  that costs nothing and needs no carrier.
- **Where signal can't physically reach** — deep inside buildings, basements,
  parking structures, ships, mines, and tunnels — short-range device-to-device
  hops carry a message out to where it can escape.
- **Overlanding and off-road convoys.** Vehicles strung out across dead zones keep
  a group thread and live positions so no one gets lost between checkpoints.

## 5. When you want privacy — messages no one else can read

Sometimes the network is fine; you just **don't want anyone reading your
messages** — not a carrier, not a platform, not whoever later gets hold of the
logs.

- **End-to-end encryption** with audited cryptography: only the intended
  recipient can read a message, and each message is **forward-secret**, so a key
  compromised later can't unlock what was already said.
- **Sealed sender** hides *who sent* a message from the devices that carry it, and
  **rendezvous addressing** hides *who's receiving* — Lifeline is built to leak as
  little metadata as possible, not just message contents.
- **No account and no central server** means there's no company sitting on a
  searchable history of your conversations and contacts.
- A **panic wipe** lets you destroy your keys and history instantly if a device is
  about to fall into the wrong hands.

Useful for sensitive personal, medical, legal, or organizational conversations —
anywhere "please don't let this be logged forever on someone else's computer" is
the whole point.

## 6. Coordinating groups and finding people

Any time a **group has to stay coordinated across an area with poor or no
coverage**, the same primitives — group threads, live location, SOS, geocast,
find-each-other — do the work.

- **Search-and-rescue teams** sweeping terrain: share found-locations, keep a
  team channel alive with no infrastructure, and mark points with coordinates.
- **Event and venue staff** — security, medics, stage crew — coordinating across a
  site where radios are clunky and the public network is jammed.
- **Field trips, tour groups, expeditions:** a leader keeps a live picture of
  where everyone is and can send the whole group a location to converge on.
- **Humanitarian and NGO operations, refugee camps, disaster-relief zones**, where
  the local infrastructure is destroyed or was never there — a mesh stands up in
  minutes with just the phones people already carry.

## 7. Everyday resilience and preparedness

You don't have to be in a crisis to want a network that **doesn't depend on
anyone else staying online**.

- **Neighbourhood / mutual-aid mesh.** A community that pre-connects its phones
  has a communication layer ready the instant a storm or outage knocks out the
  towers — no scramble, it just keeps working.
- **Power and grid outages.** Cell towers run on backup batteries that die within
  hours of a blackout. A phone-to-phone mesh outlives them.
- **Travelling abroad without roaming.** A group of travellers with no local data
  plan can still message each other and share locations locally.
- **A family safety plan.** A shared "we're safe" broadcast and a known way to send
  an SOS-with-location that doesn't rely on the network being up when it matters.

---

## Why one tool covers all of this

Every case above is really the same problem wearing different clothes: **the
usual path is unavailable, so the message has to find its own way there.**
Lifeline's core does exactly that, and the safety features layer on top:

| Need | Feature |
|---|---|
| Reach someone with no towers / no internet | **Device-to-device mesh** (store-carry-forward) |
| Get word out when only one person has signal | **One gateway lights the mesh** |
| "I'm hurt / lost / trapped — here" | **SOS with GPS + battery**, one tap |
| Find your people in a crowd or the wild | **Find each other** (distance + direction) |
| Warn everyone in an area | **Geocast** (address a region, not a person) |
| Keep it private, even from carriers | **End-to-end encryption + sealed sender + rendezvous addressing** |
| No identity to block or hand over | **No accounts** — a key, generated on your device |
| Device seized or coerced | **Panic wipe** |

## Learn more

- [WHITEPAPER.md](../WHITEPAPER.md) — the plain-English white paper (start here).
- [docs/whitepaper/](whitepaper/) — the technical white paper and analysis.
- [ARCHITECTURE.md](../ARCHITECTURE.md) — how the system is built.
- [SECURITY.md](../SECURITY.md) — the honest threat model and how to report issues.
- [ROADMAP-location.md](ROADMAP-location.md) — what's left to make the location / find-each-other story production-real.
