import { requireAdmin } from '@/lib/auth';
import { users, orgs } from '@/lib/db';

const fmt = (ts: number) => new Date(ts).toLocaleDateString();

export default async function Admin() {
  await requireAdmin();
  const allUsers = users.all();
  const allOrgs = orgs.all();

  return (
    <>
      <h1 className="page-h">Admin</h1>
      <p className="page-sub">Everyone and everything on this instance.</p>

      <div className="tiles" style={{ marginBottom: 24 }}>
        <div className="tile"><div className="k">Users</div><div className="v">{allUsers.length}</div></div>
        <div className="tile"><div className="k">Workspaces</div><div className="v">{allOrgs.length}</div></div>
        <div className="tile"><div className="k">Paid workspaces</div><div className="v">{allOrgs.filter((o) => o.plan !== 'free').length}</div></div>
      </div>

      <div className="card" style={{ marginBottom: 18 }}>
        <h2>Users</h2>
        <div style={{ marginTop: 8 }}>
          {allUsers.map((u) => (
            <div className="list-row" key={u.id}>
              <div className="grow" style={{ minWidth: 0 }}>
                <div style={{ fontWeight: 590, fontSize: 14.5 }}>{u.name}</div>
                <div className="muted" style={{ fontSize: 12.5 }}>{u.email}</div>
              </div>
              {u.role === 'admin' && <span className="badge accent">admin</span>}
              <span className="muted" style={{ fontSize: 12.5 }}>{fmt(u.created_at)}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="card">
        <h2>Workspaces</h2>
        <div style={{ marginTop: 8 }}>
          {allOrgs.map((o) => (
            <div className="list-row" key={o.id}>
              <div className="grow" style={{ minWidth: 0 }}>
                <div style={{ fontWeight: 590, fontSize: 14.5 }}>{o.name}</div>
                <div className="muted mono" style={{ fontSize: 12 }}>{o.slug}</div>
              </div>
              <span className={`badge ${o.plan !== 'free' ? 'accent' : ''}`}>{o.plan}</span>
              <span className="muted" style={{ fontSize: 12.5 }}>{fmt(o.created_at)}</span>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
