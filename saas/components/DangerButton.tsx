'use client';

/** A destructive action button that asks for confirmation before submitting its
 *  bound server action. */
export function DangerButton({ action, label, confirm }: { action: () => Promise<void>; label: string; confirm: string }) {
  return (
    <form action={action} onSubmit={(e) => { if (!window.confirm(confirm)) e.preventDefault(); }}>
      <button className="btn btn-danger sm" type="submit">{label}</button>
    </form>
  );
}
