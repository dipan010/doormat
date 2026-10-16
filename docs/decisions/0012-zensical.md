# 0012: Build the documentation with Zensical

**Status:** accepted, 2026-10-16

**Context.** The site was built with MkDocs 1.6 and Material for MkDocs 9.7.
Material is in maintenance mode and receives security fixes only until
November 2026 (#14). MkDocs 2.0 drops the plugin and theme systems the site
uses, so it is not a way forward.

**Options.** Stay on Material until fixes stop; move to Zensical, the
successor from Material's authors; or move to another generator (Sphinx,
Docusaurus) and rewrite the configuration and theme.

**Decision.** Zensical, pinned (0.0.71), reading the existing `mkdocs.yml`
with the classic theme variant so the site looks the same. Accepted risk:
Zensical is pre-1.0 and releases often; the pin and Dependabot keep upgrades
deliberate, and CI's strict build catches regressions.

**Evidence.** The unchanged configuration builds in strict mode with no
warnings in about a second. Both builds produce the same 21 pages, with the
API reference, the `SECURITY.md` and `CHANGELOG.md` includes and every
decision record present. Strict mode fails the build on a broken internal
link and on a missing snippet file, as the MkDocs build did.
