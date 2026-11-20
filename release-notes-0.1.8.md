SVN 0.1.8 for OpenIDEA

- Fix W160013/E200009 when opening history diffs for files modified, replaced or deleted inside copied branches. Resolve the nearest copy source at its recorded revision and show the source path/version in the native diff.
- Keep ordinary replacement comparisons against the previous target and surface real read/authentication failures.
- Include 40 bundled file icons and 14 folder icons from 0.1.7. No additional SVN requests or external icon downloads.
- Preserve directory double-click, ignore/unignore, confirmed revert, tree/list views and default-off configurable auto-refresh.

Validation: 63 Rust tests and 12 Node tests passed; fmt, clippy, pinned SDK/icon checks, Wasm release build and official signed package validation passed. Real local SVN repositories cover copied and nested branches, replacement, deletion, Unicode paths and later removal of copy sources. Windows GUI and remote development hosts remain unverified.

Requires SVN command-line client and OpenIDEA plugin API 1.10. Same plugin ID, signing publisher and capabilities as 0.1.6.

SHA256: a74a0709c78a6dc5cafd3844b2f198fb2ca6691bd52a98b05d8bc6bb6822a025
