# Security policy

doormat reads spreadsheets that may come from untrusted sources and reports the
credentials it finds in them. Both sides of that matter: the library must be
safe to run on hostile files, and it must not leak the secrets it finds.

## Supported versions

| Version | Supported |
|---|---|
| 0.2.x | yes |
| older | no |

doormat is pre-1.0. Fixes are released on the latest minor version only.

## Reporting a vulnerability

**Do not open a public issue.** Report privately through GitHub:
[Security > Report a vulnerability](https://github.com/dipan010/doormat/security/advisories/new).

Please include:

- the doormat version (`doormat.__version__`), Python version and OS
- what happens and what you expected
- a way to reproduce it, ideally a small spreadsheet built from made-up values

You can expect an acknowledgement within 7 days and a fix or a decision within
90 days. You will be credited in the advisory unless you ask otherwise.

## What counts as a vulnerability

- a crafted spreadsheet that crashes the process, hangs it, or exhausts memory
  (beyond the cost of reading a genuinely large file)
- memory unsafety in the Rust core or the Python extension
- a finding's credential value appearing somewhere the API does not put it,
  such as `repr()`, exception messages or warnings
- a vulnerable dependency that doormat actually exercises

Missed credentials and false alarms are detection-quality issues, not
vulnerabilities. Report those as a regular
[detection issue](https://github.com/dipan010/doormat/issues/new/choose).

## Never share real secrets

Do not attach spreadsheets, logs or screenshots that contain real credentials,
in a report or anywhere else in this project. Rebuild the layout with made-up
values instead; doormat's behaviour depends on where cells sit and what they
look like, not on whether a password is real.
