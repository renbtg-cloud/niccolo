const HANDOFF_FRAGMENT = /^#handoff=([0-9a-f]{64})$/;
const TOKEN = /^ncl_[0-9a-f]{64}$/;

export function takeLocalHandoffFromFragment(locationLike, historyLike) {
  const match = HANDOFF_FRAGMENT.exec(locationLike?.hash ?? '');
  if (!match) return '';
  // Remove the capability before any network request so it cannot survive in
  // copied links, screenshots, navigation history, or later referrers.
  historyLike?.replaceState?.(
    historyLike.state ?? null,
    '',
    `${locationLike.pathname ?? '/'}${locationLike.search ?? ''}`,
  );
  return match[1];
}

export async function redeemLocalHandoff(code, fetchImpl = fetch) {
  if (!/^[0-9a-f]{64}$/.test(code)) throw new Error('invalid local handoff');
  const response = await fetchImpl('/v1/local-handoff', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ code }),
    cache: 'no-store',
    credentials: 'same-origin',
    referrerPolicy: 'no-referrer',
  });
  if (!response.ok) throw new Error('local handoff unavailable');
  const body = await response.json();
  if (!TOKEN.test(body?.token ?? '')) throw new Error('invalid local handoff response');
  return body.token;
}
