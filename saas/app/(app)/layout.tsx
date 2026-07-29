import Link from 'next/link';
import { requireUser } from '@/lib/auth';
import { logout } from '@/lib/actions';
import { SideNav } from '@/components/SideNav';

export default async function AppLayout({ children }: { children: React.ReactNode }) {
  const user = await requireUser();
  const items = [
    { href: '/dashboard', label: 'Dashboard' },
    { href: '/team', label: 'Team' },
    { href: '/billing', label: 'Billing' },
    { href: '/settings', label: 'Settings' },
    ...(user.role === 'admin' ? [{ href: '/admin', label: 'Admin' }] : []),
  ];
  return (
    <div className="app">
      <aside className="sidebar">
        <Link href="/" className="brand" style={{ fontSize: 17, padding: '4px 12px 14px' }}><span className="mark" style={{ width: 24, height: 24 }} />Lifeline</Link>
        <SideNav items={items} />
        <div className="grow" />
        <div className="side-foot">
          <div className="side-name" style={{ fontSize: 13, fontWeight: 590 }}>{user.name}</div>
          <div className="side-email muted" style={{ fontSize: 12, overflow: 'hidden', textOverflow: 'ellipsis' }}>{user.email}</div>
          <form action={logout}><button className="btn btn-plain sm" style={{ paddingLeft: 0, marginTop: 6 }}>Log out</button></form>
        </div>
      </aside>
      <main className="main">{children}</main>
    </div>
  );
}
