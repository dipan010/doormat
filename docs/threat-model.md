# Threat model

doormat reads spreadsheets that may come from anyone and reports the secrets it
finds. This page states what it protects, against whom, how, and what it does
not protect. It describes version 0.2.

## Assets

1. **The host process.** doormat runs inside your Python process, often in a
   scanner that processes many files. A file must not be able to crash it,
   hang it, exhaust its memory or run code in it.
2. **The secrets it finds.** A finding contains a live credential. doormat must
   not spread it further than the caller asked.
3. **The integrity of results.** A scan is used to make decisions (block a
   commit, open an incident). Results must be correct about *where* a secret is.

## Adversary

The main adversary is the **author of a spreadsheet** being scanned. They
control every byte of the file: zip structure, XML, cell contents, declared
sizes and ranges. They may want to disrupt the scanner (denial of service) or
hide a secret from it (evasion).

doormat does not defend against an adversary who controls the Python
environment, the doormat package itself, or its dependencies; those are
supply-chain risks covered by the project's dependency policy instead.

## Parser surface by format

| Format | Parsed by | Container | Notes |
|---|---|---|---|
| `.xlsx`, `.xlsm` | openpyxl | zip | XML parsed through `defusedxml` |
| `.ods` | odfpy | zip | odfpy parses XML through `defusedxml` itself |
| `.xls` | xlrd | OLE2 compound file | binary, no XML |
| `.csv`, `.tsv` | Python `csv` | plain text | no container, no XML |

doormat's own detection code is Rust with no `unsafe` in the bindings; it
receives already-extracted cell text.

## Attacks and mitigations

| Attack | Mitigation | Status |
|---|---|---|
| XML entity expansion ("billion laughs"), external entities (XXE) | `defusedxml` refuses entity declarations in `.xlsx` and `.ods` XML. A worksheet that declares entities is skipped and listed in `GridDoc.errors`; anywhere else `load()` raises `ValueError` | Mitigated, tested |
| Oversized workbook (millions of real cells) | Cells with content are counted while each sheet is parsed (cells holding only a style are removed first); past `max_cells` (default 2,000,000, about 1 GiB) `load()` raises `ValueError` | Mitigated, tested |
| Zip bomb (tiny file, huge decompressed parts) | Before parsing, the zip directory is checked: more than 10,000 parts, more than 512 MiB declared (configurable), or a part of 10 MiB or more compressed beyond 500:1 is refused. Parts are read through Python's `zipfile`, which never returns more than a part's declared size, so understated sizes fail instead of bypassing the check | Mitigated, tested with real bombs and forged sizes |
| Range inflation (one formatted cell at `XFD1048576`, about 17 billion positions) | The `.xlsx` extractor visits only cells present in the file, never the declared range | Mitigated, tested |
| Slow pre-parse filtering (a cell tag packed with ` r='`, or a huge text node) | The `.xlsx` filter that removes style-only cells bounds every span of its pattern at 64 bytes and passes long text through, so its cost is linear in the sheet size | Mitigated, tested |
| Stream amplification in `.xls` (sectors reused to build a workbook stream larger than the file) | xlrd's strict sector check is relaxed so real files with shared sectors can be read; a workbook stream declared larger than the file is refused before parsing | Mitigated, tested |
| Repeat inflation in `.ods` (a cell "repeated" a billion times) | Repeated empty cells are skipped; repeated values are capped at 16,384 columns and rows at 1,000 repetitions | Mitigated, tested |
| Secret leaking into logs | `repr(Relationship)` omits the value; doormat logs nothing | Mitigated |
| A malformed sheet hides the rest of the workbook | Each sheet is parsed separately; one that fails is listed in `GridDoc.errors` and the others are still scanned | Mitigated, tested |
| Wrong location reported | Findings carry sheet, row and column from the file; regressions are tested (e.g. repeated ODS rows) | Mitigated |
| Evasion: secret with no label near it | Not detectable by design; doormat finds labelled secrets | Not mitigated |
| Evasion: label in a language or form doormat does not know | Keyword lists are finite | Partially mitigated |

## Residual risks

- **No time limit.** doormat has no per-file deadline. The cell limit bounds
  the work per file (about a minute at the default), but a file within it can
  still be slow, and the `.ods` reader parses its whole XML before counting.
  Run large-scale scans with a process-level timeout and memory limit.
- **Third-party parsers.** openpyxl, odfpy and xlrd parse untrusted input in
  pure Python. A bug in them is a bug in doormat's attack surface; Dependabot
  and `pip-audit` watch for published advisories.
- **A sheet that cannot be parsed is not scanned.** `load()` reports it in
  `GridDoc.errors` and scans the other sheets, so its secrets are missed.
  Callers that need an exhaustive scan must check `GridDoc.complete`. An
  `.ods` file keeps all sheets in one XML document, so damage there still
  fails the whole file.

## Recommendations for deployment

- Treat findings as secrets: store and transmit them as you would the
  credentials themselves, and avoid printing `Relationship.value`.
- Scan untrusted files in a separate process with a timeout and memory limit.
- Report suspected vulnerabilities privately, as described in the
  [security policy](security.md).
