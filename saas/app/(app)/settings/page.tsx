import { requireUser } from '@/lib/auth';
import { updateProfile, changePassword, deleteAccount } from '@/lib/actions';
import { StatefulForm } from '@/components/forms';
import { DangerButton } from '@/components/DangerButton';

export default async function Settings() {
  const user = await requireUser();
  return (
    <>
      <h1 className="page-h">Settings</h1>
      <p className="page-sub">Manage your profile, password, and account.</p>

      <div className="card" style={{ marginBottom: 18 }}>
        <h2>Profile</h2>
        <p className="muted" style={{ fontSize: 13.5, margin: '2px 0 16px' }}>Your name and sign-in email.</p>
        <StatefulForm action={updateProfile} submit="Save changes">
          <div className="field"><label htmlFor="name">Name</label>
            <input className="input" id="name" name="name" defaultValue={user.name} required /></div>
          <div className="field"><label htmlFor="email">Email</label>
            <input className="input" id="email" name="email" type="email" defaultValue={user.email} required /></div>
        </StatefulForm>
      </div>

      <div className="card" style={{ marginBottom: 18 }}>
        <h2>Password</h2>
        <p className="muted" style={{ fontSize: 13.5, margin: '2px 0 16px' }}>Change your password. You’ll stay signed in on this device.</p>
        <StatefulForm action={changePassword} submit="Change password">
          <div className="field"><label htmlFor="current">Current password</label>
            <input className="input" id="current" name="current" type="password" autoComplete="current-password" required /></div>
          <div className="field"><label htmlFor="password">New password</label>
            <input className="input" id="password" name="password" type="password" autoComplete="new-password" required /></div>
        </StatefulForm>
      </div>

      <div className="card" style={{ borderColor: 'color-mix(in srgb,var(--sos) 24%,var(--hairline))' }}>
        <h2 style={{ color: 'var(--sos)' }}>Danger zone</h2>
        <p className="muted" style={{ fontSize: 13.5, margin: '2px 0 16px' }}>Permanently delete your account and sign out everywhere. This cannot be undone.</p>
        <DangerButton action={deleteAccount} label="Delete account" confirm="Delete your account permanently? This cannot be undone." />
      </div>
    </>
  );
}
