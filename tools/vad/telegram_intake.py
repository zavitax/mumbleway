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
every recording on the phone, and that archive is taken apart here. It is the
normal way recordings arrive, and it is what makes a whole ride fit: Telegram
will not hand a bot more than 20 MB, and PCM of a mostly silent ride compresses
around ten times over. Sending the same archive twice is harmless — files are
named after the ride they came from, so a resend overwrites rather than
accumulates.

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
    return "audio", convert(kept, raw), name


def unpack(archive, root, mode):
    """Take a share-button archive apart, one member at a time.

    Returns `(minutes, recordings, logs, problems)`.

    **Members are taken by name only.** `../` in a member name is the oldest
    way there is to write outside the directory you meant, and while this bot
    has an allow-list, an allow-list is one environment variable away from
    being wrong — and this is the one place where a name chosen elsewhere
    decides where a file lands.

    Sorted, so a `.csv` and the `.s16` it describes are handled together and a
    failure names the ride it belongs to.
    """
    minutes, recordings, logs, problems = 0.0, 0, 0, []
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

            temp = os.path.join(root, f".unpacking-{member}")
            try:
                with z.open(entry) as src, open(temp, "wb") as f:
                    shutil.copyfileobj(src, f)
                kind, value, _ = absorb(root, member, temp, mode)
                if kind == "audio":
                    minutes += value / 60
                    recordings += 1
                else:
                    logs += 1
            except Exception as e:  # noqa: BLE001 - one bad member is not the archive
                problems.append(f"{member}: {e}")
            finally:
                if os.path.exists(temp):
                    os.remove(temp)
    return minutes, recordings, logs, problems


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
            "Send audio with caption 'noise' or 'speech', or use /noise and "
            "/speech to set a mode. Files must be under 20 MB — Telegram will "
            "not give a bot anything larger.\n\n"
            "The easiest way is the app itself: diagnostics panel, share, and "
            "send the mumbleway-recordings.zip it makes. It holds every "
            "recording on the phone with the decisions the noise gate made "
            "about each, it is about a tenth the size of the raw audio, and "
            "sending it again later is harmless.")
        return

    payload, kind = pick_file(msg)
    if payload is None:
        return

    caption = (msg.get("caption") or "").strip().lower()
    mode = "noise" if "noise" in caption else "speech" if "speech" in caption else modes.get(chat)
    if mode is None:
        say(token, chat, "Which is it? Caption the file 'noise' or 'speech', "
                         "or send /noise or /speech first.")
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
        sent_as = payload.get("file_name") or os.path.basename(path)
        stem = app_stem(sent_as)
        name = stem or f"{mode}_{datetime.now(timezone.utc):%Y%m%d-%H%M%S}"
        root = roots[mode]
        os.makedirs(root, exist_ok=True)

        original = os.path.join(root, f"{name}{os.path.splitext(sent_as)[1] or '.bin'}")
        with urllib.request.urlopen(url, timeout=300) as r, open(original, "wb") as f:
            f.write(r.read())

        # What the app's share button produces, and so the ordinary case.
        if sent_as.lower().endswith(".zip"):
            minutes, recordings, logs, problems = unpack(original, root, mode)
            # The archive itself is a copy of things now unpacked beside it, and
            # every send brings the whole phone again. Keeping them would fill
            # the disk with the same ride over and over.
            os.remove(original)
            if not recordings and not logs:
                say(token, chat, "That archive held nothing I recognise. The "
                                 "app's own share button is the one to use.")
                return
            say(token, chat,
                f"Unpacked into {mode}: {recordings} recording"
                f"{'' if recordings == 1 else 's'} ({minutes:.1f} min) and "
                f"{logs} decision log{'' if logs == 1 else 's'}."
                + ("\n\nLeft out:\n" + "\n".join(problems[:5]) if problems else ""))
            print(f"{mode}: archive -> {recordings} audio, {logs} logs, "
                  f"{minutes:.1f} min")
            return

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
    }
    modes = {}

    me = api(token, "getMe")["result"]
    print(f"listening as @{me.get('username')}; noise -> {roots['noise']}, "
          f"speech -> {roots['speech']}")

    offset = None
    while True:
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
