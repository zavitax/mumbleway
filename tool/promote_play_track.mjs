/**
 * Promotes a build that is already on one Google Play track to another.
 *
 *     node tool/promote_play_track.mjs <key.json> --to beta            # report only
 *     node tool/promote_play_track.mjs <key.json> --to beta --commit
 *
 * The fourth of the store tools, beside `read_play_listing.mjs`,
 * `push_play_listing.mjs`, `push_ms_store_listing.py` and
 * `push_app_store_listing.mjs`, and read-only by default like all of them.
 *
 * ## Why promote rather than publish twice
 *
 * `publish.yml` takes one `track`, so the obvious way to reach two of them is
 * to run it twice — and that is wrong in a way worth stating. Every run takes
 * its build number from `github.run_number`, so two runs are **two different
 * builds**: internal testers would be riding with one and beta testers with
 * another, which is precisely what a beta exists to prevent. It would also
 * upload a second iOS build to TestFlight and expire the first.
 *
 * Play's own model is one artefact promoted across tracks, and that is what
 * this does: it reads the version codes already on `--from` and writes them to
 * `--to` without uploading anything.
 *
 * ## Read-only until told otherwise, and that is structural
 *
 * Every change happens inside a Play *edit*, and an edit that is not committed
 * never happened. Without `--commit` the edit is deleted in a `finally`, so the
 * default path cannot alter the console even if something in the middle throws.
 * `tools/play.mjs`'s `withEdit` is what guarantees it.
 *
 * ## What `--commit` actually does to a tester
 *
 * `status: completed` on a wider track means **live to everyone on it**, at
 * once. `publish.yml` deliberately uploads alpha, beta and production as
 * `draft` for that reason — see its own comment — so this tool is the
 * deliberate act that comment expects a person to perform, and it says so when
 * asked for nothing.
 *
 * The key is the one `publish.yml` holds as a repository secret. It is not in
 * this repository and must never be: `zavitax/mumbleway` is public.
 */

import { readFileSync } from 'node:fs';
import { withEdit } from './play.mjs';

const argv = process.argv.slice(2);
const flags = new Set(argv.filter((a) => a.startsWith('--')));
const valueOf = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  return i >= 0 && argv[i + 1] && !argv[i + 1].startsWith('--') ? argv[i + 1] : fallback;
};

const positional = argv.filter((a, i) => {
  if (a.startsWith('--')) return false;
  const prev = argv[i - 1];
  return !(prev === '--to' || prev === '--from' || prev === '--package');
});

const KEY = positional[0] ?? process.env.GOOGLE_APPLICATION_CREDENTIALS;
const PKG = valueOf('package', 'com.mumbleway.mumbleway');
const FROM = valueOf('from', 'internal');
const TO = valueOf('to', null);
const COMMIT = flags.has('--commit');

if (!KEY || !TO) {
  console.error('usage: node tool/promote_play_track.mjs <key.json> --to <track> [--from internal] [--commit]');
  process.exit(2);
}
if (FROM === TO) {
  console.error(`--from and --to are both ${TO}; nothing to promote`);
  process.exit(2);
}

const key = JSON.parse(readFileSync(KEY, 'utf8'));

await withEdit(key, PKG, async (edit) => {
  const { tracks } = await edit.get('/tracks');
  const source = (tracks ?? []).find((t) => t.track === FROM);
  const target = (tracks ?? []).find((t) => t.track === TO);

  const describe = (t) =>
    (t?.releases ?? [])
      .map(
        (r) =>
          `${r.name ?? '(unnamed)'} codes=${JSON.stringify(r.versionCodes ?? [])} ${r.status}`,
      )
      .join('; ') || '(nothing)';

  console.log(`${FROM}: ${describe(source)}`);
  console.log(`${TO}:   ${describe(target)}`);

  // The newest release that actually carries an artefact. A track can hold a
  // release with no version codes — a notes-only draft — and promoting that
  // would move nothing while reporting success.
  const release = (source?.releases ?? []).find((r) => (r.versionCodes ?? []).length > 0);
  if (!release) {
    console.log(`\nNothing on ${FROM} carries a build. Nothing to promote.`);
    return;
  }

  const already = (target?.releases ?? []).some((r) =>
    (r.versionCodes ?? []).some((c) => (release.versionCodes ?? []).includes(c)),
  );
  if (already) {
    console.log(`\n${TO} already has ${JSON.stringify(release.versionCodes)}. Nothing to do.`);
    return;
  }

  // The notes travel with it. A promoted release that loses them shows testers
  // the previous version's text, which is the exact fault the last release
  // shipped with on the Microsoft Store.
  const promoted = {
    name: release.name,
    versionCodes: release.versionCodes,
    status: 'completed',
    ...(release.releaseNotes ? { releaseNotes: release.releaseNotes } : {}),
  };

  console.log(
    `\nwould write to ${TO}: ${promoted.name} codes=${JSON.stringify(promoted.versionCodes)} completed` +
      `${promoted.releaseNotes ? `, with notes in ${promoted.releaseNotes.length} language(s)` : ', WITHOUT notes'}`,
  );

  if (!COMMIT) {
    console.log(
      `\nNo --commit: the edit is discarded and ${TO} is untouched.\n` +
        `Committing makes this live to everyone on ${TO} at once, which is why ` +
        `publish.yml uploads wider tracks as drafts and leaves this to a person.`,
    );
    return;
  }

  await edit.patch(`/tracks/${TO}`, { track: TO, releases: [promoted] });
  await edit.commit();
  console.log(`\nCommitted. ${JSON.stringify(promoted.versionCodes)} is live on ${TO}.`);
});
