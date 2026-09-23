#!/bin/sh
# Create a self-signed code-signing certificate called "Vox Dev" in the login keychain, so
# local builds carry a stable signature and macOS keeps the permission grants across rebuilds.
# One-time. The first codesign that uses the key shows a keychain dialog: click "Always Allow".
set -eu
NAME="${VOX_SIGNING_IDENTITY:-Vox Dev}"
DIR="$(mktemp -d)"
trap 'rm -rf "$DIR"' EXIT

cat > "$DIR/cfg" <<CFG
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
CFG
openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
  -keyout "$DIR/key.pem" -out "$DIR/cert.pem" -config "$DIR/cfg" 2>/dev/null
openssl pkcs12 -export -inkey "$DIR/key.pem" -in "$DIR/cert.pem" \
  -name "$NAME" -out "$DIR/id.p12" -passout pass:vox \
  -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1
security import "$DIR/id.p12" -k "$HOME/Library/Keychains/login.keychain-db" \
  -P vox -T /usr/bin/codesign -T /usr/bin/security
# Trust it for code signing so codesign accepts it without a warning.
security add-trusted-cert -p codeSign -k "$HOME/Library/Keychains/login.keychain-db" "$DIR/cert.pem" 2>/dev/null || true
echo "Created '$NAME'. Now run scripts/install-dev.sh."
security find-identity -v -p codesigning | grep "$NAME" || echo "(identity not visible yet; open Keychain Access once, then retry)"
