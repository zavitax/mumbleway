/**
 * Puts the release notes on a TestFlight build's "What to Test".
 *
 *     node tool/push_testflight_notes.mjs            # what it would do
 *     node tool/push_testflight_notes.mjs --push
 *
 * `docs/RELEASE_NOTES.md` has said for a while that Apple's two note fields are
 * scriptable and that neither is scripted. This is the TestFlight half.
 *
 * ## Two fields, not connected, and this is the one testers read
 *
 * The App Store's "What's New in This Version" lives on a **version**
 * localization and is what a customer reads; TestFlight's lives on a **build**
 * as a `betaBuildLocalization` and is what a tester reads. Writing one does
 * nothing for the other, which is why Apple release notes are always done
 * twice — and why `tool/push_app_store_listing.mjs` does not cover this.
 *
 * Being per *build* is the practical difference: there is nothing to create
 * first and nothing to submit. A build exists the moment it finishes
 * processing, so this can run straight after a publish, where the App Store
 * half needs a version record somebody decided to make.
 *
 * ## What it writes
 *
 * `distribution/whatsnew/`, the same files that ship to Google Play — so a
 * tester and a Play tester read the same words, and the two cannot drift
 * without somebody editing one and not the other. The 4000-character TestFlight
 * limit is far above Play's 500, so text written for Play always fits.
 *
 * By default it finds the newest unexpired build and reports what it would set.
 * `--push` writes it. Read-only otherwise, like every other tool in here.
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { client } from './asc.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const APP = process.env.APP_STORE_APP_ID ?? '6797305046';
const PUSH = process.argv.includes('--push');

/// Play's locale codes against Apple's. Only the two this app ships.
const LOCALES = {
  'en-US': 'distribution/whatsnew/whatsnew-en-US',
  ru: 'distribution/whatsnew/whatsnew-ru-RU',
};

const { get, post, patch } = client();

// `/apps/{id}/builds` refuses `sort` and returns an unordered page — the trap
// recorded in docs/RELEASE_NOTES.md, which once answered with a build three
// weeks old and hid the two uploaded that morning. This is the form that sorts.
const builds = (
  await get(`/builds?filter[app]=${APP}&sort=-uploadedDate&limit=10&include=preReleaseVersion`)
).data;

const newest = builds.find((b) => b.attributes.expired === false);
if (!newest) {
  console.error('no unexpired build to annotate');
  process.exit(1);
}
console.log(
  `build ${newest.attributes.version} (${newest.attributes.processingState}), uploaded ${newest.attributes.uploadedDate}`,
);

const existing = (await get(`/builds/${newest.id}/betaBuildLocalizations`)).data;

for (const [locale, file] of Object.entries(LOCALES)) {
  const text = readFileSync(join(ROOT, file), 'utf8').trim();
  const had = existing.find((l) => l.attributes.locale === locale);
  const current = (had?.attributes.whatsNew ?? '').trim();

  if (current === text) {
    console.log(`  ${locale}: already current (${text.length} chars)`);
    continue;
  }
  console.log(`  ${locale}: ${current.length} chars -> ${text.length}`);
  if (!PUSH) continue;

  if (had) {
    await patch(`/betaBuildLocalizations/${had.id}`, {
      data: {
        type: 'betaBuildLocalizations',
        id: had.id,
        attributes: { whatsNew: text },
      },
    });
  } else {
    // A locale with no localization yet needs creating rather than patching,
    // and a build arrives with whichever ones Apple feels like.
    await post('/betaBuildLocalizations', {
      data: {
        type: 'betaBuildLocalizations',
        attributes: { locale, whatsNew: text },
        relationships: { build: { data: { type: 'builds', id: newest.id } } },
      },
    });
  }
  console.log(`  ${locale}: written`);
}

console.log(
  PUSH
    ? '\nDone. Nothing was submitted for review — this is a build note, not a version.'
    : '\nNothing was changed. Re-run with --push.',
);
