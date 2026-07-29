import 'server-only';

/**
 * Transactional email. If RESEND_API_KEY is set, send via Resend's HTTP API
 * (no SDK dependency). Otherwise, log the message to the server console — so
 * password-reset and invite links are usable in local dev without a provider.
 */
export async function sendEmail(to: string, subject: string, text: string): Promise<void> {
  const key = process.env.RESEND_API_KEY;
  const from = process.env.EMAIL_FROM || 'Lifeline <noreply@lifeline.app>';
  if (!key) {
    console.log(`\n[email → ${to}] ${subject}\n${text}\n`);
    return;
  }
  try {
    await fetch('https://api.resend.com/emails', {
      method: 'POST',
      headers: { Authorization: `Bearer ${key}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ from, to, subject, text }),
    });
  } catch (e) {
    console.error('email send failed', e);
  }
}
