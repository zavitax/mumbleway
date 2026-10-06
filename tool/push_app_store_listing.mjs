/**
 * Put the App Store listing text that this repository holds into App Store Connect.
 *
 *     node tool/push_app_store_listing.mjs                    # what differs; change nothing
 *     node tool/push_app_store_listing.mjs --create-version   # make the missing version records
 *     node tool/push_app_store_listing.mjs --push             # write the text into them
 *
 * The write counterpart to `tool/read_app_store_listing.mjs`, kept in a separate
 * file so that one can go on promising it never writes. The Microsoft equivalent
 * is `tool/push_ms_store_listing.py`, and the division of labour is the same:
 * this does the data entry, a person decides to submit.
 *
 * **It does not submit for review, and must not learn how.** Submitting is the
 * decision; filling sixteen fields by hand is the chore, and only the chore
 * belongs in a script. `docs/RELEASE_NOTES.md` makes the same argument about why
 * none of this lives in `publish.yml`.
 *
 * ## The version record is the gate
 *
 * Apple's release notes and description belong to a *version* record, and a
 * released version's cannot be changed -- so new text needs a new record, and a
 * record does not exist until somebody decides to ship. That is why
 * `--create-version` is its own flag: it is the decision, separated from the
 * typing.
 *
 * Measured on 2026-10-07: 1.1.0 built and uploaded to both of Apple's stores and
 * there was no 1.1.0 version record on either, so the builds sat unattached and
 * the live description was still 1.0.1's. Nothing had failed; nothing had been
 * asked to happen.
 *
 * ## What it writes, and the two fields it refuses to touch
 *
 * **`description` and `whatsNew`** come from this repository, which is the one
 * source of truth for them: the same words go to every store, cut to the
 * shortest limit, and `docs/RELEASE_NOTES.md` explains why.
 *
 * **`promotionalText` is carried forward, not pushed.** A new version record
 * clones every field except this one, which arrives empty -- the trap recorded
 * in `docs/RELEASE_NOTES.md`, where a version publishes with Apple's one
 * review-free field blank. So when it is empty this copies the previous version
 * of the *same platform*, and otherwise leaves it alone.
 *
 * **`keywords` are never written.** iOS and macOS carry deliberately different
 * sets -- 97 and 93 characters for en-US on 1.0.1, tuned per platform -- and
 * `docs/STORE_LISTING.md` holds one. Pushing it would flatten that difference
 * silently, and Apple clones keywords to a new version anyway, so there is
 * nothing to fix. They are reported and left.
 *
 * Credentials and the JWT come from `tool/asc.mjs`, and are never printed.
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { client } from './asc.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const APP_ID = process.env.APP_STORE_APP_ID ?? '6797305046';
const PLATFORMS = ['IOS', 'MAC_OS'];
const LOCALES = ['en-US', 'ru'];

const argv = process.argv.slice(2);
const CREATE = argv.includes('--create-version');
const PUSH = argv.includes('--push');

// Apple takes a metadata edit only in these states. Anything else -- in review,
// ready for sale, processing -- is refused, and saying so here beats a 409 that
// names a relationship rather than the reason.
const EDITABLE = new Set([
  'PREPARE_FOR_SUBMISSION',
  'DEVELOPER_REJECTED',
  'REJECTED',
  'METADATA_REJECTED',
  'INVALID_BINARY',
]);

const LIMITS = { description: 4000, whatsNew: 4000, promotionalText: 170 };

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
        // Only the first block under a heading counts; later ones are
        // alternatives offered in prose.
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

function fromDoc(relative, heading) {
  const found = blocks(read(relative));
  if (!found.has(heading)) throw new Error(`${relative}: no fenced block under '${heading}'`);
  return found.get(heading).trim();
}

const version = (() => {
  const line = read('app/pubspec.yaml').match(/^version:\s*(\d+\.\d+\.\d+)/m);
  if (!line) throw new Error('app/pubspec.yaml has no version line');
  return line[1];
})();

const wanted = {
  'en-US': {
    description: fromDoc('docs/STORE_DESCRIPTION.md', '## Description'),
    whatsNew: read('distribution/whatsnew/whatsnew-en-US').trim(),
  },
  ru: {
    description: fromDoc('docs/STORE_DESCRIPTION.md', '## Russian description'),
    whatsNew: read('distribution/whatsnew/whatsnew-ru-RU').trim(),
  },
};

for (const [locale, fields] of Object.entries(wanted)) {
  for (const [field, text] of Object.entries(fields)) {
    if (text.length > LIMITS[field]) {
      throw new Error(`${locale} ${field}: ${text.length} characters, limit ${LIMITS[field]}`);
    }
  }
}

const { get, post, patch } = client();
const tidy = (s) => (s ?? '').replace(/\r\n/g, '\n').trim();

console.log(`MumbleWay ${version} — app ${APP_ID}\n`);

const versions = (await get(`/apps/${APP_ID}/appStoreVersions?limit=20`)).data;
let wrote = 0;
let pending = 0;

for (const platform of PLATFORMS) {
  const mine = versions.filter((v) => v.attributes.platform === platform);
  let record = mine.find((v) => v.attributes.versionString === version);

  if (!record) {
    console.log(`=== ${platform}: no ${version} record ===`);
    console.log(`    newest is ${mine[0]?.attributes.versionString} (${mine[0]?.attributes.appStoreState})`);
    if (!CREATE) {
      console.log('    --create-version makes one. Nothing was changed.\n');
      pending += 1;
      continue;
    }
    // MANUAL: approval should not publish on its own. Releasing stays a
    // separate press, the same reason this script will not submit.
    const made = await post('/appStoreVersions', {
      data: {
        type: 'appStoreVersions',
        attributes: { platform, versionString: version, releaseType: 'MANUAL' },
        relationships: { app: { data: { type: 'apps', id: APP_ID } } },
      },
    });
    record = made.data;
    console.log(`    created ${version} (${record.attributes.appStoreState}), release MANUAL\n`);
    wrote += 1;
  } else {
    console.log(`=== ${platform}: ${version} exists (${record.attributes.appStoreState}) ===`);
  }

  const state = record.attributes.appStoreState;
  if (!EDITABLE.has(state)) {
    console.log(`    ${state} does not take a metadata edit; skipping.\n`);
    continue;
  }

  const locs = (await get(`/appStoreVersions/${record.id}/appStoreVersionLocalizations`)).data;

  // For the promotionalText carry-forward: the newest other version of this
  // same platform, whose localizations hold the value Apple failed to clone.
  const previous = mine.find((v) => v.id !== record.id);
  let previousLocs = [];
  if (previous) {
    previousLocs = (await get(`/appStoreVersions/${previous.id}/appStoreVersionLocalizations`)).data;
  }

  for (const locale of LOCALES) {
    const existing = locs.find((l) => l.attributes.locale === locale);
    const want = wanted[locale];
    console.log(`  --- ${locale} ---`);

    if (!existing) {
      console.log('    no localization at all');
      if (!PUSH) {
        pending += 1;
        continue;
      }
      await post('/appStoreVersionLocalizations', {
        data: {
          type: 'appStoreVersionLocalizations',
          attributes: { locale, description: want.description, whatsNew: want.whatsNew },
          relationships: {
            appStoreVersion: { data: { type: 'appStoreVersions', id: record.id } },
          },
        },
      });
      console.log('    created with description and whatsNew');
      wrote += 1;
      continue;
    }

    const attrs = existing.attributes;
    const changes = {};
    for (const field of ['description', 'whatsNew']) {
      if (tidy(attrs[field]) === tidy(want[field])) {
        console.log(`    ${field}: already current (${want[field].length} chars)`);
      } else {
        console.log(`    ${field}: ${tidy(attrs[field]).length} in the store -> ${want[field].length} here`);
        changes[field] = want[field];
      }
    }

    // The documented trap: cloned from nothing, so fill it from the platform's
    // own previous version rather than from this repository, which holds one
    // set for two platforms that deliberately differ.
    if (!tidy(attrs.promotionalText)) {
      const carried = tidy(previousLocs.find((l) => l.attributes.locale === locale)?.attributes.promotionalText);
      if (carried) {
        console.log(`    promotionalText: empty -> carrying ${carried.length} chars forward from ${previous.attributes.versionString}`);
        changes.promotionalText = carried;
      } else {
        console.log('    promotionalText: empty, and the previous version has none to carry');
      }
    }

    console.log(`    keywords: ${tidy(attrs.keywords).length} chars, left alone by design`);

    if (!Object.keys(changes).length) continue;
    if (!PUSH) {
      pending += Object.keys(changes).length;
      continue;
    }
    await patch(`/appStoreVersionLocalizations/${existing.id}`, {
      data: { type: 'appStoreVersionLocalizations', id: existing.id, attributes: changes },
    });
    console.log(`    wrote ${Object.keys(changes).join(', ')}`);
    wrote += Object.keys(changes).length;
  }
  console.log();
}

if (!CREATE && !PUSH) {
  console.log(`${pending} thing(s) would change. Nothing was.`);
  console.log('--create-version makes the missing records, --push writes the text.');
} else {
  console.log(`${wrote} change(s) written.`);
  console.log('Nothing was submitted for review: attach the build and submit in App Store Connect.');
}
