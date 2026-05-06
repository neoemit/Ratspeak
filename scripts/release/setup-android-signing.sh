#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
ANDROID_DIR="$REPO_DIR/src-tauri/gen/android"
KEYSTORE_FILE="$ANDROID_DIR/upload-keystore.jks"
PROPERTIES_FILE="$ANDROID_DIR/keystore.properties"
REQUIRED="${ANDROID_SIGNING_REQUIRED:-0}"

missing=()
for name in \
    ANDROID_UPLOAD_KEYSTORE_BASE64 \
    ANDROID_UPLOAD_KEYSTORE_PASSWORD \
    ANDROID_UPLOAD_KEY_ALIAS \
    ANDROID_UPLOAD_KEY_PASSWORD
do
    if [ -z "${!name:-}" ]; then
        missing+=("$name")
    fi
done

if [ "${#missing[@]}" -gt 0 ]; then
    if [ "$REQUIRED" = "1" ]; then
        echo "ERROR: Android signing is required, but these variables are missing:" >&2
        printf '  %s\n' "${missing[@]}" >&2
        exit 1
    fi

    if [ -f "$PROPERTIES_FILE" ]; then
        echo "Using existing Android signing properties at $PROPERTIES_FILE"
    else
        echo "Android signing secrets are not configured; release builds will be unsigned."
    fi
    exit 0
fi

decode_base64() {
    if base64 --help 2>&1 | grep -q -- '--decode'; then
        base64 --decode
    else
        base64 -D
    fi
}

umask 077
mkdir -p "$ANDROID_DIR"
printf '%s' "$ANDROID_UPLOAD_KEYSTORE_BASE64" | decode_base64 > "$KEYSTORE_FILE"

cat > "$PROPERTIES_FILE" <<EOF
storeFile=upload-keystore.jks
storePassword=$ANDROID_UPLOAD_KEYSTORE_PASSWORD
keyAlias=$ANDROID_UPLOAD_KEY_ALIAS
keyPassword=$ANDROID_UPLOAD_KEY_PASSWORD
EOF

echo "Android release signing configured at $PROPERTIES_FILE"
