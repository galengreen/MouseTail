#!/bin/bash
# Sign a release's downloads and write the feeds installed copies update from:
#   dist/latest.json   Linux: the version, and each tarball's URL and signature
#   dist/appcast.xml   Mac: Sparkle's feed, pointing at MouseTail-macos.zip
# The key is the Ed25519 release key, in $UPDATE_SIGNING_KEY (PEM) or the file named by
# $UPDATE_SIGNING_KEY_FILE. Each signature is checked against the public key built into the
# apps, so a mismatched key fails here instead of shipping updates nobody can install.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
tag=v$version
base=https://github.com/galengreen/MouseTail/releases/download/$tag
public=$(sed -n 's/^pub const PUBLIC_KEY: &str = "\(.*\)";/\1/p' crates/core/src/update.rs)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
key=${UPDATE_SIGNING_KEY_FILE:-$work/key.pem}
[[ -n ${UPDATE_SIGNING_KEY_FILE:-} ]] || printf '%s\n' "$UPDATE_SIGNING_KEY" > "$key"
# The public key as DER: the fixed Ed25519 header, then the raw 32 bytes.
{ printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'; printf '%s' "$public" | base64 -d; } > "$work/public.der"

# Sets $signature. (Not run in a subshell, so a failure stops the script.)
sign() {
  openssl pkeyutl -sign -inkey "$key" -rawin -in "$1" -out "$work/sig"
  if ! openssl pkeyutl -verify -pubin -inkey "$work/public.der" -keyform DER -rawin -in "$1" \
    -sigfile "$work/sig" >/dev/null 2>&1; then
    echo "the signing key doesn't match PUBLIC_KEY in crates/core/src/update.rs" >&2
    exit 1
  fi
  signature=$(base64 < "$work/sig" | tr -d '\n')
}

linux='{}'
for tarball in dist/mousetail-linux-*.tar.gz; do
  arch=${tarball#dist/mousetail-linux-}
  arch=${arch%.tar.gz}
  sign "$tarball"
  linux=$(jq --arg arch "$arch" --arg url "$base/${tarball#dist/}" --arg sig "$signature" \
    '. + {($arch): {url: $url, signature: $sig}}' <<<"$linux")
done
jq -n --arg version "$version" --argjson linux "$linux" '{version: $version, linux: $linux}' > dist/latest.json

zip=dist/MouseTail-macos.zip
sign "$zip"
cat > dist/appcast.xml <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>MouseTail</title>
    <item>
      <title>MouseTail $version</title>
      <pubDate>$(LC_ALL=C date -u "+%a, %d %b %Y %H:%M:%S +0000")</pubDate>
      <sparkle:version>$version</sparkle:version>
      <sparkle:shortVersionString>$version</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>13.0</sparkle:minimumSystemVersion>
      <sparkle:releaseNotesLink>https://github.com/galengreen/MouseTail/releases/tag/$tag</sparkle:releaseNotesLink>
      <enclosure url="$base/MouseTail-macos.zip" length="$(wc -c < "$zip" | tr -d ' ')" type="application/octet-stream" sparkle:edSignature="$signature"/>
    </item>
  </channel>
</rss>
XML
echo "wrote dist/latest.json and dist/appcast.xml for $version"
