/**
 * Read the Google Play listing back out of the console.
 *
 *     node tool/read_play_listing.mjs <service-account-key.json>
 *     node tool/read_play_listing.mjs          # uses GOOGLE_APPLICATION_CREDENTIALS
 *
 * `tool/check_listing.py` measures the copy in this repository against each
 * store's limits. It cannot know whether that copy was ever pasted into a
 * store. This reads what Play actually holds, so the two can be diffed — which
 * is how the 60 ms error in `docs/STORE_SURVEY.md` was found, months after the
 * repository had been corrected.
 *
 * **Read-only, and that is enforced rather than intended.** `listings.get`
 * requires an open edit, so one is created; it is deleted in a `finally` and
 * never committed, and an uncommitted edit changes nothing on the store. There
 * is no code path here that calls `commit`. Keep it that way: a script that can
 * publish is a script nobody dares run.
 *
 * No dependencies. Node's own crypto signs the JWT, so the key is read into
 * memory and never copied anywhere — do not add a step that writes it out.
 *
 * The key is the one `publish.yml` uses, held as a repository secret. It is not
 * in this repository and must never be: `zavitax/mumbleway` is public.
 *
 * Note that `play.google.com` may be unreachable where this still works. Some
 * networks reset TLS to it on the SNI; `androidpublisher.googleapis.com` is a
 * different host and is generally not filtered.
 */

import { readFileSync } from 'node:fs';
import { createSign } from 'node:crypto';

const KEY = process.argv[2] ?? process.env.GOOGLE_APPLICATION_CREDENTIALS;
const PKG = process.argv[3] ?? 'com.mumbleway.mumbleway';
const API = 'https://androidpublisher.googleapis.com/androidpublisher/v3';

if (!KEY) {
  console.error('usage: node tool/read_play_listing.mjs <key.json> [packageName]');
  console.error('   or: set GOOGLE_APPLICATION_CREDENTIALS');
  process.exit(2);
}

// Windows note: a WSL UNC path has to be given with forward slashes
// (//wsl.localhost/...). The backslash form loses a separator passing through a
// shell and arrives as a path that cannot be opened.

const b64 = (o) =>
  Buffer.from(typeof o === 'string' ? o : JSON.stringify(o)).toString('base64url');

async function accessToken(key) {
  const now = Math.floor(Date.now() / 1000);
  const head = b64({ alg: 'RS256', typ: 'JWT' });
  const body = b64({
    iss: key.client_email,
    scope: 'https://www.googleapis.com/auth/androidpublisher',
    aud: 'https://oauth2.googleapis.com/token',
    iat: now,
    exp: now + 3600,
  });
  const sig = createSign('RSA-SHA256')
    .update(`${head}.${body}`)
    .end()
    .sign(key.private_key, 'base64url');

  const r = await fetch('https://oauth2.googleapis.com/token', {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer',
      assertion: `${head}.${body}.${sig}`,
    }),
  });
  const j = await r.json();
  // Never print the response body on failure: it can echo the assertion back.
  if (!r.ok) throw new Error(`token request failed: HTTP ${r.status} ${j.error ?? ''}`);
  return j.access_token;
}

async function call(tok, method, path) {
  const r = await fetch(`${API}${path}`, { method, headers: { authorization: `Bearer ${tok}` } });
  const text = await r.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    /* a non-JSON body is an error page; keep the text */
  }
  return { status: r.status, ok: r.ok, json, text };
}

const IMAGE_TYPES = [
  'icon',
  'featureGraphic',
  'phoneScreenshots',
  'sevenInchScreenshots',
  'tenInchScreenshots',
  'tvBanner',
  'tvScreenshots',
  'wearScreenshots',
];
// What Play requires before a production release can go out.
const REQUIRED = { icon: 1, featureGraphic: 1, phoneScreenshots: 2 };

const key = JSON.parse(readFileSync(KEY, 'utf8'));
const tok = await accessToken(key);

const opened = await call(tok, 'POST', `/applications/${PKG}/edits`);
if (!opened.ok) {
  console.error(`could not open an edit: HTTP ${opened.status}`);
  console.error(opened.text.slice(0, 600));
  process.exit(1);
}
const editId = opened.json.id;

try {
  const details = await call(tok, 'GET', `/applications/${PKG}/edits/${editId}/details`);
  console.log('===== DETAILS =====');
  console.log(JSON.stringify(details.json, null, 1));

  const tracks = await call(tok, 'GET', `/applications/${PKG}/edits/${editId}/tracks`);
  console.log('\n===== TRACKS =====');
  for (const t of tracks.json?.tracks ?? []) {
    const rel = (t.releases ?? [])
      .map((r) => `${r.name ?? '(unnamed)'} [${r.status}]`)
      .join(' · ');
    // A track with no releases is why a public page can 404 while the listing
    // in the console is complete.
    console.log(`  ${t.track.padEnd(12)} ${rel || 'no releases'}`);
    // Release notes are worth printing because the way they go missing is
    // silent: `whatsNewDirectory` matches files by locale name, and a file
    // named for a locale the listing does not have is skipped without a word.
    // "No notes" and "notes that never uploaded" look identical in the console.
    for (const r of t.releases ?? []) {
      for (const n of r.releaseNotes ?? []) {
        console.log(`    ${n.language}: ${(n.text ?? '').split('\n')[0].slice(0, 72)}`);
      }
      if (r.status === 'completed' && !(r.releaseNotes ?? []).length) {
        console.log('    (no release notes on this one)');
      }
    }
  }

  const listings = await call(tok, 'GET', `/applications/${PKG}/edits/${editId}/listings`);
  console.log('\n===== LISTINGS =====');
  for (const l of listings.json?.listings ?? []) {
    console.log(`\n--- ${l.language} ---`);
    console.log(`title             ${(l.title ?? '').length}/30`);
    console.log(`shortDescription  ${(l.shortDescription ?? '').length}/80`);
    console.log(`fullDescription   ${(l.fullDescription ?? '').length}/4000`);
    console.log(`\n${l.shortDescription ?? ''}\n`);
    console.log(l.fullDescription ?? '');
  }

  console.log('\n===== GRAPHICS =====');
  for (const l of listings.json?.listings ?? []) {
    console.log(`\n--- ${l.language} ---`);
    for (const type of IMAGE_TYPES) {
      const r = await call(
        tok,
        'GET',
        `/applications/${PKG}/edits/${editId}/listings/${l.language}/${type}`,
      );
      const n = r.ok ? (r.json?.images?.length ?? 0) : null;
      const need = REQUIRED[type];
      const note =
        n === null ? `HTTP ${r.status}` : need && n < need ? `BLOCKS PUBLISHING, needs ${need}` : '';
      console.log(`  ${type.padEnd(22)} ${String(n ?? '?').padStart(2)}  ${note}`);
    }
  }
} finally {
  const gone = await call(tok, 'DELETE', `/applications/${PKG}/edits/${editId}`);
  console.log(`\nedit deleted: HTTP ${gone.status} — nothing was committed`);
}
