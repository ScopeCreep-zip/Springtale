#!/usr/bin/env sh
# Subset Symbols Nerd Font Mono to exactly the codepoints the utterance def
# table uses (ALIGNMENT-PLAN 3.4 A). The subset is committed, so a build needs
# neither Python nor the upstream TTF; run this after the def table changes.
#
# Input: the upstream TTF from the NerdFontsSymbolsOnly release (MIT patcher;
# glyph sets under their own licences: Font Awesome OFL 1.1, Material Design
# Icons Apache 2.0, Codicons CC BY 4.0, Octicons MIT). Attribution lives in
# docs/THIRD-PARTY.md. glyphnames.json is the Nerd Fonts source of truth
# (ryanoasis/nerd-fonts), committed under vendor/fonts/ with its version.
set -e
cd "$(dirname "$0")/.."

VENDOR=vendor/fonts
TTF="$VENDOR/SymbolsNerdFontMono-Regular.ttf"
OUT=tauri/packages/ui/src/colony/fonts
GLYPHS="$OUT/glyphs.txt"
WOFF2="$OUT/springtale-symbols.woff2"

if [ ! -f "$TTF" ]; then
  ZIP="$VENDOR/NerdFontsSymbolsOnly.zip"
  if [ ! -f "$ZIP" ]; then
    echo "fetching NerdFontsSymbolsOnly.zip (not committed; see .gitignore)" >&2
    curl -sL -o "$ZIP" https://github.com/ryanoasis/nerd-fonts/releases/latest/download/NerdFontsSymbolsOnly.zip
  fi
  unzip -o -q "$ZIP" SymbolsNerdFontMono-Regular.ttf LICENSE -d "$VENDOR"
fi

# 1. Assert every (name, codepoint) constant in utterance/defs.rs matches upstream.
cargo run -q -p springtale-cli -- cooperation glyphs --check "$VENDOR/glyphnames.json" > /dev/null
# 2. Emit the codepoint list the subset is built from.
cargo run -q -p springtale-cli -- cooperation glyphs > "$GLYPHS"

# pyftsubset from fonttools; woff2 output needs the brotli extra.
if command -v pyftsubset > /dev/null 2>&1; then
  SUBSET=pyftsubset
else
  SUBSET="uvx --from fonttools[woff] pyftsubset"
fi
$SUBSET "$TTF" \
  --unicodes-file="$GLYPHS" \
  --flavor=woff2 --no-hinting --desubroutinize \
  --output-file="$WOFF2"
# Served at /ui/assets/springtale-symbols.woff2 for third-party frontends.
cp "$WOFF2" tauri/apps/dashboard/public/assets/
echo "wrote $WOFF2 ($(wc -c < "$WOFF2") bytes, $(grep -c . "$GLYPHS") codepoints)"
