# Deploying relayne.org (Vercel + Namecheap)

The marketing site is a **static** site in this `website/` folder. No build step.
`vercel.json` (next to this file) sets security headers + caching; Vercel auto-serves
`404.html` for unknown paths.

## 1. Vercel project (one-time)

Dashboard route (easiest):

1. Push the repo to GitHub (`matrix-share/matrix`) and make it public when ready.
2. vercel.com → **Add New… → Project** → import `matrix-share/matrix`.
3. In **Configure Project**, set:
   - **Root Directory**: `website`
   - **Framework Preset**: Other
   - **Build Command**: *(leave empty)*
   - **Output Directory**: *(leave empty — serves `website/` as-is)*
   - **Install Command**: *(leave empty)*
4. **Deploy**. You'll get a `relayne-xxxx.vercel.app` URL to smoke-test.

CLI alternative (from this `website/` folder, after `vercel login`):

```bash
vercel        # preview deploy, links the project
vercel --prod # production deploy
```

Pick **Root Directory = website** when the CLI prompts (or run the commands from inside `website/`).

## 2. Point relayne.org at it

In Vercel: **Project → Settings → Domains → Add** `relayne.org` **and** `www.relayne.org`.
Vercel then shows the exact DNS records. Add them at **Namecheap → Domain List →
relayne.org → Manage → Advanced DNS**. Typical values (confirm against what Vercel shows you):

| Type  | Host | Value                  |
|-------|------|------------------------|
| A     | `@`  | `76.76.21.21`          |
| CNAME | `www`| `cname.vercel-dns.com` |

- Remove any existing parking/placeholder A or CNAME records on `@` and `www` first.
- Vercel issues the TLS certificate automatically once DNS resolves (minutes to ~an hour).
- Set the **primary domain** in Vercel (apex `relayne.org`) and let it redirect `www` → apex.

## 3. After it's live — verify

- `https://relayne.org` loads, dark/light toggle works, product screenshots show.
- `https://relayne.org/doesnotexist` → the custom 404.
- Paste `https://relayne.org` into the [OpenGraph debugger](https://www.opengraph.xyz/)
  to confirm the `social-preview.png` card renders (title, tagline, warm image).
- Headers check: `curl -sI https://relayne.org | grep -i -E 'content-security|strict-transport|x-content-type'`

## Notes

- GitHub links across the site intentionally still point to `github.com/matrix-share/matrix`
  (repo kept under that name for now). If/when the repo is renamed, update those links.
- The `saas/` Next.js app is a **separate** Vercel project (it has a build step and needs
  `DATABASE_URL` etc. from its own `.env`). Don't point the apex domain at it; give it a
  subdomain like `app.relayne.org` later.
