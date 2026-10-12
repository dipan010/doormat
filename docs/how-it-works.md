# How detection works

doormat treats each sheet as a 2D grid and looks for credential *labels*
(`Password`, `pwd`, `contraseña`, ...) and the *values* around them. This page
describes the algorithm as implemented in `crates/doormat-core`; every number
below is a constant in that code, cited by file so it can be checked and
changed deliberately.

The pipeline runs once per sheet, and sheets run in parallel (Rayon). Python
only reads the file; everything below happens in Rust.

```
cells ─► store ─► features ─► candidates ─► classify ─► regions
                                                          │
          findings ◄── locate + sort ◄── dedup ◄── detect + pair
```

## 1. Store (`store.rs`)

Every non-empty cell becomes a row in a columnar store: coordinates, value,
formula, comment, sheet name and a merged-cell flag. A `(row, col)` index over
**all** cells lets later stages look up any neighbour in constant time. A cell
supplied twice keeps its first value and appends the later comment.

## 2. Features (`features.rs`)

Each value is normalised once: lower-cased, with spaces, tabs, newlines and
`_ - : =` removed. From it doormat derives bit flags:

| Flag | Meaning |
|---|---|
| upper, lower, digit, special | character classes present |
| short, numeric, alpha | shape of the value |
| colon | the raw value contains `:` |
| formula, comment | the cell has a formula or a comment |
| password / username / URL header | the normalised value **exactly** equals a keyword |

Header keywords are matched with Aho-Corasick against whole normalised values,
so `Password:` and `PASS_WORD` are password labels but `Password reset policy`
is not. The password list covers English (`password`, `pwd`, `pass`, `passwd`,
`passphrase`, `passcode`, `secret`, `secretkey`, `apikey`, `token`,
`accesstoken`, `authtoken`, ...) and German, Spanish, French, Portuguese,
Russian, Japanese, Chinese, Korean, Dutch, Polish and Swedish forms. Username
and URL lists are separate and only add context.

Shannon entropy is computed only for values of at least **8** characters
(`ENTROPY_MIN_LENGTH`, `types.rs`); shorter values skip the floating-point work.

## 3. Candidates (`candidates.rs`)

A cell becomes a candidate if any of these holds:

1. it is a password label;
2. it has a formula;
3. it has a comment;
4. its entropy is at least **3.5** bits per character (`ENTROPY_THRESHOLD`);
5. it matches the inline pattern (step 6).

Candidates that are labels are classified as **headers**. A candidate in a
sparse row (two cells or fewer), at most 60 characters, more than half
letters and without `= @ # $ % ^ & * { } [ ]`, is a **section title**. Every
other cell is a potential **value**.

## 4. Regions (`regions.rs`)

Candidates are clustered by breadth-first search: two candidates belong to the
same region if one lies within the other's neighbourhood (step 7). Region
membership is a scoring signal: a label and a value in the same cluster are
more likely to belong together.

## 5. Inline detection (`detection.rs`)

Credentials written inside one cell are matched by a regular expression
(`INLINE_CREDENTIAL_REGEX`):

- a credential keyword (`password`, `pwd`, `pass`, `passwd`, `secret`, `pin`,
  `passwort`, `contraseña`, `motdepasse`, `senha` or `пароль`) that is **not
  preceded by a letter or digit**, so `spin`, `compass` and `humpass` never
  match while `DB_PASSWORD=` and `app.pwd:` do. Japanese, Chinese and Korean
  keywords are recognised as labels (step 2) but not inline;
- then a separator: `:` or `=`, or a dash with whitespace on at least one side
  (`Password - X`). Plain whitespace is not a separator, which excludes prose
  such as "cost pass through";
- or, for password-family keywords only, whitespace followed by a single token
  containing a digit (`Password X333#`).

Formula cells are skipped here; their text is a formula, handled in step 6.

| Pathway | Reason string | Confidence |
|---|---|---|
| inline value | `inline_same_cell` | 250 |
| formula: concatenated strings form `password: x` | `formula_concatenated` | 240 |
| formula: a keyword string followed by a value string | `formula_split_strings` | 230 |
| comment on a password label | `comment_on_header_cell` | 230 |
| comment containing `password: x` | `comment_contains_credential` | 220 |
| value split down a column under a label | `split_password` | 200 |

## 6. Split values (`inference.rs`)

Before pairing, doormat checks whether a password label sits above a value
split across 2 to 5 cells in the same column. Each fragment must be a value,
under 30 characters, not end in `:`, and have **no horizontal neighbour**: a
cell with something beside it is a form row or a table row, not a fragment.
The joined value must be at least 6 characters.

## 7. Pairing and scoring (`spatial.rs`, `scoring.rs`, `inference.rs`)

For each password label, every value cell within **3** rows and columns
(`NEIGHBOR_RADIUS`, a 7×7 window) is scored, except formula cells and values
already claimed by another label. The best score wins if it reaches **120**
(`RELATIONSHIP_THRESHOLD`).

**Distance**, by offset of the value from the label (`spatial.rs`):

| Offset | Score |
|---|---|
| right, (0, +1) | 100 |
| below, (+1, 0) | 95 |
| left, (0, −1) | 80 |
| above, or any diagonal neighbour | 70 |
| same row or column, *n* cells away (*n* ≥ 2) | 90 − 15 × (*n* − 1), floor 0 |
| anything else | 60 − 10 × Euclidean distance, floor 0 |

**Content bonuses and penalties** (`scoring.rs`):

| Signal | Points |
|---|---|
| length ≥ 8 | +20 |
| each of upper, lower, digit, special present | +10 each |
| entropy > 3.5 | +25 |
| label and value in the same region | +30 |
| a username label within 3 cells of the value | +30 |
| a URL label within 3 cells of the value | +20 |
| the value is itself a password or username label | −80 |
| the value ends in `:` (a form label) | −80 |
| the value contains whitespace | −40 |
| the value is shorter than 4 characters (`MIN_CANDIDATE_LENGTH`) | −50 |

Every finding carries its `reason` string, the `;`-separated list of the terms
that produced its score, so a result can always be explained.

## 8. Deduplication, location and order (`pipeline.rs`)

Two findings are duplicates only if they report the same value in the same
cell (for example the inline and comment pathways on one cell); the higher
confidence wins. The same secret in two cells, or on two sheets, is two
findings. Each finding is then resolved to its sheet, row and column, and the
list is returned in reading order.

## Known limits of the approach

- **Tables are not modelled.** A `Password` column with one credential per row
  yields at most one pairing. This is the largest recall gap measured
  ([evaluation](evaluation.md)).
- **An empty value cell does not stop pairing.** A form label whose value is
  blank can pair with the next label below it.
- **Scores are hand-set, not learned.** The constants above were chosen on
  synthetic fixtures and then corrected against the dev split of the Enron
  corpus; they are not fitted by a model.
EOF