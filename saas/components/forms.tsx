'use client';

import { useActionState } from 'react';

type State = { error?: string; ok?: string };
type Action = (state: State, form: FormData) => Promise<State>;

/** A form wired to a server action, rendering inline error/success alerts and a
 *  pending state on the submit button. */
export function StatefulForm({
  action, submit, children,
}: {
  action: Action;
  submit: string;
  children: React.ReactNode;
}) {
  const [state, formAction, pending] = useActionState<State, FormData>(action, {});
  return (
    <form action={formAction}>
      {state.error && <div className="alert alert-err">{state.error}</div>}
      {state.ok && <div className="alert alert-ok">{state.ok}</div>}
      {children}
      <button className="btn btn-primary wide" disabled={pending} style={{ marginTop: 6 }}>
        {pending ? 'Please wait…' : submit}
      </button>
    </form>
  );
}
