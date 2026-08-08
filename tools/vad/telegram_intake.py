"""Take road recordings straight off the phone, over Telegram.

Getting audio off a bike has been the slow step: ride, stop, find a cable,
copy, convert. This closes that loop — record on the phone, send it to the bot
at the next set of lights, and it lands in the corpus already converted to the
format the training pipeline reads.

    setx MUMBLEWAY_TG_TOKEN "..."          (or export, on a Mac)
    setx MUMBLEWAY_TG_CHATS "123456789"
    python telegram_intake.py C:/ml_data

Send a file with the caption `noise` or `speech`, or send /noise or /speech
first to set the mode for everything that follows. The bot replies with what it
did, so a mistake is visible at the roadside rather than three weeks later when
the numbers look strange.

The app's own share button sends a single `mumbleway-recordings.zip` holding
every recording on the phone, and **that needs no mode and is never asked about
one**. A ride is not noise or speech; it is both in turn, and the `.csv` beside
each recording already says which for every 10 ms of it. So an archive is
unpacked into `rides/`, converted, and cut into `speech_road/` and
`noise_road/` by its own decision logs.

Those labels are the noise gate's opinion, not ground truth, and the difference
matters: a model trained on them can only learn to imitate the gate, mistakes
included, and the gate's mistakes are the reason for collecting road audio in
the first place. Treat the split as triage — a way to find the passages worth
listening to — and relabel from the whole rides, which are kept for exactly
that.

An archive also makes a whole ride fit. Telegram will not hand a bot more than
20 MB, and PCM of a mostly silent ride compresses around ten times over.
Sending the same archive twice is harmless: files are named after the ride they
came from, so a resend overwrites rather than accumulates.

# The token

Read from the environment and never from a file in this repository, which is
public. There is no default, no fallback, and no argument to pass it on the
command line — a token in a shell history is a token in a backup. If it leaks,
revoke it with @BotFather's /revoke; it grants control of the bot and nothing
else, but that is enough to let a stranger fill a disk.

# Who may send

`MUMBLEWAY_TG_CHATS` is a comma-separated allow-list of chat ids, and it is not
optional. A bot's username is guessable and anyone who finds it can upload;
without the list the first thing this would accept is whatever a stranger sent.
Message the bot once and it will tell you your own id.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import zipfile
from datetime import datetime, timezone

API = "https://api.telegram.org"

# Telegram will not hand a bot a file larger than this, whatever the app shows
# as sent. Long rides have to be split; see the note in the reply text.
MAX_BYTES = 20 * 1024 * 1024

# What everything here reads, and what a segment is cut out of.
RATE = 48_000

# Kept either side of a run the chain called speech, and taken off either side
# of one it did not. Without it every speech clip starts mid-vowel: the column
# it comes from is the instantaneous decision, which drops between words.
PAD_SECONDS = 0.10

# Shorter than this is not a training example, it is a click.
MIN_SEGMENT_SECONDS = 0.50

# How long an inbox file must sit still before it is treated as finished. A
# copy in progress is not an archive, and opening one looks like corruption.
SETTLE_SECONDS = 5


def api(token, method, **params):
    url = f"{API}/bot{token}/{method}"
    if params:
        url += "?" + urllib.parse.urlencode(params)
    with urllib.request.urlopen(url, timeout=60) as r:
        return json.loads(r.read())


def say(token, chat, text):
    try:
        api(token, "sendMessage", chat_id=chat, text=text)
    except Exception as e:  # noqa: BLE001 - a failed reply must not stop intake
        print(f"could not reply: {e}", file=sys.stderr)


def convert(src, dst):
    """To the format everything here reads: mono, 48 kHz, 32-bit float.

    `.s16` files come from the app's own diagnostic recorder and are headerless
    by design, so ffmpeg has to be told what they are. Guessing is not an option
    it has: given no header it will refuse the file, and given the wrong flags
    it will cheerfully produce noise at the wrong rate.
    """
    src_flags = []
    if src.lower().endswith(".s16"):
        src_flags = ["-f", "s16le", "-ar", "48000", "-ac", "1"]
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", *src_flags, "-i", src,
         "-ac", "1", "-ar", "48000", "-f", "f32le", dst],
        check=True,
    )
    return os.path.getsize(dst) / 4 / 48_000


def to_wav(raw, dst):
    """A copy of a `.raw` that a media player will open.

    The corpus format is headerless 32-bit float, because that is what every
    tool here reads — they all glob `*.raw` — and headerless is the point: no
    container, no metadata, no decoding. It is also why nothing on a desktop
    will play one, and judging a label means listening to it. Without this the
    only way to hear a segment was to import it by hand with the sample rate
    and format typed in from memory.

    Written beside the `.raw` rather than instead of it. The training tools
    are not touched, and a `.wav` in the same directory is invisible to a glob
    for `*.raw`.

    16-bit rather than float, because a float WAV is a thing some players still
    decline, and this copy exists to be opened without thinking about it.
    """
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-f", "f32le", "-ar", str(RATE),
         "-ac", "1", "-i", raw, "-c:a", "pcm_s16le", dst],
        check=True,
    )


def pick_file(msg):
    """Whichever way the phone chose to send it."""
    for key in ("voice", "audio", "document", "video_note", "video"):
        if key in msg:
            return msg[key], key
    return None, None


def app_stem(name):
    """The recorder's own name for a file, if this came from the app.

    Its files are named `YYYYMMDD-HHMM-NNN`, and the audio and the decision log
    share that stem. Keeping it is what keeps the pair together: they arrive as
    two separate messages, minutes apart, so a name built from the arrival time
    would separate them permanently and the log would be attached to nothing.
    """
    m = re.match(r"^(\d{8}-\d{4}-\d{3})\.(s16|csv)$", os.path.basename(name or ""))
    return m.group(1) if m else None


def absorb(root, sent_as, src, mode):
    """File one recording under the recorder's own name, converting audio.

    Returns `("audio", seconds, name)` or `("log", blocks, name)`. The caller
    decides what to say about it, because one file arriving on its own and
    twenty arriving in an archive want very different replies.

    A file already there is overwritten rather than renamed around. The app
    shares *everything* on the phone each time, so the same ride arrives again
    on every send, and the alternative to overwriting is a corpus that grows a
    duplicate copy per visit to the bot.
    """
    stem = app_stem(sent_as)
    name = stem or f"{mode}_{datetime.now(timezone.utc):%Y%m%d-%H%M%S}"
    kept = os.path.join(root, f"{name}{os.path.splitext(sent_as)[1] or '.bin'}")
    if os.path.abspath(src) != os.path.abspath(kept):
        os.replace(src, kept)

    # A decision log is not audio and must not be handed to ffmpeg. It is the
    # more valuable half of the pair — it is the only record of what the chain
    # concluded, and nothing can reconstruct it from the audio.
    if kept.lower().endswith(".csv"):
        with open(kept, encoding="utf-8", errors="replace") as f:
            rows = sum(1 for line in f if line and not line.startswith("#"))
        return "log", max(rows - 1, 0), name

    raw = os.path.join(root, f"{name}.raw")
    seconds = convert(kept, raw)
    to_wav(raw, os.path.join(root, f"{name}.wav"))
    return "audio", seconds, name


def decisions(path):
    """The `speaking` column of a decision log, one entry per block, in order.

    That column is the chain's instantaneous answer to "is this a voice",
    before the hold and fade the transmitter puts around it. Which is what
    makes it usable as a label and also what makes the padding below
    necessary — it goes false between words, not between sentences.
    """
    says, header = [], None
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split(",")
            if header is None:
                header = parts
                continue
            row = dict(zip(header, parts))
            says.append(row.get("speaking") == "1")
    return says


def runs(says):
    """Consecutive blocks that agree, as `(speaking, first, last_exclusive)`."""
    start = 0
    for i in range(1, len(says) + 1):
        if i == len(says) or says[i] != says[start]:
            yield says[start], start, i
            start = i


def cut(raw, first, last, dst):
    """One stretch of a ride, copied out by sample offset.

    The whole ride is already f32 mono at 48 kHz, so a segment is a byte range
    and nothing has to be decoded again to take one.
    """
    with open(raw, "rb") as src, open(dst, "wb") as out:
        src.seek(first * 4)
        remaining = (last - first) * 4
        while remaining > 0:
            chunk = src.read(min(1 << 20, remaining))
            if not chunk:
                break
            out.write(chunk)
            remaining -= len(chunk)


def split_by_log(raw, log, speech_root, noise_root, stem):
    """Cut a ride into speech and noise using what the chain decided.

    **These are weak labels, not ground truth, and that distinction is
    load-bearing.** `speaking` is the gate's own opinion, so a model trained on
    it can at best learn to imitate the gate — including its mistakes, which
    are the entire reason for collecting road audio in the first place. What
    the split is good for is triage: finding the passages worth listening to,
    and having something usable without an afternoon of hand-labelling. The
    whole ride stays beside these so any of it can be relabelled.

    Speech runs are padded and noise runs are trimmed by the same margin. The
    column goes false between words rather than between sentences, so an
    unpadded speech cut loses the consonant that starts it and an untrimmed
    noise cut collects the one that ends it.

    Returns `(speech_count, speech_seconds, noise_count, noise_seconds)`.
    """
    says = decisions(log)
    total = os.path.getsize(raw) // 4
    if not says or not total:
        return 0, 0.0, 0, 0.0

    # Derived rather than assumed. It is 480 samples — 10 ms — but a recorder
    # that changes block size would otherwise silently mislabel every ride.
    block = total // len(says)
    if not 160 <= block <= 1920:
        raise ValueError(f"{block} samples per block does not look right")

    pad = int(PAD_SECONDS * RATE)
    least = int(MIN_SEGMENT_SECONDS * RATE)
    counts = {True: 0, False: 0}
    seconds = {True: 0.0, False: 0.0}

    for speaking, first, last in runs(says):
        lo, hi = first * block, min(last * block, total)
        lo, hi = (max(0, lo - pad), min(total, hi + pad)) if speaking else (lo + pad, hi - pad)
        if hi - lo < least:
            continue
        counts[speaking] += 1
        seconds[speaking] += (hi - lo) / RATE
        root = speech_root if speaking else noise_root
        os.makedirs(root, exist_ok=True)
        tag = "s" if speaking else "n"
        segment = os.path.join(root, f"{stem}-{tag}{counts[speaking]:04d}.raw")
        cut(raw, lo, hi, segment)
        # The listenable copy. These are the files whose labels have to be
        # judged by ear, so they are the ones that most need to be playable.
        to_wav(segment, segment[:-4] + ".wav")

    return counts[True], seconds[True], counts[False], seconds[False]


def unpack(archive, roots):
    """Take a share-button archive apart and label what is in it.

    Returns `(rides, minutes, speech, speech_seconds, noise, noise_seconds,
    problems)`.

    **No mode is asked for and none would mean anything.** A ride is not noise
    or speech, it is both in turn, and the `.csv` beside the audio already says
    which for every 10 ms of it. Asking the rider to pick one for a whole
    archive threw that away and then made them guess at a label the phone had
    already worked out.

    **Members are taken by name only.** `../` in a member name is the oldest
    way there is to write outside the directory you meant, and while this bot
    has an allow-list, an allow-list is one environment variable away from
    being wrong — and this is the one place where a name chosen elsewhere
    decides where a file lands.
    """
    rides = roots["rides"]
    os.makedirs(rides, exist_ok=True)
    problems, members = [], []

    with zipfile.ZipFile(archive) as z:
        for entry in sorted(z.infolist(), key=lambda e: e.filename):
            if entry.is_dir():
                continue
            member = os.path.basename(entry.filename.replace("\\", "/"))
            if not member or member.startswith("."):
                continue
            if not member.lower().endswith((".s16", ".csv")):
                problems.append(f"{member}: not a recording, left out")
                continue
            try:
                with z.open(entry) as src, open(os.path.join(rides, member), "wb") as f:
                    shutil.copyfileobj(src, f)
                members.append(member)
            except Exception as e:  # noqa: BLE001 - one bad member is not the archive
                problems.append(f"{member}: {e}")

    ridden = minutes = 0
    speech = noise = 0
    speech_s = noise_s = 0.0
    for stem in sorted({os.path.splitext(m)[0] for m in members}):
        audio = os.path.join(rides, f"{stem}.s16")
        log = os.path.join(rides, f"{stem}.csv")
        if not os.path.exists(audio):
            problems.append(f"{stem}: a decision log with no audio beside it")
            continue
        try:
            raw = os.path.join(rides, f"{stem}.raw")
            seconds = convert(audio, raw)
            # The whole ride, playable, for listening straight through — which
            # is how the passages worth cutting out get noticed in the first
            # place.
            to_wav(raw, os.path.join(rides, f"{stem}.wav"))
            ridden += 1
            minutes += seconds / 60
            if os.path.exists(log):
                a, b, c, d = split_by_log(raw, log, roots["speech"], roots["noise"], stem)
                speech, speech_s, noise, noise_s = speech + a, speech_s + b, noise + c, noise_s + d
            else:
                problems.append(f"{stem}: no decision log, so nothing to label it by")
        except Exception as e:  # noqa: BLE001
            problems.append(f"{stem}: {e}")

    return ridden, minutes, speech, speech_s, noise, noise_s, problems


def drain_inbox(roots):
    """Unpack any archive dropped into `inbox/` by hand.

    Telegram will not give a bot a file over 20 MB, and no amount of care on
    the app's side removes that ceiling — a long enough ride simply cannot
    arrive that way. This is the way round it: copy the `.zip` off the phone
    over a cable, AirDrop or a drive, drop it here, and it is unpacked and
    labelled exactly as if it had been sent, with no size limit at all.

    A file still being copied in is not an archive yet. Anything touched in the
    last few seconds is left for the next pass rather than opened half-written,
    which reads as a corrupt archive and would otherwise be reported as one.
    """
    inbox = roots["inbox"]
    for name in sorted(os.listdir(inbox)):
        if not name.lower().endswith(".zip"):
            continue
        path = os.path.join(inbox, name)
        if time.time() - os.path.getmtime(path) < SETTLE_SECONDS:
            continue
        try:
            rides, minutes, speech, speech_s, noise, noise_s, problems = unpack(
                path, roots)
            os.remove(path)
            print(f"inbox: {name} -> {rides} rides, {minutes:.1f} min, "
                  f"{speech} speech ({speech_s / 60:.1f} min), "
                  f"{noise} noise ({noise_s / 60:.1f} min)")
            for problem in problems:
                print(f"  left out: {problem}")
        except Exception as e:  # noqa: BLE001 - one bad archive is not the inbox
            # Left where it is, and renamed so the next pass does not retry it
            # for ever. The name says what happened without needing the log.
            print(f"inbox: {name} failed: {e}", file=sys.stderr)
            try:
                os.replace(path, path + ".failed")
            except OSError:
                pass


def handle(token, roots, modes, msg):
    chat = msg["chat"]["id"]
    text = (msg.get("text") or "").strip().lower()

    if text in ("/noise", "/speech"):
        modes[chat] = text[1:]
        say(token, chat, f"Mode set: {modes[chat]}. Send audio and it goes there.")
        return
    if text in ("/start", "/help", "/id"):
        say(token, chat,
            f"Your chat id is {chat}.\n\n"
            "Send the mumbleway-recordings.zip from the app's diagnostics "
            "panel and it is unpacked, converted and split into speech and "
            "noise on its own — the .csv beside each recording already says "
            "which every 10 ms of it was, so there is nothing to label by "
            "hand and nothing to tell me. Sending the same archive again "
            "later is harmless.\n\n"
            "Anything else — a voice note, an audio file — has no such log, "
            "so caption it 'noise' or 'speech', or set a mode with /noise or "
            "/speech first.\n\n"
            "Files must be under 20 MB; Telegram will not give a bot "
            "anything larger.")
        return

    payload, kind = pick_file(msg)
    if payload is None:
        return

    # The name is known before the file is fetched, and it decides whether
    # there is anything to ask. An archive carries its own labels, so asking
    # for one would be asking the rider to overrule the phone.
    sent_as = payload.get("file_name") or ""
    archive = sent_as.lower().endswith(".zip")

    caption = (msg.get("caption") or "").strip().lower()
    mode = "noise" if "noise" in caption else "speech" if "speech" in caption else modes.get(chat)
    if mode is None and not archive:
        say(token, chat, "Which is it? Caption the file 'noise' or 'speech', "
                         "or send /noise or /speech first. An archive from the "
                         "app needs neither — its decision logs say which.")
        return

    size = payload.get("file_size") or 0
    if size > MAX_BYTES:
        say(token, chat,
            f"That is {size / 1e6:.0f} MB and Telegram caps what a bot may "
            f"fetch at 20 MB. If it is the app's archive, delete the "
            f"recordings in the panel after this send — it packs everything "
            f"still on the phone every time, so it only grows.")
        return

    try:
        info = api(token, "getFile", file_id=payload["file_id"])
        path = info["result"]["file_path"]
        url = f"{API}/file/bot{token}/{path}"

        # The name the phone sent, which for the app's own recordings carries
        # the pairing. `file_path` is Telegram's storage path and does not.
        sent_as = sent_as or os.path.basename(path)
        stem = app_stem(sent_as)

        # What the app's share button produces, and so the ordinary case.
        if archive:
            os.makedirs(roots["rides"], exist_ok=True)
            original = os.path.join(roots["rides"], ".incoming.zip")
            with urllib.request.urlopen(url, timeout=300) as r, open(original, "wb") as f:
                f.write(r.read())

            rides, minutes, speech, speech_s, noise, noise_s, problems = unpack(
                original, roots)
            # The archive is a second copy of what now sits beside it, and every
            # send brings the whole phone again. Keeping them would fill the
            # disk with the same ride over and over.
            os.remove(original)

            if not rides:
                say(token, chat,
                    "That archive held no recordings I could read."
                    + ("\n\n" + "\n".join(problems[:5]) if problems else ""))
                return
            say(token, chat,
                f"{rides} ride{'' if rides == 1 else 's'}, {minutes:.1f} min.\n"
                f"Split by the decision logs: {speech} speech "
                f"({speech_s / 60:.1f} min), {noise} noise "
                f"({noise_s / 60:.1f} min).\n\n"
                "Those labels are the gate's own opinion rather than ground "
                "truth — the whole rides are kept if any of it needs "
                "relabelling."
                + ("\n\nLeft out:\n" + "\n".join(problems[:5]) if problems else ""))
            print(f"archive: {rides} rides, {minutes:.1f} min, "
                  f"{speech} speech, {noise} noise")
            return

        name = stem or f"{mode}_{datetime.now(timezone.utc):%Y%m%d-%H%M%S}"
        root = roots[mode]
        os.makedirs(root, exist_ok=True)

        original = os.path.join(root, f"{name}{os.path.splitext(sent_as)[1] or '.bin'}")
        with urllib.request.urlopen(url, timeout=300) as r, open(original, "wb") as f:
            f.write(r.read())

        kept, value, saved = absorb(root, sent_as, original, mode)
        if kept == "log":
            say(token, chat,
                f"Kept the decision log for {saved} ({value} blocks). "
                f"Send the .s16 beside it.")
            print(f"{mode}: log -> {saved}.csv")
            return

        paired = stem and os.path.exists(os.path.join(root, f"{stem}.csv"))
        say(token, chat,
            f"Got {value / 60:.1f} min of {mode} ({kind}).\n"
            f"Saved as {saved}.raw."
            + ("\nPaired with its decision log." if paired
               else "\nSend the .csv beside it if there is one." if stem else ""))
        print(f"{mode}: {value:.1f}s -> {saved}.raw")
    except Exception as e:  # noqa: BLE001
        say(token, chat, f"That did not work: {e}")
        print(f"failed: {e}", file=sys.stderr)


def main():
    token = os.environ.get("MUMBLEWAY_TG_TOKEN")
    if not token:
        sys.exit("set MUMBLEWAY_TG_TOKEN (from @BotFather). Never commit it.")

    allowed = {c.strip() for c in os.environ.get("MUMBLEWAY_TG_CHATS", "").split(",") if c.strip()}
    if not allowed:
        print("MUMBLEWAY_TG_CHATS is empty: every message will be REFUSED and "
              "the sender told their chat id. Set it and restart.", file=sys.stderr)

    base = sys.argv[1] if len(sys.argv) > 1 else "."
    roots = {
        "noise": os.path.join(base, "noise_road"),
        "speech": os.path.join(base, "speech_road"),
        # Rides land here whole — audio, decision log and the converted copy —
        # and the segments cut from them go to the two above. Keeping the whole
        # thing is what makes the split reversible: the labels are the gate's
        # own, so anything it got wrong is still here to be relabelled.
        "rides": os.path.join(base, "rides"),
        # Drop a `.zip` here and it is taken in on the next pass, with none of
        # Telegram's 20 MB ceiling. This is the route for a ride too long to
        # send, and it needs no bot at all beyond one that is running.
        "inbox": os.path.join(base, "inbox"),
    }
    modes = {}

    # Made now rather than when the first file lands. Empty directories are how
    # somebody checks the bot is pointed where they think it is, and creating
    # them lazily means the answer to "is this working" is unavailable until
    # after a 20 MB upload — at which point a wrong base path or a directory
    # that cannot be written to is discovered the expensive way.
    for root in roots.values():
        os.makedirs(root, exist_ok=True)

    me = api(token, "getMe")["result"]
    print(f"listening as @{me.get('username')}")
    for label in ("inbox", "rides", "speech", "noise"):
        print(f"  {label:6} -> {roots[label]}")
    print("drop a .zip in inbox to take one in without Telegram's 20 MB limit")

    offset = None
    while True:
        # Before the poll, which blocks for the best part of a minute. After it
        # would mean a file dropped in just as a long poll started sits there
        # until the poll gives up.
        try:
            drain_inbox(roots)
        except Exception as e:  # noqa: BLE001 - the inbox must not stop the bot
            print(f"inbox scan failed: {e}", file=sys.stderr)

        try:
            got = api(token, "getUpdates", timeout=50, **({"offset": offset} if offset else {}))
        except Exception as e:  # noqa: BLE001 - a dropped poll is not fatal
            print(f"poll failed, retrying: {e}", file=sys.stderr)
            time.sleep(5)
            continue

        for update in got.get("result", []):
            offset = update["update_id"] + 1
            msg = update.get("message") or update.get("channel_post")
            if not msg:
                continue
            chat = str(msg["chat"]["id"])
            if chat not in allowed:
                say(token, msg["chat"]["id"],
                    f"Not an allowed sender. Your chat id is {chat} — add it to "
                    "MUMBLEWAY_TG_CHATS and restart the bot.")
                print(f"refused chat {chat}", file=sys.stderr)
                continue
            handle(token, roots, modes, msg)


if __name__ == "__main__":
    main()
