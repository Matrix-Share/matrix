'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

const ICONS: Record<string, React.ReactNode> = {
  '/dashboard': <path d="M3 12l9-9 9 9M5 10v10h14V10" />,
  '/team': <><circle cx="9" cy="8" r="3" /><path d="M3 20a6 6 0 0 1 12 0M16 6a3 3 0 0 1 0 6M22 20a6 6 0 0 0-4-5.7" /></>,
  '/billing': <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="M3 10h18" /></>,
  '/settings': <><circle cx="12" cy="12" r="3" /><path d="M12 3v3M12 18v3M3 12h3M18 12h3" /></>,
  '/admin': <path d="M12 3l8 4v5c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V7z" />,
};

export function SideNav({ items }: { items: { href: string; label: string }[] }) {
  const path = usePathname();
  return (
    <>
      {items.map((it) => {
        const on = path === it.href || path.startsWith(it.href + '/');
        return (
          <Link key={it.href} href={it.href} className={`side-link${on ? ' on' : ''}`}>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              {ICONS[it.href]}
            </svg>
            {it.label}
          </Link>
        );
      })}
    </>
  );
}
