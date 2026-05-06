#!/bin/bash
# Build dashboard CSS — concatenates modular CSS files in dependency order
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CSS_DIR="$SCRIPT_DIR/static/css"
OUT="$SCRIPT_DIR/static/style.css"

cat \
    "$CSS_DIR/00-tokens.css" \
    "$CSS_DIR/01-reset.css" \
    "$CSS_DIR/02-typography.css" \
    "$CSS_DIR/03-scrollbar.css" \
    "$CSS_DIR/04-layout.css" \
    "$CSS_DIR/05-panels.css" \
    "$CSS_DIR/06-forms.css" \
    "$CSS_DIR/07-components.css" \
    "$CSS_DIR/08-modals.css" \
    "$CSS_DIR/09-messaging.css" \
    "$CSS_DIR/10-views.css" \
    "$CSS_DIR/11-games.css" \
    "$CSS_DIR/12-animations.css" \
    "$CSS_DIR/13-responsive.css" \
    > "$OUT"

# Minify: strip comments, collapse whitespace, trim lines
UNMIN_SIZE=$(wc -c < "$OUT" | tr -d ' ')
sed -i '' -e 's|/\*[^*]*\*\+\([^/*][^*]*\*\+\)*/||g' "$OUT" 2>/dev/null || true
# Remove remaining multi-line comments via perl (sed can't handle multi-line well)
if command -v perl &>/dev/null; then
    perl -0777 -pi -e 's{/\*.*?\*/}{}gs' "$OUT"
fi
# Collapse blank lines and trim trailing whitespace
sed -i '' -e '/^[[:space:]]*$/d' -e 's/[[:space:]]*$//' "$OUT" 2>/dev/null || true
MIN_SIZE=$(wc -c < "$OUT" | tr -d ' ')

echo "Built $OUT ($(wc -l < "$OUT" | tr -d ' ') lines, ${UNMIN_SIZE}B → ${MIN_SIZE}B)"
