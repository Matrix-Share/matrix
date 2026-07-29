import { requireUser } from '@/lib/auth';
import { orgs, memberships, invites } from '@/lib/db';
import { inviteMember, createOrg, removeMember } from '@/lib/actions';
import { StatefulForm } from '@/components/forms';

const COLORS = ['#FF9500', '#FF3B30', '#34C759', '#007AFF', '#5856D6', '#AF52DE', '#FF2D55', '#30B0C7'];
const color = (s: string) => { let h = 0; for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0; return COLORS[h % COLORS.length]; };
const initials = (s: string) => (s || '?').trim().slice(0, 2).toUpperCase();

export default async function Team({ searchParams }: { searchParams: Promise<{ org?: string }> }) {
  const user = await requireUser();
  const myOrgs = orgs.forUser(user.id);
  const { org: orgParam } = await searchParams;
  const current = (orgParam && orgs.byId(orgParam)) || myOrgs[0];

  if (!current) {
    return (
      <>
        <h1 className="page-h">Team</h1>
        <p className="page-sub">Create a workspace to invite people and manage roles.</p>
        <div className="card" style={{ maxWidth: 420 }}>
          <h2>New workspace</h2>
          <StatefulForm action={createOrg} submit="Create workspace">
            <div className="field"><label htmlFor="name">Workspace name</label><input className="input" id="name" name="name" placeholder="Rescue Team" required /></div>
          </StatefulForm>
        </div>
      </>
    );
  }

  const members = memberships.forOrg(current.id);
  const pending = invites.forOrg(current.id);
  const me = memberships.get(current.id, user.id);
  const canManage = me?.role === 'owner' || me?.role === 'admin';

  return (
    <>
      <h1 className="page-h">Team</h1>
      <p className="page-sub">{current.name} · {members.length} member{members.length === 1 ? '' : 's'}</p>

      {myOrgs.length > 1 && (
        <div className="row" style={{ gap: 8, marginBottom: 20, flexWrap: 'wrap' }}>
          {myOrgs.map((o) => (
            <a key={o.id} href={`/team?org=${o.id}`} className={`badge ${o.id === current.id ? 'accent' : ''}`} style={{ padding: '6px 12px' }}>{o.name}</a>
          ))}
        </div>
      )}

      <div className="card" style={{ marginBottom: 18 }}>
        <h2>Members</h2>
        <div style={{ marginTop: 8 }}>
          {members.map((m) => (
            <div className="list-row" key={m.user_id}>
              <div className="avatar" style={{ background: color(m.email) }}>{initials(m.name)}</div>
              <div className="grow" style={{ minWidth: 0 }}>
                <div style={{ fontWeight: 590, fontSize: 14.5 }}>{m.name}{m.user_id === user.id ? ' (you)' : ''}</div>
                <div className="muted" style={{ fontSize: 12.5 }}>{m.email}</div>
              </div>
              <span className="badge">{m.role}</span>
              {canManage && m.role !== 'owner' && m.user_id !== user.id && (
                <form action={removeMember.bind(null, current.id, m.user_id)}>
                  <button className="btn btn-plain sm" style={{ color: 'var(--sos)' }}>Remove</button>
                </form>
              )}
            </div>
          ))}
        </div>
      </div>

      {canManage && (
        <div className="card" style={{ marginBottom: 18 }}>
          <h2>Invite a teammate</h2>
          <p className="muted" style={{ fontSize: 13.5, margin: '2px 0 14px' }}>They’ll get an email with a link to join {current.name}.</p>
          <StatefulForm action={inviteMember} submit="Send invite">
            <input type="hidden" name="orgId" value={current.id} />
            <div className="row" style={{ gap: 10, alignItems: 'flex-end' }}>
              <div className="field grow" style={{ margin: 0 }}><label htmlFor="email">Email</label><input className="input" id="email" name="email" type="email" placeholder="teammate@example.com" required /></div>
              <div className="field" style={{ margin: 0, width: 130 }}><label htmlFor="role">Role</label>
                <select className="input" id="role" name="role" defaultValue="member"><option value="member">Member</option><option value="admin">Admin</option></select></div>
            </div>
          </StatefulForm>
          {pending.length > 0 && (
            <div style={{ marginTop: 16 }}>
              <div className="sectlabel" style={{ marginTop: 0 }}>Pending invites</div>
              {pending.map((i) => (
                <div className="list-row" key={i.token}><div className="grow">{i.email}</div><span className="badge">{i.role}</span></div>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="card">
        <h2>New workspace</h2>
        <StatefulForm action={createOrg} submit="Create workspace">
          <div className="field"><label htmlFor="name">Workspace name</label><input className="input" id="name" name="name" placeholder="Another team" required /></div>
        </StatefulForm>
      </div>
    </>
  );
}
