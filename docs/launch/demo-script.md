# Demo script & storyboard (#109)

The single highest-leverage marketing asset. Goal: make "phone-to-phone with no
internet" **undeniable** in under 20 seconds.

## The shot that proves it
Show **airplane mode ON** on every device, then a message arriving anyway —
ideally relayed through a **third** device so the two endpoints are demonstrably
out of range of each other. Direct Bluetooth is easy to dismiss ("that's just
Bluetooth"); a *relayed* delivery shows the mesh.

## Setup
- 3 devices (or 3 nodes): **A** (sender), **B** (relay, in the middle), **C** (recipient).
- A and C physically far enough apart that they can't reach each other directly.
- All in airplane mode / no SIM / Wi-Fi off — visibly, on camera.
- Optional: a caption overlay naming each device's role.

## Storyboard (≈18s loop)
1. **0–3s** — Close-up: toggle airplane mode ON on A, B, C. Hold so it's unmistakable.
2. **3–6s** — Wide shot: A and C at opposite ends; B in between. Label distances.
3. **6–11s** — On A, type "Are you okay?" and hit send. Show the mesh indicator /
   hop animation. B lights up as it relays.
4. **11–15s** — C receives the message. Show the delivered ✓ (proof of delivery).
5. **15–18s** — Cut back to A: the signed read-receipt has traveled back. Freeze
   on both screens side by side.

## Deliverables
- [ ] **15–20s silent GIF** (looping) — README hero + website. Keep it < 5 MB;
      optimize with `gifski` or export a small MP4 and also a GIF.
- [ ] **~60s narrated MP4** — for Show HN / social. Add captions (most people
      watch muted).
- [ ] Raw clips archived somewhere durable.

## Production tips
- Screen-record the phones (scrcpy for Android; QuickTime for iOS) for crisp UI,
  and intercut with one real-world wide shot for credibility.
- Show real timestamps; don't fake latency.
- If native BLE isn't shipped yet, be honest: demo the current relay/LAN path and
  label it as such, or use the node CLI across machines with networking disabled
  between the endpoints. **Do not stage a capability that doesn't exist** — a
  debunked demo is fatal on HN.

## Where it goes once made
- README (top, under the wordmark) — replace the placeholder from #107.
- Website hero (`website/`) — optional secondary placement.
- Show HN first comment, Reddit posts, social pins.
