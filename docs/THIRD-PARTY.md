# Third-party assets

## Fonts

- **Silkscreen** (`tauri/packages/ui/src/colony/fonts/silkscreen-*.woff2`) —
  Jason Kottke, SIL Open Font License 1.1.
- **Springtale Symbols** (`tauri/packages/ui/src/colony/fonts/springtale-symbols.woff2`)
  — a subset of *Symbols Nerd Font Mono* (ryanoasis/nerd-fonts, MIT patcher)
  containing only the codepoints the utterance def table renders
  (`crates/springtale-cooperation/src/utterance/defs.rs`). Every glyph in the
  subset is from Material Design Icons (Pictogrammers, Apache License 2.0).
  Rebuilt by `scripts/build-symbol-font.sh`; the pinned upstream glyph table
  is `vendor/fonts/glyphnames.json` (its `METADATA.version` is the version).
