/**
 * App Store Connect: credentials, a JWT, and the three verbs.
 *
 * Extracted from `tool/read_app_store_listing.mjs` when a second tool needed the
 * same token. The ES256 detail below is the whole reason this is shared rather
 * than copied: it is a one-line mistake that answers with a 401 saying nothing
 * about why, and it should exist in one place.
 *
 * `get` is here for readers and `post`/`patch` for writers. A tool that imports
 * this is not thereby a tool that writes -- `read_app_store_listing.mjs` uses
 * `get` alone and stays read-only, which is a property of that file rather than
 * of this one.
 *
 * ## Credentials
 *
 * Taken from whatever the `app-store-connect` MCP server is already configured
 * with in `~/.claude.json` -- key id, issuer id and the path to the `.p8` --
 * rather than asking for them again or keeping a second copy. They are read
 * straight into the JWT and never printed. `zavitax/mumbleway` is public;
 * nothing here may ever echo them.
 *
 * Override with APP_STORE_CONNECT_KEY_ID, APP_STORE_CONNECT_ISSUER_ID and
 * APP_STORE_CONNECT_P8_PATH if the key lives somewhere else.
 */

import { readFileSync } from 'node:fs';
import { createSign } from 'node:crypto';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const API = 'https://api.appstoreconnect.apple.com/v1';

export function credentials() {
  const fromEnv = {
    keyId: process.env.APP_STORE_CONNECT_KEY_ID,
    issuerId: process.env.APP_STORE_CONNECT_ISSUER_ID,
    p8Path: process.env.APP_STORE_CONNECT_P8_PATH,
  };
  if (fromEnv.keyId && fromEnv.issuerId && fromEnv.p8Path) return fromEnv;

  const config = JSON.parse(readFileSync(join(homedir(), '.claude.json'), 'utf8'));
  const pools = [config.mcpServers, ...Object.values(config.projects ?? {}).map((p) => p.mcpServers)];
  for (const pool of pools) {
    for (const [name, cfg] of Object.entries(pool ?? {})) {
      if (!/app-?store/i.test(name)) continue;
      const e = cfg.env ?? {};
      if (e.APP_STORE_CONNECT_KEY_ID && e.APP_STORE_CONNECT_ISSUER_ID && e.APP_STORE_CONNECT_P8_PATH) {
        return {
          keyId: e.APP_STORE_CONNECT_KEY_ID,
          issuerId: e.APP_STORE_CONNECT_ISSUER_ID,
          p8Path: e.APP_STORE_CONNECT_P8_PATH,
        };
      }
    }
  }
  throw new Error('no App Store Connect credentials in the environment or in ~/.claude.json');
}

const b64 = (o) => Buffer.from(JSON.stringify(o)).toString('base64url');

export function token() {
  const { keyId, issuerId, p8Path } = credentials();
  const now = Math.floor(Date.now() / 1000);
  const head = b64({ alg: 'ES256', kid: keyId, typ: 'JWT' });
  const body = b64({ iss: issuerId, iat: now, exp: now + 1200, aud: 'appstoreconnect-v1' });
  // ES256 wants the raw r||s pair. Node defaults to DER, which Apple rejects
  // with a 401 that says nothing about why.
  const sig = createSign('SHA256')
    .update(`${head}.${body}`)
    .end()
    .sign({ key: readFileSync(p8Path, 'utf8'), dsaEncoding: 'ieee-p1363' })
    .toString('base64url');
  return `${head}.${body}.${sig}`;
}

/** A client bound to one token, so a long run does not re-sign per request. */
export function client(jwt = token()) {
  async function call(method, path, body) {
    const url = path.startsWith('http') ? path : `${API}${path}`;
    const headers = { authorization: `Bearer ${jwt}` };
    if (body !== undefined) headers['content-type'] = 'application/json';
    const r = await fetch(url, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await r.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {}
    if (!r.ok) {
      // Apple puts the useful sentence in `detail` and the useless one in
      // `title`, and a validation failure names the field in `source.pointer`.
      const why =
        json?.errors
          ?.map((e) => [e.detail ?? e.title, e.source?.pointer].filter(Boolean).join(' at '))
          .join('; ') ?? text.slice(0, 300);
      throw new Error(`HTTP ${r.status} on ${method} ${path} — ${why}`);
    }
    return json;
  }

  return {
    get: (path) => call('GET', path),
    post: (path, body) => call('POST', path, body),
    patch: (path, body) => call('PATCH', path, body),
  };
}
