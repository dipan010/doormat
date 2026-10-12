# File formats

| Format | Extensions | Formulas | Comments | Extra |
|---|---|---|---|---|
| Excel (OOXML) | `.xlsx`, `.xlsm` | yes | yes | none |
| Excel 97-2003 | `.xls` | no | no | `doormat[xls]` |
| OpenDocument | `.ods` | yes | yes | `doormat[ods]` |
| Delimited text | `.csv`, `.tsv` | n/a | n/a | none |

- The reader is chosen by extension, and binary formats are checked against
  their signature (magic bytes) before parsing. A mismatch raises `ValueError`.
- CSV and TSV files are one sheet named after the file (`env.csv` gives sheet
  `env`).
- Hidden and very hidden sheets are scanned; their findings have
  `hidden=True`.
- Rows and columns are 1-based, as in the spreadsheet itself.
- `.xls` files expose no formulas or comments through xlrd, so the formula and
  comment pathways do not apply to them. Whole numbers are reported without a
  trailing `.0`.
