/**
 * Google Play: a service-account access token, and an edit-scoped client.
 *
 * `tool/read_play_listing.mjs` predates this and keeps its own copy of the
 * token code; it is read-only and working, and churning it to save forty lines
 * is not worth the risk to a tool people trust. Move it over next time it is
 * opened for another reason.
 *
 * Two details here are the reason this is a module rather than a paste:
 *
 * - **The assertion must never be logged.** A failed token request can echo it
 *   back in the response body, and it is signed with the private key. The error
 *   below carries the status and `error` field and nothing else.
 * - **Every edit must be disposed of.** An edit left open is not harmless: it
 *   holds the listing against concurrent changes, and Play will refuse a later
 *   one. `withEdit` deletes it in a `finally`, committed or not.
 *
 * No dependencies. Node's own crypto signs the JWT, so the key is read into
 * memory and never copied anywhere -- do not add a step that writes it out.
 * `zavitax/mumbleway` is public and the key must never be in it.
 */

import { createSign } from 'node:crypto';

export const API = 'https://androidpublisher.googleapis.com/androidpublisher/v3';

const b64 = (o) =>
  Buffer.from(typeof o === 'string' ? o : JSON.stringify(o)).toString('base64url');

export async function accessToken(key) {
  const now = Math.floor(Date.now() / 1000);
  const head = b64({ alg: 'RS256', typ: 'JWT' });
  const body = b64({
    iss: key.client_email,
    scope: 'https://www.googleapis.com/auth/androidpublisher',
    aud: 'https://oauth2.googleapis.com/token',
    iat: now,
    exp: now + 3600,
  });
  const sig = createSign('RSA-SHA256').update(`${head}.${body}`).end().sign(key.private_key, 'base64url');

  const r = await fetch('https://oauth2.googleapis.com/token', {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer',
      assertion: `${head}.${body}.${sig}`,
    }),
  });
  const j = await r.json().catch(() => ({}));
  // Never print the response body on failure: it can echo the assertion back.
  if (!r.ok) throw new Error(`token request failed: HTTP ${r.status} ${j.error ?? ''}`);
  return j.access_token;
}

function requester(tok) {
  return async function call(method, path, body) {
    const headers = { authorization: `Bearer ${tok}` };
    if (body !== undefined) headers['content-type'] = 'application/json';
    const r = await fetch(`${API}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await r.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      /* a non-JSON body is an error page; keep the text */
    }
    if (!r.ok) {
      const why = json?.error?.message ?? text.slice(0, 300);
      throw new Error(`HTTP ${r.status} on ${method} ${path} — ${why}`);
    }
    return json;
  };
}

/**
 * Open an edit, hand it to `work`, and always dispose of it.
 *
 * `work` gets `{ get, patch, commit }` scoped to the edit. Nothing is live
 * until `commit` is called; if it is not, the edit is deleted and the console
 * is exactly as it was.
 */
export async function withEdit(key, pkg, work) {
  const tok = await accessToken(key);
  const call = requester(tok);
  const edit = await call('POST', `/applications/${pkg}/edits`);
  const base = `/applications/${pkg}/edits/${edit.id}`;
  let committed = false;

  try {
    return await work({
      id: edit.id,
      get: (path) => call('GET', `${base}${path}`),
      patch: (path, body) => call('PATCH', `${base}${path}`, body),
      async commit() {
        const done = await call('POST', `${base}:commit`);
        committed = true;
        return done;
      },
    });
  } finally {
    if (!committed) {
      // A discarded edit changes nothing, which is what makes a report-only
      // run safe by construction rather than by intention.
      await call('DELETE', base).catch(() => {});
    }
  }
}
