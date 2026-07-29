# Lifeline — SaaS (marketing site + accounts + billing)

The web product around the open-source mesh: a public marketing site, user
accounts, a workspace dashboard, teams, and subscription billing. The **mesh
messenger itself stays accountless** — this SaaS is the hosted layer on top
(managed relays, team management, billing).

Built with **Next.js 15 (App Router) + TypeScript**, the shared iOS design system
(`app/globals.css`), and **zero heavy dependencies**: data is Node's built-in
`node:sqlite` (no native module) and auth is a hand-rolled scrypt + cookie-session
system — so it builds and runs anywhere Node 22+ runs.

## Features
- **Marketing homepage** at `/` (hero, features, pricing) + `/pricing`.
- **Auth:** signup, login, logout, password reset (email or dev-console link).
  scrypt password hashing, httpOnly cookie sessions, no-enumeration reset.
- **Dashboard** (`/dashboard`) — workspace at a glance + links into the mesh app.
- **Account settings** (`/settings`) — profile, change password, delete account.
- **Teams / orgs** (`/team`) — multiple workspaces, roles (owner/admin/member),
  email invites, member management.
- **Billing** (`/billing`) — plans, Stripe Checkout + customer portal, plan gating.
  Runs in **test mode** without keys (checkout disabled, everyone on Community).
- **Admin** (`/admin`) — the first user to sign up becomes admin; sees all users
  and workspaces.

## Run it
```bash
cd saas
npm install
cp .env.example .env.local     # optional: fill in Stripe / email keys
npm run dev                    # http://localhost:3000
```
Everything works with **no configuration** — SQLite is created under `data/`, and
Stripe/email degrade to test-mode / console logging until you add keys.

## Enable billing (optional)
Set `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, and `STRIPE_PRICE_PRO` /
`STRIPE_PRICE_TEAM` in `.env.local`. Point a Stripe webhook at
`/api/stripe/webhook`. Checkout, the customer portal, and plan sync then activate.

## Going to production
The data layer (`lib/db.ts`) is deliberately plain SQL; swapping `node:sqlite` for
Postgres is mechanical. For email, set `RESEND_API_KEY`. Deploy on any Node host
(Vercel, Fly, a container). Set `APP_URL` to your public origin.

## Structure
```
app/
  (marketing)/     public homepage + pricing (+ nav/footer)
  login,signup,forgot,reset,invite/[token]   auth
  (app)/           protected: dashboard, team, billing, settings, admin
  api/stripe/webhook
lib/   db.ts · auth.ts · actions.ts (server actions) · plans.ts · stripe.ts · email.ts
components/  forms.tsx · SideNav.tsx · DangerButton.tsx
middleware.ts   edge guard for /app routes
```
