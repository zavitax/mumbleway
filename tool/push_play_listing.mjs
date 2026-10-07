/**
 * Put the Google Play listing text that this repository holds into the console.
 *
 *     node tool/push_play_listing.mjs <key.json>                     # what differs; change nothing
 *     node tool/push_play_listing.mjs <key.json> --push              # stage it in an edit
 *     node tool/push_play_listing.mjs <key.json> --push --commit     # and make it live
 *
 * The third of three, after `tool/push_ms_store_listing.py` and
 * `tool/push_app_store_listing.mjs`, and the read counterpart is
 * `tool/read_play_listing.mjs`.
 *
 * ## Play is not like the other two, and the difference decides when to run it
 *
 * On the App Store and the Microsoft Store the description belongs to a version:
 * it goes live when that version does, so writing tomorrow's words today is
 * harmless -- nobody sees them until the build they describe ships.
 *
 * **A Play listing is not versioned.** It is one page per language for the whole
 * app, and committing an edit publishes it immediately, to everybody, whatever
 * is on the production track. So pushing a description that names features only
 * an unreleased build has advertises something a visitor cannot get: they read
 * about proxy support, install what production offers, and do not find it.
 *
 * Which is a real risk here rather than a theoretical one. Measured on
 * 2026-10-07: production and beta were on 1.0.1 build 144 and 1.1.0 had gone to
 * the **internal** track only, while `docs/STORE_DESCRIPTION.md` had described
 * the proxy and the administration features since 6 October.
 *
 * So `--commit` refuses while the production track is behind the version in
 * `app/pubspec.yaml`, and `--anyway` overrides it for the case where that is
 * genuinely wanted. Release notes are not affected and need nothing from this
 * script: `publish.yml` uploads `distribution/whatsnew/` with the bundle, per
 * track, which is why Play was the one store whose notes never drifted.
 *
 * ## Safety
 *
 * An edit is opened for every run, including a reporting one, because
 * `listings.get` requires one. It is deleted in a `finally` and committed only
 * behind `--commit`; a discarded edit changes nothing. That is `tool/play.mjs`'s
 * `withEdit`, and the guarantee is structural rather than a matter of care.
 *
 * `title`, `shortDescription` and `fullDescription` are written. Graphics,
 * screenshots, pricing, countries and the data-safety form are not touched.
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { withEdit } from './play.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

const argv = process.argv.slice(2);
const flags = new Set(argv.filter((a) => a.startsWith('--')));
const positional = argv.filter((a) => !a.startsWith('--'));
const KEY = positional[0] ?? process.env.GOOGLE_APPLICATION_CREDENTIALS;
const PKG = positional[1] ?? 'com.mumbleway.mumbleway';
const PUSH = flags.has('--push');
const COMMIT = flags.has('--commit');
const ANYWAY = flags.has('--anyway');

if (!KEY) {
  console.error('usage: node tool/push_play_listing.mjs <key.json> [packageName] [--push] [--commit]');
  console.error('   or: set GOOGLE_APPLICATION_CREDENTIALS');
  process.exit(2);
}
if (COMMIT && !PUSH) {
  console.error('--commit does nothing without --push.');
  process.exit(2);
}

// Play's own limits. `tool/check_listing.py` measures the copy against these
// too; this checks again because the console rejects an over-long field with a
// 400 that names the field and not the length.
const LIMITS = { title: 30, shortDescription: 80, fullDescription: 4000 };

/** Every fenced block, paired with the nearest heading above it. */
function blocks(text) {
  const fold = (s) => s.replace(/[—–]/g, '-');
  const found = new Map();
  let heading = null;
  let inFence = false;
  let body = [];
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith('##')) {
      if (!inFence) heading = fold(line.trim());
    } else if (line.startsWith('```')) {
      if (inFence) {
        if (!found.has(heading)) found.set(heading, body.join('\n'));
        body = [];
      }
      inFence = !inFence;
    } else if (inFence) {
      body.push(line);
    }
  }
  return found;
}

const read = (relative) => readFileSync(join(ROOT, relative), 'utf8');

// Headings carry their own limit -- "## Short description - Google Play (80)" --
// so the lookup is by prefix, the same rule `tool/check_listing.py` uses. An
// exact match silently finds nothing the moment somebody edits a bracket.
function fromDoc(relative, heading) {
  const found = blocks(read(relative));
  for (const [h, body] of found) {
    if (h && h.startsWith(heading)) return body.trim();
  }
  throw new Error(`${relative}: no fenced block under a heading starting '${heading}'`);
}

const version = read('app/pubspec.yaml').match(/^version:\s*(\d+\.\d+\.\d+)/m)?.[1];
if (!version) throw new Error('app/pubspec.yaml has no version line');

const LISTING = 'docs/STORE_LISTING.md';
const DESCRIPTION = 'docs/STORE_DESCRIPTION.md';

// `title` is deliberately absent. It is the app's name, it has not changed since
// the first submission, and it is the one field in `docs/STORE_LISTING.md` that
// lives in a table rather than a fenced block -- so writing it would mean
// parsing that table for a value that is already right in the console.
const wanted = {
  'en-US': {
    shortDescription: fromDoc(LISTING, '## Short description - Google Play'),
    fullDescription: fromDoc(DESCRIPTION, '## Description'),
  },
  'ru-RU': {
    shortDescription: fromDoc(LISTING, '## Russian short description - Google Play'),
    fullDescription: fromDoc(DESCRIPTION, '## Russian description'),
  },
};

for (const [lang, fields] of Object.entries(wanted)) {
  for (const [field, text] of Object.entries(fields)) {
    if (text.length > LIMITS[field]) {
      throw new Error(`${lang} ${field}: ${text.length} characters, limit ${LIMITS[field]}`);
    }
  }
}

const tidy = (s) => (s ?? '').replace(/\r\n/g, '\n').trim();
const key = JSON.parse(readFileSync(KEY, 'utf8'));

await withEdit(key, PKG, async (edit) => {
  console.log(`${PKG} — this repository is at ${version}\n`);

  // What production actually offers, which decides whether this text is honest.
  const tracks = await edit.get('/tracks');
  const production = (tracks.tracks ?? []).find((t) => t.track === 'production');
  const liveNames = (production?.releases ?? [])
    .flatMap((r) => r.versionCodes?.length ? [r.name ?? '(unnamed)'] : [])
    .filter(Boolean);
  const live = liveNames[0] ?? null;
  console.log(`production track: ${live ?? 'nothing released'}`);

  const behind = live ? !live.startsWith(version) : true;
  if (behind) {
    console.log(`  ** production does not have ${version} yet **`);
    console.log('  A Play listing is not versioned: committing publishes this text to');
    console.log('  everyone at once, including visitors who can only install what is');
    console.log('  on production. See the note at the top of this file.');
  }
  console.log();

  let changes = 0;
  const staged = [];
  for (const [lang, fields] of Object.entries(wanted)) {
    const current = await edit.get(`/listings/${lang}`);
    console.log(`--- ${lang} ---`);
    const patch = {};
    for (const [field, text] of Object.entries(fields)) {
      if (tidy(current[field]) === tidy(text)) {
        console.log(`  ${field}: already current (${text.length} chars)`);
      } else {
        console.log(`  ${field}: ${tidy(current[field]).length} in the console -> ${text.length} here`);
        patch[field] = text;
        changes += 1;
      }
    }
    if (Object.keys(patch).length) staged.push([lang, patch]);
  }
  console.log();

  if (!changes) {
    console.log('Nothing to do: the console already has this text.');
    return;
  }
  if (!PUSH) {
    console.log(`${changes} field(s) differ. The edit was discarded; nothing changed.`);
    console.log('--push stages them, --push --commit publishes them.');
    return;
  }

  for (const [lang, patch] of staged) {
    await edit.patch(`/listings/${lang}`, patch);
    console.log(`staged ${Object.keys(patch).join(', ')} for ${lang}`);
  }

  if (!COMMIT) {
    console.log('\nStaged in an edit that is about to be discarded, because --commit was');
    console.log('not given. Play has no draft to leave open the way the Microsoft Store');
    console.log('does: an edit is either committed or it never happened. Re-run with');
    console.log('--push --commit to publish.');
    return;
  }
  if (behind && !ANYWAY) {
    console.log(`\nRefusing to commit: production is on ${live ?? 'nothing'} and this text`);
    console.log(`describes ${version}. Promote ${version} to production first, or pass`);
    console.log('--anyway if advertising it ahead of the release is what you want.');
    return;
  }

  await edit.commit();
  console.log('\nCommitted. A Play listing goes live without review, so this is public now.');
});
