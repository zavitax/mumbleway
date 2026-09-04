/**
 * Read the App Store and Mac App Store listings back out of App Store Connect.
 *
 *     node tool/read_app_store_listing.mjs
 *
 * The counterpart to `tool/read_play_listing.mjs`, and it exists for the same
 * reason: `tool/check_listing.py` measures the copy in this repository against
 * each store's limits and cannot know whether that copy was ever pasted into a
 * store. **The keyword fields in particular are visible nowhere else** — Apple
 * never publishes them, so without this they are only what STORE_LISTING.md
 * remembers.
 *
 * **Read-only.** Every request here is a GET. Nothing creates a version, and
 * nothing submits anything; adding either would make this a script nobody dares
 * run. Keep it that way.
 *
 * ## Why this is not the MCP server
 *
 * The `app-store-connect` MCP is refused a collection read on version
 * localizations — `The resource 'appStoreVersionLocalizations' does not allow
 * 'GET_COLLECTION'`. That is not a permissions problem, it is the wrong URL:
 * Apple offers no top-level collection there, only the relationship under a
 * version. This walks the relationships instead, and gets everything.
 *
 * ## Credentials
 *
 * Taken from whatever the `app-store-connect` MCP server is already configured
 * with in `~/.claude.json` — key id, issuer id and the path to the `.p8` —
 * rather than asking for them again or, worse, keeping a second copy. They are
 * read straight into the JWT and never printed. `zavitax/mumbleway` is public;
 * nothing here may ever echo them.
 *
 * Override with APP_STORE_CONNECT_KEY_ID, APP_STORE_CONNECT_ISSUER_ID and
 * APP_STORE_CONNECT_P8_PATH if the key lives somewhere else.
 */

import { readFileSync } from 'node:fs';
import { createSign } from 'node:crypto';
import { homedir } from 'node:os';
import { join } from 'node:path';

const APP_ID = process.argv[2] ?? '6797305046';
const API = 'https://api.appstoreconnect.apple.com/v1';

function credentials() {
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

function token() {
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

const jwt = token();

async function get(path) {
  const url = path.startsWith('http') ? path : `${API}${path}`;
  const r = await fetch(url, { headers: { authorization: `Bearer ${jwt}` } });
  const text = await r.text();
  let json = null;
  try { json = text ? JSON.parse(text) : null; } catch {}
  if (!r.ok) {
    const why = json?.errors?.map((e) => e.detail ?? e.title).join('; ') ?? text.slice(0, 200);
    throw new Error(`HTTP ${r.status} on ${path} — ${why}`);
  }
  return json;
}

const LIMITS = { name: 30, subtitle: 30, keywords: 100, promotionalText: 170, description: 4000, whatsNew: 4000 };
const measure = (field, value) => {
  const n = (value ?? '').length;
  const limit = LIMITS[field];
  return limit ? `${n}/${limit}${n > limit ? '  OVER' : ''}` : `${n}`;
};

// Name and subtitle live on appInfos, not on a version: they are the same
// across platforms and change without a release.
const infos = await get(`/apps/${APP_ID}/appInfos`);
for (const info of infos.data) {
  const locs = await get(`/appInfos/${info.id}/appInfoLocalizations`);
  console.log(`\n===== APP INFO (${info.attributes.state}) =====`);
  for (const l of locs.data) {
    const a = l.attributes;
    console.log(`  --- ${a.locale} ---`);
    console.log(`  name       ${measure('name', a.name)}   ${a.name ?? ''}`);
    console.log(`  subtitle   ${measure('subtitle', a.subtitle)}   ${a.subtitle ?? ''}`);
  }
}

const versions = await get(`/apps/${APP_ID}/appStoreVersions?limit=10`);
for (const v of versions.data) {
  const a = v.attributes;
  console.log(`\n===== ${a.platform}  ${a.versionString}  ${a.appStoreState} =====`);
  const locs = await get(`/appStoreVersions/${v.id}/appStoreVersionLocalizations`);
  for (const l of locs.data) {
    const t = l.attributes;
    console.log(`\n  --- ${t.locale} ---`);
    console.log(`  keywords         ${measure('keywords', t.keywords)}`);
    console.log(`    ${t.keywords ?? '(empty)'}`);
    console.log(`  promotionalText  ${measure('promotionalText', t.promotionalText)}`);
    console.log(`  description      ${measure('description', t.description)}`);
    console.log(`  whatsNew         ${measure('whatsNew', t.whatsNew)}`);
    console.log(`    ${(t.whatsNew ?? '(empty)').split('\n')[0]}`);
    // The one number this project has already had wrong in six places.
    const stale = /paid back to 60 ms|снижается до 60 мс/.test(t.description ?? '');
    if (stale) console.log('  *** description still says 60 ms; FLOOR_MS is 200 ***');
  }
}
