# Release Checklist

## Before tagging

- Confirm `cargo fmt --check` passes
- Confirm `cargo test` passes
- Review provenance and notice files for accuracy
- Review README and security docs for any changed behavior
- Confirm new public flags and subcommands are documented

## GitHub release prep

- Ensure `main` is green in CI
- Draft release notes with user-visible changes
- Call out deferred features clearly:
  - Opencode persistent transport remains v2
  - `wrangle` native server remains v3

## After release

- Verify install/build instructions still work
- Open follow-up issues for deferred items discovered during release prep
