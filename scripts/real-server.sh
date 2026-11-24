#!/bin/sh
# `make real-server`: the ADR-023 spike, the exit tests of M3 and M4, and
# what M9 can prove without a real account, against two throwaway mail
# servers in containers, started here and removed after.
#
# GreenMail is the server PLAN §9 names, and it offers UIDPLUS, MOVE, IDLE
# and XOAUTH2 but no CONDSTORE; Dovecot offers CONDSTORE and no XOAUTH2.
# Neither offers SPECIAL-USE (ADR-023). GreenMail offers STARTTLS on no
# port, so a connection that begins with it is proven on Dovecot's port 143
# alone. Only GreenMail takes mail to send,
# so sending and the Sent copy are proven there alone; archive, trash and
# undo are proven on both, by the engine and through the interface on a
# terminal over two accounts. There too: `tuimex setup` on a terminal from
# a first launch, the certificate shown and pinned, `tuimex doctor`, and
# XOAUTH2, of which GreenMail takes the account's password as the token
# and so proves the exchange and nothing about Google (ADR-023). The
# tests of one binary run one at a time:
# they share an Inbox. Each leg below fails unless the tests print
# that they ran.
#
# CONTAINER names the runtime: `podman` by default, `docker` in CI, and
# `flatpak-spawn --host podman` from inside a toolbox. Only containers
# named tuimex-greenmail and tuimex-dovecot are ever touched, both bound
# to this machine alone, and a third, tuimex-dovecot-ns: a Dovecot that
# keeps every folder under `INBOX.`, as Courier and many hosts do, where
# an Archive and a Trash made at the top are refused (ADR-048). A fourth,
# tuimex-radicale, is a CalDAV server, where an event is written, changed,
# answered and deleted (ADR-062); it serves plain HTTP, which the engine
# allows on 127.0.0.1 alone, and it is bound there. The accounts and their
# passwords are made up here.
#
# Two checkouts on one machine would take each other's containers and
# ports. REAL_SERVER_TAG is added to every name, and REAL_SERVER_PORTS is
# the two digits GreenMail's ports begin with (13 by default: 13993, 13465,
# 13025), Dovecot's being the next (14993), the namespaced Dovecot's the
# one after (15993) and Radicale's the one after that (16232), which is
# why the two digits can be at most 62.
set -eu

runtime=${CONTAINER:-podman}
tag=${REAL_SERVER_TAG:-}
green=${REAL_SERVER_PORTS:-13}
dove=$((green + 1))
spaced=$((green + 2))
caldav=$((green + 3))
root=$(cd "$(dirname "$0")/.." && pwd)
# Under the build directory: never a shared temporary one (ADR-011).
dir=$root/target/real-server
# Each image is named by its tag, for a reader, and by its digest, which is
# what the runtime fetches: the servers a run passed against are the ones
# the next run starts. GreenMail's digest is an index over amd64 and arm64;
# Dovecot publishes this version for amd64 alone.
greenmail=docker.io/greenmail/standalone:2.1.15@sha256:309ba544ea482e5d4265d09921cdab07c8dad2cc4481d715c9b8ff5d3f0a149f
dovecot=docker.io/dovecot/dovecot:2.3.21.1@sha256:547d28f9a893c93a1d2f6dc164880cd7bfa74009106dfa46b36a6a2ee636d921
# PLACEHOLDER: Radicale's digest has not been taken from a pulled image.
# Pull the tag below, put the digest the runtime reports in its place
# (`podman image inspect --format '{{.Digest}}' <image>`, or RepoDigests
# under docker), and check the tag is the one wanted. Until then the
# CalDAV leg is refused, after the mail legs have run.
radicale_tag=docker.io/tomsquest/docker-radicale:3.5.4.0
radicale_digest=sha256:FILL-FROM-THE-PULLED-IMAGE
radicale=$radicale_tag@$radicale_digest

remove() {
    $runtime rm -f "tuimex-greenmail$tag" "tuimex-dovecot$tag" "tuimex-dovecot-ns$tag" "tuimex-radicale$tag" \
        >/dev/null 2>&1 || true
}
trap remove EXIT
remove

mkdir -p "$dir"
# One certificate for localhost, which each test pins as a user would.
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj '/CN=localhost' \
    -addext 'subjectAltName=DNS:localhost' -addext 'basicConstraints=critical,CA:FALSE' \
    -keyout "$dir/key.pem" -out "$dir/cert.pem" 2>/dev/null
openssl pkcs12 -export -inkey "$dir/key.pem" -in "$dir/cert.pem" -name greenmail \
    -out "$dir/greenmail.p12" -passout pass:changeit
# The servers run as other users inside their containers.
# The image's own configuration, less the protocols nothing here speaks,
# with its one namespace given a prefix.
cat >"$dir/dovecot-ns.conf" <<'CONF'
mail_home=/srv/mail/%Lu
mail_location=sdbox:~/Mail
mail_uid=1000
mail_gid=1000
protocols = imap
first_valid_uid = 1000
last_valid_uid = 1000
passdb {
  driver = static
  args = password=pass
}
ssl=yes
ssl_cert=<cert.pem
ssl_key=<key.pem
namespace inbox {
  inbox = yes
  prefix = INBOX.
  separator = .
}
listen = *
log_path=/dev/stdout
CONF
chmod 644 "$dir/key.pem" "$dir/greenmail.p12" "$dir/dovecot-ns.conf"

$runtime run -d --name "tuimex-greenmail$tag" -p "127.0.0.1:${green}993:3993" \
    -p "127.0.0.1:${green}465:3465" -p "127.0.0.1:${green}025:3025" \
    -v "$dir/greenmail.p12:/cert/greenmail.p12:ro,Z" \
    -e GREENMAIL_OPTS='-Dgreenmail.setup.test.all -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users=spike:throwaway@example.com,second:throwaway@example.com,third:its0wn@example.com -Dgreenmail.tls.keystore.file=/cert/greenmail.p12 -Dgreenmail.tls.keystore.password=changeit' \
    "$greenmail" >/dev/null
# Dovecot's image takes any user with the password `pass`.
$runtime run -d --name "tuimex-dovecot$tag" -p "127.0.0.1:${dove}993:993" -p "127.0.0.1:${dove}143:143" \
    -v "$dir/cert.pem:/etc/dovecot/cert.pem:ro,Z" -v "$dir/key.pem:/etc/dovecot/key.pem:ro,Z" \
    "$dovecot" >/dev/null
$runtime run -d --name "tuimex-dovecot-ns$tag" -p "127.0.0.1:${spaced}993:993" \
    -v "$dir/cert.pem:/etc/dovecot/cert.pem:ro,Z" -v "$dir/key.pem:/etc/dovecot/key.pem:ro,Z" \
    -v "$dir/dovecot-ns.conf:/etc/dovecot/dovecot.conf:ro,Z" \
    "$dovecot" >/dev/null

ready() {
    (sleep 1; printf 'a LOGOUT\r\n') |
        openssl s_client -quiet -connect "localhost:$1" -CAfile "$dir/cert.pem" 2>/dev/null | grep -q '^\* OK'
}
for port in "${green}993" "${dove}993" "${spaced}993"; do
    tries=0
    until ready "$port"; do
        tries=$((tries + 1))
        if [ "$tries" -gt 60 ]; then
            echo "real-server: nothing answered on port $port" >&2
            exit 1
        fi
    done
done

# leg <name> <port> <password> <capabilities it must offer> <what the spike must report>
#     <lines it must print besides, `_` for a space> [<SMTP over TLS port> <plain SMTP port>
#     <the password of the user `third`, where it is not the others'>]
# With STARTTLS_PORT set, the leg also has a port where TLS begins with STARTTLS.
leg() {
    log=$dir/$1.log
    export TUIMEX_SERVER_HOST=localhost TUIMEX_SERVER_PORT="$2" TUIMEX_SERVER_USER=spike TUIMEX_SERVER_USER2=second
    export TUIMEX_SERVER_USER3=third TUIMEX_SERVER_PASSWORD3="${9:-$3}"
    export TUIMEX_SERVER_PASSWORD="$3" TUIMEX_SERVER_CA="$dir/cert.pem" TUIMEX_SERVER_REQUIRE="$4"
    unset TUIMEX_SERVER_SMTPS_PORT TUIMEX_SERVER_SMTP_PORT TUIMEX_SERVER_STARTTLS_PORT
    if [ -n "${STARTTLS_PORT:-}" ]; then
        export TUIMEX_SERVER_STARTTLS_PORT="$STARTTLS_PORT"
    fi
    if [ -n "${7:-}" ]; then
        export TUIMEX_SERVER_SMTPS_PORT="$7" TUIMEX_SERVER_SMTP_PORT="$8"
    fi
    (cd "$root" &&
        cargo test --locked -p tuimex-engine --test spike --test real_server --test sign_in -- --nocapture --test-threads=1 &&
        cargo test --locked -p tuimex-engine --lib imap::tests::without_move -- --nocapture &&
        cargo test --locked -p tuimex --test real_server --test setup -- --nocapture --test-threads=1) >"$log" 2>&1 || {
        cat "$log"
        echo "real-server: the $1 leg failed" >&2
        exit 1
    }
    # With --nocapture a test's first line follows its name on one line.
    grep -E '(spike|real-server[a-z -]*): |^test result' "$log"
    for ran in "spike: ran $5" 'real-server: ran sync' 'real-server: ran idle' 'real-server: ran certificate' \
        'real-server: ran tuimex-sync' 'real-server: ran tui:' 'real-server: ran tui-read (' 'real-server: ran tui-archive (under_"",' \
        'real-server: ran tui-empty-trash (' 'real-server: ran tui-rename (' 'real-server: ran archive-trash-undo' 'real-server: ran no-move' $6; do
        ran=$(printf %s "$ran" | tr _ ' ')
        grep -qF "$ran" "$log" || {
            echo "real-server: the $1 leg did not print \"$ran\"; a test was skipped or renamed" >&2
            exit 1
        }
    done
    echo "real-server: the $1 leg passed"
}
STARTTLS_PORT=
leg greenmail "${green}993" throwaway move,uidplus uidvalidity,idle,move,uidplus,xoauth2 \
    'real-server:_ran_send_( real-server:_ran_send-tls_( real-server:_ran_certificate-shown real-server:_ran_xoauth2 real-server:_ran_setup real-server:_ran_doctor-broken real-server:_ran_tui-send_( real-server:_ran_tui-send-refused_( real-server_starttls:_NOT_RUN' \
    "${green}465" "${green}025" its0wn
STARTTLS_PORT="${dove}143"
leg dovecot "${dove}993" pass condstore,move,uidplus uidvalidity,condstore,idle,move,uidplus,xoauth2-absent \
    'real-server_send:_NOT_RUN real-server_send-tls:_NOT_RUN real-server_certificate-shown:_NOT_RUN real-server_xoauth2:_NOT_RUN real-server_setup:_NOT_RUN real-server_tui-send:_NOT_RUN real-server_tui-send-refused:_NOT_RUN real-server:_ran_starttls_('

# The namespaced Dovecot answers one question: are an Archive and a Trash
# made where that server makes folders, and are messages moved into them.
log=$dir/dovecot-ns.log
export TUIMEX_SERVER_PORT="${spaced}993" TUIMEX_SERVER_PREFIX=INBOX.
unset TUIMEX_SERVER_STARTTLS_PORT
(cd "$root" &&
    cargo test --locked -p tuimex --test real_server an_archive_and_a_trash -- --nocapture) >"$log" 2>&1 || {
    cat "$log"
    echo "real-server: the dovecot-ns leg failed" >&2
    exit 1
}
grep -E 'real-server[a-z -]*: |^test result' "$log"
grep -qF 'real-server: ran tui-archive (under "INBOX.",' "$log" || {
    echo "real-server: the dovecot-ns leg did not make its folders under INBOX.; the test was skipped or renamed" >&2
    exit 1
}
echo "real-server: the dovecot-ns leg passed"

# Radicale answers whether an event is written, changed, answered and
# deleted over CalDAV, each write conditional on the tag last read, and
# whether a refused sign-in and a redirect are said as such. A leg that
# cannot start fails: a skip must never read as a pass.
case $radicale_digest in
*FILL*)
    echo "real-server: the radicale leg was not started: its image has no digest yet (radicale_digest, at the top)" >&2
    exit 1
    ;;
esac
cat >"$dir/radicale.conf" <<'CONF'
[server]
hosts = 0.0.0.0:5232
[auth]
type = htpasswd
htpasswd_filename = /config/users
htpasswd_encryption = plain
[rights]
type = owner_only
[storage]
filesystem_folder = /data/collections
CONF
printf 'caldav:throwaway-calendar\n' >"$dir/radicale-users"
chmod 644 "$dir/radicale.conf" "$dir/radicale-users"
$runtime run -d --name "tuimex-radicale$tag" -p "127.0.0.1:${caldav}232:5232" \
    -v "$dir/radicale.conf:/config/config:ro,Z" -v "$dir/radicale-users:/config/users:ro,Z" \
    "$radicale" >/dev/null
tries=0
until curl -s -o /dev/null --max-time 2 "http://127.0.0.1:${caldav}232/"; do
    tries=$((tries + 1))
    if [ "$tries" -gt 60 ]; then
        echo "real-server: nothing answered on port ${caldav}232" >&2
        exit 1
    fi
    sleep 1
done
log=$dir/radicale.log
export TUIMEX_CALDAV_SERVER="http://127.0.0.1:${caldav}232" TUIMEX_CALDAV_USER=caldav
export TUIMEX_CALDAV_PASSWORD=throwaway-calendar
(cd "$root" &&
    cargo test --locked -p tuimex-engine --test caldav_server -- --nocapture --test-threads=1) >"$log" 2>&1 || {
    cat "$log"
    echo "real-server: the radicale leg failed" >&2
    exit 1
}
grep -E 'real-server[a-z -]*: |^test result' "$log"
for ran in 'real-server: ran caldav-write (' 'real-server: ran caldav-redirect ('; do
    grep -qF "$ran" "$log" || {
        echo "real-server: the radicale leg did not print \"$ran\"; a test was skipped or renamed" >&2
        exit 1
    }
done
echo "real-server: the radicale leg passed"
