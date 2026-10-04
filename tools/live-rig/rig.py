#!/usr/bin/env python3
"""A Mumble server to test the moderation features against, in one command.

    python tools/live-rig/rig.py up        # a server, shaped for the tests
    python tools/live-rig/rig.py admin     # run the admin tests against it
    python tools/live-rig/rig.py proxies   # add proxies in front of it
    python tools/live-rig/rig.py proxy     # run the proxy tests
    python tools/live-rig/rig.py state     # what the server holds right now
    python tools/live-rig/rig.py down      # take it away

**Why a rig rather than a paragraph in a test file.** Every moderation feature
this client has — kick, ban, lift, move, register, unregister, priority speaker,
reset somebody's picture and comment, read and write an access list — is a
message whose reply is a *refusal* when the rider lacks the right, and refusals
are the half that unit tests cannot reach. So they need a server that grants
those rights, and a server grants them only to a rider the server knows. Setting
that up by hand is six sqlite statements and a restart, and getting one of them
subtly wrong looks exactly like the client being broken.

What `up` builds:

  Root ───── Garage          an ordinary channel, to move riders in and out of
        └─── Clubhouse       shut to @all, open to the group `#vip`

  on Root:  `#rideboss` is granted Write, which in Murmur implies every other
            permission on that channel and below — so a rider holding the token
            `rideboss` is an administrator, with no account to register.

  SuperUser's password is generated per rig and written to `.rig.env`, which is
  gitignored. This repository is public; nothing here goes into it.

What `proxies` adds, for the proxy tests:

  mw-proxy-http    tinyproxy, HTTP CONNECT, BasicAuth, CONNECT allowed to the
                   Mumble port and to the SOCKS proxy (so a chain can be built)
  mw-proxy-socks   SOCKS5 with username and password
  mw-proxy-strict  tinyproxy that allows CONNECT to 443 and nothing else

**The target address is not `127.0.0.1`.** Inside a container that is the
container, so the proxies are asked for `host.docker.internal`, which Docker
Desktop points back at this machine, where the server's port is published. The
tests get both addresses: the one they dial and the one the proxy is asked for.

The tests themselves live in `core/tests/live_server.rs` and are `#[ignore]`d,
so they never run on CI. This script passes them what they need through the
environment.
"""

import argparse
import json
import os
import re
import secrets
import socket
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
ENV_FILE = HERE / ".rig.env"

IMAGE = "mumblevoip/mumble-server:latest"
DEFAULT_NAME = "mw-murmur"
DEFAULT_PORT = 64739
BANDWIDTH = 32000

#: The proxies, and the ports they are published on.
HTTP_IMAGE = "vimagick/tinyproxy:latest"
SOCKS_IMAGE = "serjs/go-socks5-proxy:latest"
HTTP_NAME = "mw-proxy-http"
SOCKS_NAME = "mw-proxy-socks"
STRICT_NAME = "mw-proxy-strict"
HTTP_PORT = 18080
SOCKS_PORT = 11080
STRICT_PORT = 18081
PROXY_USER = "rider"
PROXY_PASSWORD = "throughhere"

#: How a proxy container reaches this machine. Docker Desktop provides the name;
#: `127.0.0.1` inside a container is the container itself, which would make the
#: proxy appear to work and the connection appear to go nowhere.
HOST_FROM_CONTAINER = "host.docker.internal"

#: The tests that go through a proxy, in the order they are worth reading.
PROXY_TESTS = [
    "a_session_through_an_http_proxy_holds_past_a_minute",
    "a_session_through_a_socks5_proxy_holds_past_a_minute",
    "voice_tunnelled_through_a_proxy_never_promotes_to_udp",
    "a_chain_of_two_proxies_reaches_the_server",
    "a_proxy_that_allows_only_443_says_so_and_stops",
]

#: Murmur's ACL bits, by the names its source uses.
WRITE = 0x01
ENTER = 0x04

#: The channels the rig builds, and the access lists that make them interesting.
#:
#: A token group is written `#name` and the rider types `name`. Written without
#: the `#` it matches a real group of that name instead, the token matches
#: nothing, and the channel stays shut — which reads as a broken client.
CHANNELS = ["Garage", "Clubhouse"]
TOKEN_GROUP = "#vip"
TOKEN = "vip"
ADMIN_GROUP = "#rideboss"
ADMIN_TOKEN = "rideboss"

#: The tests that are about moderation, in the order they are worth reading.
ADMIN_TESTS = [
    "an_admin_moves_kicks_and_bans_somebody",
    "an_admin_can_run_the_server",
    "an_acl_survives_a_round_trip",
    "an_admin_can_read_the_ban_list_and_is_told_nothing_about_permissions",
]


def run(cmd, **kw):
    """Run a command, returning its completed process; never shell."""
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def docker(*args, check=True):
    p = run(["docker", *args])
    if check and p.returncode != 0:
        sys.exit(f"docker {' '.join(args[:2])} failed:\n{p.stderr.strip()}")
    return p.stdout.strip()


def container_exists(name):
    out = docker("ps", "-a", "--filter", f"name=^{name}$", "--format", "{{.Names}}")
    return name in out.splitlines()


def container_running(name):
    out = docker("ps", "--filter", f"name=^{name}$", "--format", "{{.Names}}")
    return name in out.splitlines()


def sqlite(name, *statements):
    """Run statements against the server's database, inside the container."""
    p = run(["docker", "exec", name, "sqlite3", "/data/mumble-server.sqlite", *statements])
    if p.returncode != 0:
        sys.exit(f"sqlite3 failed:\n{p.stderr.strip()}")
    return p.stdout


def wait_for_port(port, seconds=30):
    deadline = time.time() + seconds
    while time.time() < deadline:
        with socket.socket() as s:
            s.settimeout(1)
            if s.connect_ex(("127.0.0.1", port)) == 0:
                return True
        time.sleep(0.5)
    return False


def write_env(values):
    ENV_FILE.write_text(
        "".join(f"{k}={v}\n" for k, v in values.items()), encoding="utf-8"
    )
    # The SuperUser password is in here. The repository is public.
    os.chmod(ENV_FILE, 0o600)


def read_env():
    if not ENV_FILE.exists():
        sys.exit("no .rig.env — run `rig.py up` first")
    out = {}
    for line in ENV_FILE.read_text(encoding="utf-8").splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            out[k] = v
    return out


def shape(name):
    """Give the server the channels and access lists the tests expect.

    Written straight into the database and followed by a restart, because
    Murmur reads the access lists once at boot: editing them underneath a
    running server changes the file and nothing else, which looks like the
    edit not having worked.
    """
    existing = dict(
        (row.split("|")[1], int(row.split("|")[0]))
        for row in sqlite(name, "select channel_id, name from channels;").splitlines()
        if "|" in row
    )
    next_id = max(existing.values(), default=0) + 1
    statements = []
    for channel in CHANNELS:
        if channel in existing:
            continue
        statements.append(
            f"insert into channels (server_id, channel_id, parent_id, name, inheritacl) "
            f"values (1, {next_id}, 0, '{channel}', 1);"
        )
        existing[channel] = next_id
        next_id += 1
    if statements:
        sqlite(name, *statements)

    clubhouse = existing["Clubhouse"]
    # Priorities are per channel and unique, so the rows are replaced rather
    # than added to: running `up` twice must leave one of each, not two.
    sqlite(
        name,
        f"delete from acl where channel_id = {clubhouse};",
        f"insert into acl values (1, {clubhouse}, 1, NULL, 'all', 1, 1, 0, {ENTER});",
        f"insert into acl values (1, {clubhouse}, 2, NULL, '{TOKEN_GROUP}', 1, 1, {ENTER}, 0);",
        "delete from acl where channel_id = 0 and priority = 8;",
        f"insert into acl values (1, 0, 8, NULL, '{ADMIN_GROUP}', 1, 1, {WRITE}, 0);",
    )
    docker("restart", name)
    return existing


def cmd_up(args):
    name, port = args.name, args.port
    if container_exists(name) and args.fresh:
        docker("rm", "-f", name)
    if not container_exists(name):
        password = secrets.token_urlsafe(12)
        docker(
            "run", "-d", "--name", name,
            "-p", f"{port}:64738/tcp", "-p", f"{port}:64738/udp",
            "-e", f"MUMBLE_CONFIG_BANDWIDTH={BANDWIDTH}",
            "-e", "MUMBLE_CONFIG_WELCOMETEXT=MumbleWay verification server",
            "-e", "MUMBLE_CONFIG_SUGGESTPUSHTOTALK=true",
            "-e", "MUMBLE_CONFIG_SUGGESTPOSITIONAL=true",
            # A test that reconnects on purpose would otherwise earn itself a
            # five-minute ban: Murmur counts every attempt, successful or not,
            # and bans at ten in two minutes.
            "-e", "MUMBLE_CONFIG_AUTOBANATTEMPTS=1000",
            "-e", f"MUMBLE_SUPERUSER_PASSWORD={password}",
            IMAGE,
        )
        write_env({
            "MW_LIVE": f"127.0.0.1:{port}",
            "MW_LIVE_SUPERUSER": password,
            "MW_LIVE_BANDWIDTH": str(BANDWIDTH),
            "MW_LIVE_CHANNEL": "Garage",
            "MW_LIVE_TOKEN_CHANNEL": "Clubhouse",
            "MW_LIVE_TOKEN": TOKEN,
            "MW_LIVE_ADMIN_TOKEN": ADMIN_TOKEN,
            "MW_LIVE_EXPECT_SUGGESTIONS": "1",
        })
        print(f"started {name} on {port}")
    else:
        if not container_running(name):
            docker("start", name)
            print(f"started {name} again")
        else:
            print(f"{name} was already up")
        # A server that outlived its `.rig.env` still knows its own password:
        # it was given to it as an environment variable and it keeps it. Taking
        # it back out beats telling somebody to throw away a working server.
        if not ENV_FILE.exists():
            config = json.loads(docker("inspect", name, "--format", "{{json .Config.Env}}"))
            password = next(
                (v.split("=", 1)[1] for v in config if v.startswith("MUMBLE_SUPERUSER_PASSWORD=")),
                None,
            )
            if password is None:
                sys.exit(f"{name} has no SuperUser password; `rig.py up --fresh` makes one")
            write_env({
                "MW_LIVE": f"127.0.0.1:{port}",
                "MW_LIVE_SUPERUSER": password,
                "MW_LIVE_BANDWIDTH": str(BANDWIDTH),
                "MW_LIVE_CHANNEL": "Garage",
                "MW_LIVE_TOKEN_CHANNEL": "Clubhouse",
                "MW_LIVE_TOKEN": TOKEN,
                "MW_LIVE_ADMIN_TOKEN": ADMIN_TOKEN,
                "MW_LIVE_EXPECT_SUGGESTIONS": "1",
            })

    if not wait_for_port(port):
        sys.exit(f"nothing is listening on {port}")
    channels = shape(name)
    if not wait_for_port(port):
        sys.exit("the server did not come back after the restart")
    print("channels:", ", ".join(f"{n}={i}" for n, i in sorted(channels.items())))
    print(f"token `{TOKEN}` opens Clubhouse; token `{ADMIN_TOKEN}` is an admin")
    print(f"environment written to {ENV_FILE.relative_to(ROOT)} (gitignored)")


def tinyproxy_conf(connect_ports):
    """A tinyproxy configuration allowing CONNECT to exactly these ports.

    The allow-list is the whole point of having two of these: a stock proxy
    permits CONNECT to 443 and refuses a Mumble port, which is the mistake a
    rider actually makes, and the client has to say that the *proxy* refused.
    """
    lines = [
        "Port 8888",
        "Listen 0.0.0.0",
        "Timeout 600",
        "MaxClients 50",
        "LogLevel Info",
        f"BasicAuth {PROXY_USER} {PROXY_PASSWORD}",
    ]
    lines += [f"ConnectPort {p}" for p in connect_ports]
    return "\n".join(lines) + "\n"


def run_tinyproxy(name, port, connect_ports):
    """Starts a tinyproxy with its configuration written in from here.

    `sh -c` rather than a bind mount: a mounted file would make this script care
    about how the host's paths look to the daemon, which differs between Docker
    Desktop and a plain Linux box for no benefit at all.
    """
    conf = tinyproxy_conf(connect_ports)
    docker(
        "run", "-d", "--name", name,
        "-p", f"127.0.0.1:{port}:8888",
        "--entrypoint", "sh",
        HTTP_IMAGE, "-c",
        f"printf '%s' {shlex_quote(conf)} > /etc/tinyproxy/tinyproxy.conf"
        " && exec tinyproxy -d -c /etc/tinyproxy/tinyproxy.conf",
    )


def shlex_quote(text):
    """Single-quotes text for a POSIX shell, the way shlex would."""
    return "'" + text.replace("'", "'\\''") + "'"


def cmd_proxies(args):
    """Puts proxies in front of the server and records how to reach them."""
    env = read_env()
    port = args.port
    wanted = {
        HTTP_NAME: lambda: run_tinyproxy(
            HTTP_NAME, HTTP_PORT,
            # The Mumble port, so a session can be dialled; and the SOCKS port,
            # so this proxy can be the first hop of a chain whose second hop is
            # the SOCKS one.
            [port, SOCKS_PORT],
        ),
        SOCKS_NAME: lambda: docker(
            "run", "-d", "--name", SOCKS_NAME,
            "-p", f"127.0.0.1:{SOCKS_PORT}:1080",
            "-e", f"PROXY_USER={PROXY_USER}",
            "-e", f"PROXY_PASSWORD={PROXY_PASSWORD}",
            SOCKS_IMAGE,
        ),
        # Allows exactly what a browser's proxy allows, and so refuses Mumble.
        STRICT_NAME: lambda: run_tinyproxy(STRICT_NAME, STRICT_PORT, [443]),
    }
    for name, start in wanted.items():
        if container_exists(name) and args.fresh:
            docker("rm", "-f", name)
        if container_exists(name):
            if not container_running(name):
                docker("start", name)
                print(f"started {name} again")
            else:
                print(f"{name} was already up")
            continue
        start()
        print(f"started {name}")

    for name, p in ((HTTP_NAME, HTTP_PORT), (SOCKS_NAME, SOCKS_PORT), (STRICT_NAME, STRICT_PORT)):
        if not wait_for_port(p):
            sys.exit(f"{name} is not listening on {p}:\n" + docker("logs", "--tail", "20", name))

    env.update({
        "MW_LIVE_PROXY_TARGET": f"{HOST_FROM_CONTAINER}:{port}",
        "MW_LIVE_PROXY_HTTP": f"127.0.0.1:{HTTP_PORT}",
        "MW_LIVE_PROXY_SOCKS": f"127.0.0.1:{SOCKS_PORT}",
        "MW_LIVE_PROXY_STRICT": f"127.0.0.1:{STRICT_PORT}",
        # The second hop as the *first* hop sees it, which is the only vantage
        # that matters: the HTTP proxy is the one that has to open it.
        "MW_LIVE_PROXY_INNER": f"socks5://{HOST_FROM_CONTAINER}:{SOCKS_PORT}",
        "MW_LIVE_PROXY_USER": PROXY_USER,
        "MW_LIVE_PROXY_PASSWORD": PROXY_PASSWORD,
    })
    write_env(env)
    print(f"http {HTTP_PORT} (CONNECT to {port} and {SOCKS_PORT}), "
          f"socks5 {SOCKS_PORT}, strict {STRICT_PORT} (443 only)")
    print(f"both want {PROXY_USER} / {PROXY_PASSWORD} \u2014 a rig credential, not a secret")
    print(f"proxies are asked for {HOST_FROM_CONTAINER}:{port}")


def cmd_state(args):
    name = args.name
    if not container_running(name):
        sys.exit(f"{name} is not running")
    print("-- channels")
    print(sqlite(name, "select channel_id, parent_id, name from channels order by channel_id;").strip())
    print("-- access lists")
    print(sqlite(
        name,
        "select channel_id, priority, group_name, grantpriv, revokepriv from acl "
        "order by channel_id, priority;",
    ).strip() or "(none)")
    print("-- registered users")
    print(sqlite(name, "select user_id, name from users order by user_id;").strip() or "(none)")
    print("-- bans")
    bans = sqlite(name, "select base, mask, reason from bans;").strip()
    print(bans or "(none)")
    print("-- who is connected (from the log)")
    log = docker("logs", "--tail", "400", name)
    live = {}
    for line in log.splitlines():
        if m := re.search(r"<(\d+):([^(]+)\(", line):
            session, who = m.group(1), m.group(2)
            if "Connection closed" in line or "Authenticated" in line:
                live[session] = (who, "Authenticated" in line)
    here = sorted(who for who, up in live.values() if up)
    print(", ".join(here) if here else "(nobody)")


def cmd_test(args):
    env = {**os.environ, **read_env()}
    names = args.names or ADMIN_TESTS if args.admin else args.names
    cmd = ["cargo", "test", "--test", "live_server"]
    if names and len(names) == 1:
        cmd.append(names[0])
    # One at a time: they share one server, and two tests moving the same rider
    # about at once would each be reading the other's doing.
    cmd += ["--", "--ignored", "--nocapture", "--test-threads=1"]
    if names and len(names) > 1:
        # One cargo run per test, because `cargo test` takes a single filter and
        # these must not share a server state they each change.
        failed = []
        for name in names:
            print(f"\n=== {name}")
            p = subprocess.run(
                ["cargo", "test", "--test", "live_server", name, "--",
                 "--ignored", "--nocapture"],
                cwd=ROOT / "core", env=env,
            )
            if p.returncode != 0:
                failed.append(name)
        if failed:
            sys.exit("failed: " + ", ".join(failed))
        print("\nall of them passed")
        return
    p = subprocess.run(cmd, cwd=ROOT / "core", env=env)
    sys.exit(p.returncode)


def cmd_down(args):
    for name in (args.name, HTTP_NAME, SOCKS_NAME, STRICT_NAME):
        if container_exists(name):
            docker("rm", "-f", name)
            print(f"removed {name}")
        elif name == args.name:
            print(f"{args.name} was not there")
    if ENV_FILE.exists() and args.forget:
        ENV_FILE.unlink()
        print("forgot the environment")


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--name", default=DEFAULT_NAME, help="container name")
    p.add_argument("--port", type=int, default=DEFAULT_PORT, help="host port")
    sub = p.add_subparsers(dest="what", required=True)

    up = sub.add_parser("up", help="start a server and shape it for the tests")
    up.add_argument("--fresh", action="store_true", help="throw away any existing one")
    up.set_defaults(fn=cmd_up)

    sub.add_parser("state", help="what the server holds").set_defaults(fn=cmd_state)

    admin = sub.add_parser("admin", help="run the moderation tests")
    admin.set_defaults(fn=cmd_test, admin=True, names=None)

    proxies = sub.add_parser("proxies", help="put proxies in front of the server")
    proxies.add_argument("--fresh", action="store_true", help="throw away any existing ones")
    proxies.set_defaults(fn=cmd_proxies)

    proxy = sub.add_parser("proxy", help="run the proxy tests (several minutes)")
    proxy.set_defaults(fn=cmd_test, admin=False, names=PROXY_TESTS)

    test = sub.add_parser("test", help="run live tests by name (default: all)")
    test.add_argument("names", nargs="*", help="test name filters")
    test.set_defaults(fn=cmd_test, admin=False)

    down = sub.add_parser("down", help="remove the server")
    down.add_argument("--forget", action="store_true", help="delete .rig.env too")
    down.set_defaults(fn=cmd_down)

    args = p.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
