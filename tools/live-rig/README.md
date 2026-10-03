# The live rig

A Mumble server to test the moderation features against, in one command.

```bash
python tools/live-rig/rig.py up      # a server, shaped for the tests
python tools/live-rig/rig.py admin   # kick, ban, lift, move, register, ACLs
python tools/live-rig/rig.py state   # what the server holds right now
python tools/live-rig/rig.py down    # take it away
```

It needs Docker and `cargo`. Everything else it builds itself.

## Why this exists

Every moderation feature in this client is a message whose other answer is a
**refusal**: kick, ban, lift a ban, move somebody, register and unregister a
user, hand out priority speaker, reset a rider's picture and comment, read and
write a channel's access list. A unit test can prove the message is well formed
and can prove nothing about what a server does with it — and the interesting
half is precisely what a server does when the rider may *not* do the thing.

That needs a server which grants those rights, and a server grants them only to
a rider it recognises. Doing it by hand is six SQL statements and a restart, and
getting one of them subtly wrong looks exactly like the client being broken: the
ACL round trip below was first reported as "the server ignores our writes", and
it was a group name missing a `#`.

## What `up` builds

```
Root ───── Garage          an ordinary channel, to move riders in and out of
      └─── Clubhouse       shut to @all, open to the group #vip
```

On **Root**, the group `#rideboss` is granted **Write**. In Murmur, Write on a
channel implies every other permission on it and below, so a rider presenting
the token `rideboss` is an administrator — with no account to register and no
password to keep. The token `vip` opens Clubhouse the same way.

SuperUser's password is generated per rig and written to `.rig.env`, which is
gitignored, along with the addresses and channel names the tests read from the
environment. **This repository is public**; nothing from that file belongs in a
commit, an issue or a chat message.

`up` reuses a server that is already there — including one started by hand — and
recovers its SuperUser password from the container rather than asking you to
throw a working server away. `up --fresh` starts over.

## What the moderation tests prove

| Test | What it would catch |
|---|---|
| `an_admin_moves_kicks_and_bans_somebody` | a move that silently does nothing; a kick that disconnects nobody; a ban that is not in the list afterwards, or that cannot be lifted |
| `an_admin_can_run_the_server` | registering and unregistering a user, priority speaker, resetting somebody's picture and comment — each of which a server refuses from the wrong rider |
| `an_acl_survives_a_round_trip` | an access list written and read back as something else: a lost group, an inverted deny, a bit in the wrong column |
| `an_admin_can_read_the_ban_list_and_is_told_nothing_about_permissions` | reading the ban list without the rights to change it, which must not be reported as rights the rider does not have |

`rig.py test` runs the whole live suite instead — sixteen tests covering the
handshake, bandwidth, quality figures, suppression, tokens, listening,
reconnection and the rest.

## Things this server taught us the hard way

- **A token group is written `#vip`, and the rider types `vip`.** Written as
  `vip` the rule matches a real group of that name, the token matches nothing,
  and the channel stays shut.
- **Murmur rate-limits ACL writes.** Two writes in quick succession and the
  second comes back `RATELIMIT`; this client re-reads on its health tick rather
  than immediately after writing, for that reason.
- **SuperUser cannot be the target.** Murmur refuses a permission query and any
  flag change aimed at user id 0, so a test that gives *itself* priority speaker
  proves nothing and fails. Act on a second rider.
- **Autoban counts every attempt**, successful or not: ten in two minutes earns
  a five-minute ban that shows up as "Global ban" in the log with an empty ban
  table. The rig sets `autobanAttempts` high so a test that reconnects on
  purpose does not ban itself.
- **Editing the database under a running server changes the file and nothing
  else.** Murmur reads the access lists at boot, so `up` restarts it after
  shaping. Changes made *through the client* take effect at once.

## The environment it writes

| Variable | What it is for |
|---|---|
| `MW_LIVE` | `host:port` of the server; without it every live test skips |
| `MW_LIVE_SUPERUSER` | SuperUser's password, so the admin tests can be an admin |
| `MW_LIVE_BANDWIDTH` | what the server was configured with, so the test checks the figure that arrives is *that* one rather than merely a number |
| `MW_LIVE_CHANNEL` | a channel to move in and out of |
| `MW_LIVE_TOKEN_CHANNEL`, `MW_LIVE_TOKEN` | the shut channel and the token that opens it |
| `MW_LIVE_EXPECT_SUGGESTIONS` | the server suggests push-to-talk and positional audio, so the client must say so |
