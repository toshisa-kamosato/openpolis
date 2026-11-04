#!/usr/bin/env bash
# openpolis installer for a Debian/Ubuntu machine. DRAFT, NOT TESTED: read README.md before running it.
# Safe to run again: it keeps what already exists.
set -euo pipefail

[ "$(id -u)" = 0 ] || { echo "Eseguilo come root (sudo ./install.sh)."; exit 1; }
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/.." && pwd)
ROOT=/opt/openpolis
DATA=/var/lib/openpolis

ask() { local v; read -r -p "$1 [$2]: " v; echo "${v:-$2}"; }
askpw() { local a b; read -r -s -p "$1: " a; echo >&2; read -r -s -p "Ripetila: " b; echo >&2; [ "$a" = "$b" ] || { echo "Le password non coincidono." >&2; exit 1; }; echo "$a"; }
rand() { head -c 24 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' | head -c 24; }

echo "== 1. Programmi necessari"
apt-get update -qq
apt-get install -y -qq curl ca-certificates build-essential pkg-config docker.io docker-compose-v2 >/dev/null
systemctl enable --now docker >/dev/null

echo "== 2. Domande"
if [ -f "$ROOT/.env" ]; then
  echo "Trovata una configurazione in $ROOT/.env: la riuso."
  set -a; . "$ROOT/.env"; set +a
else
  DOMAIN=$(ask "Dominio della rete (già tuo, con accesso ai record DNS)" "openpolis.site")
  ACME_EMAIL=$(ask "Email per i certificati" "admin@$DOMAIN")
  COMMUNITY=$(ask "Nome della comunità" "La mia comunità")
  CURRENCY_NAME=$(ask "Nome della moneta" "Lux")
  CURRENCY_SYMBOL=$(ask "Simbolo della moneta" "LUX")
  ADMIN_USER=$(ask "Nome utente dell'amministratore" "amministratore")
  ADMIN_PASSWORD=$(askpw "Password dell'amministratore (almeno 10 caratteri)")
  mkdir -p "$ROOT"; umask 077
  cat > "$ROOT/.env" <<ENV
DOMAIN=$DOMAIN
ACME_EMAIL=$ACME_EMAIL
COMMUNITY=$COMMUNITY
CURRENCY_NAME=$CURRENCY_NAME
CURRENCY_SYMBOL=$CURRENCY_SYMBOL
ADMIN_USER=$ADMIN_USER
DB_PASSWORD=$(rand)
WP_ADMIN_PASSWORD=$(rand)
ENV
  umask 022
  set -a; . "$ROOT/.env"; set +a
fi

echo "== 3. luxd"
if [ ! -x /usr/local/bin/luxd ]; then
  command -v cargo >/dev/null || { curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal; . "$HOME/.cargo/env"; }
  (cd "$REPO" && cargo build --release -p luxd)
  install -m 0755 "$REPO/target/release/luxd" /usr/local/bin/luxd
fi
id openpolis >/dev/null 2>&1 || useradd --system --home "$DATA" --create-home --shell /usr/sbin/nologin openpolis
usermod -aG docker openpolis
if [ ! -f "$DATA/node.db" ]; then
  [ -n "${ADMIN_PASSWORD:-}" ] || ADMIN_PASSWORD=$(askpw "Password dell'amministratore")
  sudo -u openpolis env LUX_HOME="$DATA" LUX_ADMIN_PASSWORD="$ADMIN_PASSWORD" luxd init \
    --name "$COMMUNITY" --domain "$DOMAIN" --currency-name "$CURRENCY_NAME" --currency-symbol "$CURRENCY_SYMBOL" --admin-user "$ADMIN_USER"
fi
install -m 0644 "$HERE/openpolis.service" /etc/systemd/system/openpolis.service
systemctl daemon-reload && systemctl enable --now openpolis

echo "== 4. Servizi (Docker)"
cp "$HERE/compose.yaml" "$HERE/Caddyfile" "$ROOT/"
sed "s/__DOMAIN__/$DOMAIN/g" "$HERE/element-config.json.tmpl" > "$ROOT/element-config.json"
mkdir -p "$ROOT/synapse"
if [ ! -f "$ROOT/synapse/homeserver.yaml" ]; then
  docker run --rm -v "$ROOT/synapse:/data" -e SYNAPSE_SERVER_NAME="$DOMAIN" -e SYNAPSE_REPORT_STATS=no matrixdotorg/synapse:latest generate
  echo "encryption_enabled_by_default_for_room_type: all" >> "$ROOT/synapse/homeserver.yaml"
fi
(cd "$ROOT" && docker compose --env-file .env up -d)

echo "== 5. Blog (WordPress Multisite)"
sleep 20
if ! docker exec openpolis-wpcli wp core is-installed --network 2>/dev/null; then
  docker exec openpolis-wpcli wp core multisite-install --url="https://$DOMAIN" --subdomains --title="$COMMUNITY" \
    --admin_user=wpadmin --admin_password="$WP_ADMIN_PASSWORD" --admin_email="wpadmin@$DOMAIN" --skip-email
fi

echo "== 6. Chiave DKIM della posta"
docker exec openpolis-mail setup config dkim domain "$DOMAIN" || true

IP=$(curl -s https://api.ipify.org || true)
echo
echo "== Fatto. Ora aggiungi questi record DNS presso il tuo registrar:"
echo
luxd --home "$DATA" dns --ip "${IP:-INDIRIZZO.PUBBLICO}" || true
echo
echo "Chiave DKIM (da mettere nel record mail._domainkey):"
cat "$ROOT"/../../var/lib/docker/volumes/openpolis_mail_config/_data/opendkim/keys/"$DOMAIN"/mail.txt 2>/dev/null \
  || docker exec openpolis-mail cat "/tmp/docker-mailserver/opendkim/keys/$DOMAIN/mail.txt" || true
echo
echo "Poi apri https://$DOMAIN/admin per generare il primo PIN d'invito."
