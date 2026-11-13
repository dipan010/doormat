# 0022: Spaced password labels, password columns without an account column, and two everyday keyword uses

**Status:** accepted, 2026-11-14.

**Context.** The lab's second test (ADR 0018, "Outcome") found three gaps in
the pipeline, reproduced since on synthetic workbooks:

- A header is a password label only when its whole text is a keyword or it is
  an identifier-style compound (`routerPassword`). `Router Password 24`, with
  spaces, is neither, so a 210-row two-column password table under such
  headers gave no finding at all (211 of the test's 281 secrets).
- A credential table needs an account label in its header row. That table
  had none, so even with its headers recognised it would give one `distance`
  finding, not one per row.
- `inline_same_cell` reports `... Pin : 560001` in addresses and `PWD 250GM`
  (powder) in product lists at 250: 15 of the pipeline's 37 false positives on
  the lab's test split.

A fourth candidate, `PW` as a keyword (a 6-row `PW` column was missed), was
tried and dropped: on Enron and set B, `PW` is mostly power or a code (`_PW`
column suffixes, a `PW` split pair over trade codes, `PW` over three-letter
codes), adding about 20 false positives and no credential.

**Options.** Loosen the keyword match to any label containing a password
word (prose such as `Password reset policy` would pair with its neighbours);
learn the label rule (the lab's learned scorer leaned on the pipeline's own
confidence and did not recover these, ADR 0018); or add narrow, word-level
rules and measure each on the corpora.

**Decision.**

- **Spaced labels** (`is_spaced_credential_label`, `features.rs`): at most 4
  words after a trailing number, `#`, `#n` or `(n)` is set aside; the last word
  in `SPACED_LABEL_HEADS` (`password`, `passwd`, `passphrase`, `passcode` and
  the non-English password words; not `pass`, `pwd`, `pin`, `secret`, `key`,
  `token`); no word of `SPACED_LABEL_BLOCKERS` before it (`forgot`, `reset`,
  `change`, `enter`, `with`, `your`, ...); no numbered step, no trailing `..`,
  only letters, digits, spaces and `_ . - # ( )`. Such a label sets
  `FLAG_IS_PASSWORD_HEADER`. It never heads a split value
  (`detect_split_password`): the one split value it produced on Enron was a
  row of a list (`Synergi 2nd password` in a column of board names) paired
  with the next rows.
- **Password columns** (`detect_password_column`, `inference.rs`): a password
  label that heads no credential table, is not ambiguous, is not a variable
  name and is not a form field (no trailing `:`, no credential value to its
  right) heads a column of at least 5 credential values
  (`PASSWORD_COLUMN_MIN_ROWS`; not a date, URL, e-mail address or quantity),
  with 80% single tokens and at least half of the values distinct. Each is
  reported at 150 (`PASSWORD_COLUMN_CONFIDENCE`), reason `password_column`,
  pathway `table` in baselines. A column repeating one default password does
  not qualify.
- **Everyday keyword uses** (`detect_inline_credentials`): no inline finding
  when the value is a quantity with a unit (`is_quantity`, `QUANTITY_UNITS`),
  or when the keyword is `pin` and the value a six-digit postal code written
  `999 999` or after text holding a comma (`is_postal_pin`).

**Evidence.** Before and after, `doormat scan --min-confidence 0` over each
corpus:

| Corpus | Files | Findings before | after | Change |
|---|---|---|---|---|
| Enron | 15,929 | 322 | 322 | none |
| Set B (Fuse volumes 20-34) | 36,439 | 85 | 83 | 2 removed: `... Pwd` product lines with a quantity, not in the answer key |
| Set A (exports) | 192 | 2,231 | 2,231 | none |
| Set A2 (configs) | 396 | 708 | 708 | none |

The fixture harness stays at precision and recall 1.00.
`cargo bench --bench pipeline` (`process_workbook_10x5k`), new, old, new in
one session: 2.67, 2.88, 2.84 ms. A first version without the form-field
guard took about 20% longer: the benchmark's `Password | value` row sits over
a column of data, which it reported as a password column; the guard removes
both the cost and those findings. On synthetic
workbooks (fake values): `Router Password 24` over two columns of 210 rows
gives 420 findings (none before); `Username | Router Password` 210 (none
before); a `Password` column with no account column 210 (1 before); the same
column repeating one value 1 (unchanged); the postal and powder lines 0 (1
each before); `Forgot Password` over values 0.

None of the corpora holds a spaced-label or account-less credential table,
so the gain is shown only on synthetic workbooks and the lab's test, whose
files are deleted (lab amendment 11); the lab's third round measures it on
new data.

**Consequences.** Findings that were missed can now appear at 150 or above
in spreadsheets with spaced password headers. The blocker list is short and
chosen from the false positives seen here; a label it misses (`Password Hint
Answer`) is a prose case to add from evidence, not by guesswork. `PW` stays
unrecognised.
