#!/bin/sh
set -eu

STATE_DIR=/var/lib/holochain
READY_FILE="$STATE_DIR/.provisioned-hc07-v3"
PASSWORD_FILE=${HOLOCHAIN_KEYSTORE_PASSWORD_FILE:-/run/secrets/holochain_keystore_password}
ADMIN_PORT=8888

if [ ! -r "$PASSWORD_FILE" ]; then
    echo "ERROR: the Holochain keystore password secret is missing or unreadable" >&2
    exit 1
fi

HC_VERSION=$(hc --version 2>&1)
case "$HC_VERSION" in
    *0.7.0*) ;;
    *) echo "ERROR: expected hc 0.7.0, found: $HC_VERSION" >&2; exit 1 ;;
esac

cleanup() {
    if [ -n "${API_PROXY_PID:-}" ]; then
        kill -TERM "$API_PROXY_PID" 2>/dev/null || true
        wait "$API_PROXY_PID" 2>/dev/null || true
    fi
    if [ -n "${CONDUCTOR_PID:-}" ]; then
        kill -TERM "$CONDUCTOR_PID" 2>/dev/null || true
        wait "$CONDUCTOR_PID" 2>/dev/null || true
    fi
}
trap cleanup INT TERM EXIT

# The daemon intentionally binds its local API to loopback. This proxy exposes
# it only through the host-loopback port mapping without exposing Holochain.
socat TCP-LISTEN:8892,bind=0.0.0.0,reuseaddr,fork TCP:127.0.0.1:8890 &
API_PROXY_PID=$!

holochain --piped --config-path /etc/holochain/conductor.yaml < "$PASSWORD_FILE" &
CONDUCTOR_PID=$!

attempt=0
until hc client call --port "$ADMIN_PORT" list-apps >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    if ! kill -0 "$CONDUCTOR_PID" 2>/dev/null; then
        echo "ERROR: the Holochain conductor exited during startup" >&2
        wait "$CONDUCTOR_PID"
        exit 1
    fi
    if [ "$attempt" -ge 60 ]; then
        echo "ERROR: the Holochain admin interface did not become ready" >&2
        exit 1
    fi
    sleep 1
done

app_is_installed() {
    hc client call --port "$ADMIN_PORT" list-apps 2>/dev/null | grep -q "$1"
}

install_app() {
    app_id=$1
    bundle=$2
    app_port=$3

    if ! app_is_installed "$app_id"; then
        agent_output=$(hc client call --port "$ADMIN_PORT" new-agent)
        agent_key=$(printf '%s\n' "$agent_output" \
            | grep -o 'uhCAk[A-Za-z0-9_-]*' \
            | head -n 1 || true)
        case "$agent_key" in
            uhCAk*) ;;
            *) echo "ERROR: failed to generate an agent key for $app_id" >&2; exit 1 ;;
        esac
        hc client call --port "$ADMIN_PORT" install-app \
            --app-id "$app_id" \
            --agent-key "$agent_key" \
            "$bundle"
    fi

    hc client call --port "$ADMIN_PORT" enable-app "$app_id" >/dev/null

    if ! hc client call --port "$ADMIN_PORT" list-app-ws 2>/dev/null \
        | grep -Eq "(^|[^0-9])${app_port}([^0-9]|$)"; then
        hc client call --port "$ADMIN_PORT" add-app-ws "$app_port" \
            --allowed-origins "*" \
            --installed-app-id "$app_id"
    fi
}

rm -f "$READY_FILE"
install_app ainonymous /opt/happs/ainonymous-core.happ 8889
install_app hybridnode /opt/happs/hybridnode.happ 8891
umask 077
printf '%s\n' 'Holochain 0.7.0 / DNA epoch v3 provisioned' > "$READY_FILE"

wait "$CONDUCTOR_PID"
