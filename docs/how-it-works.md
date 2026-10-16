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
`accesstoken`, `authtoken`, and the TOTP secret labels `totp` and
`otpsecret`, ...) and German, Spanish, French, Portuguese,
Russian, Japanese, Chinese, Korean, Dutch, Polish and Swedish forms. Username
and URL lists are separate and only add context.

**Compound labels.** An identifier-style label (letters, digits and `_ . -`,
no spaces, at most **64** characters; `COMPOUND_LABEL_MAX_LEN`) is also a
password label when its final part, split at `_ . -` and camelCase, is a
password keyword or ends in `password`, `passwd`, `passphrase`, `passcode`,
`secret` or `token` (`COMPOUND_SUFFIXES`): `MYSQL_ROOT_PASSWORD`,
`login_password`, `mail__options__auth__pass`,
`ME_CONFIG_MONGODB_ADMINPASSWORD`. A credential word followed by another part
is something else (`POSTGRES_PASSWORD_FILE` is a path,
`MARIADB_ROOT_PASSWORD_HASH` a hash), and short keywords must be a whole part,
so `bypass` never matches. Labels whose final part is `key` or `salt`
(`KEY_PARTS`, `WORDPRESS_AUTH_KEY`) are weaker: they count only in lists of
configuration variables (step 8) and only for values that look like key
material (at least **16** characters mixing upper case, lower case and digits,
entropy at least **3.5**; `KEY_MATERIAL_MIN_LEN`). Labels with spaces
(`Admin Password`) are matched as before.

Shannon entropy is computed only for values of at least **8** characters
(`ENTROPY_MIN_LENGTH`, `types.rs`); shorter values skip the floating-point work.

## 3. Candidates (`candidates.rs`)

A cell becomes a candidate if any of these holds:

1. it is a password label;
2. it has a formula;
3. it has a comment;
4. its entropy is at least **3.5** bits per character (`ENTROPY_THRESHOLD`);
5. it matches the inline pattern (step 5).

Candidates that are labels are classified as **headers**. A candidate in a
sparse row (two cells or fewer), at most 60 characters, more than half
letters and without `= @ # $ % ^ & * { } [ ]`, is a **section title**. Every
other cell is a potential **value**.

## 4. Regions (`regions.rs`)

Candidates are clustered by breadth-first search: two candidates belong to the
same region if one lies within the other's neighbourhood (step 8). Region
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
  containing a digit (`Password X333#`);
- in a URL query (`otpauth://totp/ada?secret=X&issuer=Y`), the value ends at
  the next `&` or `#`;
- an assignment `NAME=value` or `NAME: value` whose name is a compound label
  (`ME_CONFIG_MONGODB_ADMINPASSWORD=x`, `WORDPRESS_AUTH_KEY=x` with key
  material) yields its value.

A switch (`yes`, `no`, `true`, `false`, `on`, `off`, `y`, `n`, `0`, `1`;
`BOOLEAN_LITERALS`) is never a credential value, so
`MYSQL_ALLOW_EMPTY_PASSWORD=true` is not reported.

Formula cells are skipped here; their text is a formula, handled by the
formula pathways in the table below.

| Pathway | Reason string | Confidence |
|---|---|---|
| inline value | `inline_same_cell` | 250 |
| formula: concatenated strings form `password: x` | `formula_concatenated` | 240 |
| formula: a keyword string followed by a value string | `formula_split_strings` | 230 |
| comment on a password label | `comment_on_header_cell` | 230 |
| comment containing `password: x` | `comment_contains_credential` | 220 |
| value split down a column under a label | `split_password` | 200 |
| one row of a credential table (step 6) | `table_column` | 190 |
| the value of a configuration variable (step 8) | `variable_value` | 190 |

## 6. Credential tables (`inference.rs`)

Some workbooks list many accounts, one per row, under a header row such as
`System | User ID | Password`; password-manager exports look the same. A
password label heads a **credential table** when:

- an **account label** sits in the **same row**, at most **10** columns away
  (`TABLE_HEADER_REACH`): a username label (`User ID`, `Username`, `Login`,
  `Account`, ...) or, for tables only, a bare `ID`, `UID`, `Login ID` or
  `Logon ID` (`TABLE_USERNAME_LABELS`). The nearest one is the **account
  column**;
- the table's rows run down while the account cell or the secret cell is
  filled, and end at a new label row or after more than **1** row with both
  empty (`TABLE_MAX_GAP`; accounts are often separated by a blank row). Rows
  without a secret are allowed: an account with no password, or a TOTP column
  that is mostly empty;
- there are at least **2** rows (`TABLE_MIN_ROWS`) and at least one secret, and
  at least **80%** of the secrets are single tokens (`TABLE_TOKEN_SHARE`): no
  whitespace, no trailing `:`, not a label and not a formula.

The whole column is judged at once, so a help-desk log whose `Password` column
holds categories, a column of dates or notes, or a login form whose next rows
are labels does not qualify. Every token of at least **3** characters
(`TABLE_MIN_VALUE_LEN`) with a letter or digit is then reported, one finding
per row; placeholders from the labelling rubric (`N/A`, `TBD`, `none`, `?`,
`xxxx`, `****`; `TABLE_PLACEHOLDERS`) are skipped. The header is not paired
again by the steps below.

A password label that occurs **3** or more times in its own column
(`CATEGORY_REPEAT`) is a category value in a data column (a ticket log's
`Category: Password`). It may still head a credential table, since stacked
tables repeat their header row, but it is not paired by the steps below.

## 7. Split values (`inference.rs`)

Before pairing, doormat checks whether a password label sits above a value
split across 2 to 5 cells in the same column. Each fragment must be a value,
under 30 characters, not end in `:`, and have **no horizontal neighbour**: a
cell with something beside it is a form row or a table row, not a fragment.
The joined value must be at least 6 characters.

## 8. Pairing and scoring (`spatial.rs`, `scoring.rs`, `inference.rs`)

**Configuration variables.** A compound label in a list of configuration
variables takes its value from across the list, never from a neighbouring
variable. If more of its vertical neighbours than horizontal ones are variable
names (two or more `_`/`.` parts, starting with a letter), the names run down a
column and the value is the cell to the label's right. If more horizontal
neighbours are names, they form a header row and the values are the column
below (one row per environment), down to a label or more than **1** empty row.
Values must be single tokens of at least **3** characters that are not
placeholders, switches or amounts (a signed or decimal number such as
`-12345.67`); each is reported with reason `variable_value` and confidence
**190** (`VARIABLE_VALUE_CONFIDENCE`).

A password label ending in `:` is a form field: its value is to the right.
If none of the **2** cells to its right is filled (`FORM_FIELD_REACH`; two
allow for a merged label cell), the field is empty and nothing is paired, so
the label below it is never reported as its value.

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

## 9. Deduplication, location and order (`pipeline.rs`)

Two findings are duplicates only if they report the same value in the same
cell (for example the inline and comment pathways on one cell); the higher
confidence wins. The same secret in two cells, or on two sheets, is two
findings. Each finding is then resolved to its sheet, row and column, and the
list is returned in reading order.

## Known limits of the approach

- **Credential tables need a username column.** A `Password` column is read as
  a table only when a username or ID label shares its header row. Tables
  without one (an export configuration listing `Path | Password`), or laid out
  in ways the rule above rejects, still yield at most one pairing. On the
  corpus this rule finds about half of the table credentials
  ([evaluation](evaluation.md)).
- **Unlabelled value columns are not modelled.** A form label without a colon
  (`Password`) whose value cell is empty can still pair with a nearby cell.
- **Scores are hand-set, not learned.** The constants above were chosen on
  synthetic fixtures and then corrected against the dev split of the Enron
  corpus; they are not fitted by a model.
