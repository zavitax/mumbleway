# Getting recordings from riders anywhere, without running a server

> **Nothing here is built.** It is a set of options with a recommendation, so
> the choice is made deliberately rather than discovered halfway through.

The diagnostic recorder works and the share button works. What is missing is a
route from a rider in another country to the corpus, without the developer
operating anything.

## The constraint that actually decides this

Not storage. **What the app currently declares about itself.**

- `docs/privacy.md` says: *"Nothing is uploaded automatically. They leave only
  if you choose to share them, using your device's normal share sheet, to a
  destination you pick."*
- `docs/STORE_DESCRIPTION.md` tells readers the app collects nothing.
- Apple's App Privacy and Google Play's Data Safety answers say the same.

**A built-in upload to a destination the developer controls makes the developer
a collector of audio data.** Apple would need *Audio Data* declared under
Diagnostics; Play would need the equivalent. That is survivable and it is
honest — but it is a change of posture, and it costs a policy revision, two
store metadata updates, and the loss of a plain sentence that is currently
true.

So the options divide on whether they cross that line, and cheap options exist
on both sides of it.

## The move that shrinks the problem

**Split the payload.** A recording is two things and only one of them is a
voice.

| | Contains | Size, 2.5 min ride | Privacy |
|---|---|---|---|
| `.csv` decision log | numbers per 10 ms — transmitting, speaking, gate, SNR, level, harmonicity | ~180 KB, compresses hard | **No audio whatsoever** |
| `.s16` audio | the rider's microphone | ~14 MB | A person's voice, possibly with others in the channel |

Most reports are answerable from the log alone. *Did the gate close? When?
What was the SNR at that moment? Did `transmitting` drop while `speaking` was
still true?* None of that needs listening.

The log can be attached to a public issue without a second thought. The audio
cannot. **Treating them as one payload forces the hardest privacy answer onto
the commonest case.**

Concretely: a **"Share decisions only"** action beside the existing share.
Costs a rider nothing to send, crosses no line, and answers most questions.

## Options for the audio half

### A. The share sheet and a link — *recommended*

The rider shares the archive to whatever they already use — Drive, iCloud,
WeTransfer, Telegram, email — and puts the link in a GitHub issue.

- **Infrastructure:** none.
- **Privacy posture:** unchanged. The rider picks the destination, which is
  exactly what the policy already describes.
- **Works today**, everywhere, with no release.
- **Cost:** friction on the rider, and links that expire before anyone looks.

### B. GitHub issue attachment

Drag the file into an issue. Fine up to about 25 MB.

- Good for **logs**. Zero infrastructure and it lands next to the discussion.
- **Not for voice.** An issue attachment is public and permanent. A rider
  sending their own voice — with whoever else was in the channel — cannot be
  asked to accept that as the default route.

### C. Serverless intake: Cloudflare Worker plus R2

The app asks a Worker for a one-time presigned URL and `PUT`s the archive
straight to object storage. Free tier covers this comfortably and there is no
server to patch.

- **Best experience by a distance:** one tap, done.
- **Crosses the line above.** Declarations change, and the policy sentence
  quoted at the top stops being true.
- Needs rate limiting, a size cap and a retention rule, or it is an open bucket
  with extra steps.

Worth doing **only if friction turns out to be the real blocker**, and worth
doing properly when it is: the store answers are part of the work, not
paperwork afterwards.

### D. Anonymous file drops — rejected

`0x0.st`, `catbox.moe`, `file.io` and similar. Tempting, no infrastructure.

**No.** Putting somebody else's voice on a third-party host with unclear
retention, unclear jurisdiction and terms nobody read is worse than every
option above, including doing nothing.

### E. The Telegram bot that already exists

`tools/vad/telegram_intake.py` already unpacks, converts and labels an archive,
and `inbox/` takes anything too large for Telegram's 20 MB bot ceiling.

It is the right receiver and the wrong front door: it only works while a
machine at the developer's desk is switched on. Useful for people who are
already in contact; not a mechanism for strangers.

## Recommendation

1. **Split the payload** and add *Share decisions only*. Small change, no
   privacy consequence, and it makes the common case free.
2. **Ship option A** with a page on the site telling riders exactly what to do.
3. **Keep the bot** as the fast path for anyone already in touch.
4. **Hold option C** until friction is demonstrated rather than assumed.

## One addition either way

The privacy policy already asks people to *"listen to it first"*. The app
should make that literal — play a recording back, with a waveform and a
playhead, before it is shared. It is the difference between an instruction and
an affordance, and the app already has both the file list and an audio path.

Asking somebody to send a recording of their own microphone without giving them
a way to hear it is asking them to take it on trust.
