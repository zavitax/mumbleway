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
import subprocess
import sys
import time
import urllib.parse
import urllib.request
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
    """To the format everything here reads: mono, 48 kHz, 32-bit float."""
    subprocess.run(
        ["ffmpeg", "-y", "-loglevel", "error", "-i", src,
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
            "not give a bot anything larger, so split long rides.")
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
            f"fetch at 20 MB. Split it and send the pieces.")
        return

    try:
        info = api(token, "getFile", file_id=payload["file_id"])
        path = info["result"]["file_path"]
        url = f"{API}/file/bot{token}/{path}"

        stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
        root = roots[mode]
        os.makedirs(root, exist_ok=True)
        original = os.path.join(root, f"{mode}_{stamp}{os.path.splitext(path)[1] or '.bin'}")
        with urllib.request.urlopen(url, timeout=300) as r, open(original, "wb") as f:
            f.write(r.read())

        raw = os.path.join(root, f"{mode}_{stamp}.raw")
        seconds = convert(original, raw)
        say(token, chat,
            f"Got {seconds / 60:.1f} min of {mode} ({kind}).\n"
            f"Saved as {os.path.basename(raw)}.")
        print(f"{mode}: {seconds:.1f}s -> {raw}")
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
