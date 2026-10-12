# doormat

*Find the passwords people leave next to the label that says "Password".*

People hide the spare key under the doormat. They hide passwords the same way:
in the spreadsheet cell right next to `Password:`, in a cell that reads
`pwd = hunter2`, or inside a database connection string. **doormat** reads
`.xlsx`, `.xls`, `.ods`, `.csv` and `.tsv` files, models each sheet as a 2D
grid, and pairs credential labels with the values around them.

- **Spatial, not just regex.** Scores every candidate value by its distance and
  direction from a credential label, its character mix and entropy, and nearby
  username or URL labels. See [how detection works](how-it-works.md).
- **Finds what grep misses.** Credentials split across cells, built with
  `CONCAT` in formulas, or tucked into cell comments.
- **Safe on hostile files.** XML entity attacks and zip bombs are refused, and
  inflated sheet ranges cannot hang it. See the [threat model](threat-model.md).
- **Fast.** A Rust core processes a 5,000-cell sheet in under a millisecond.
- **Measured on real data.** Precision and recall on the public Enron
  spreadsheet corpus are published with their caveats. See
  [evaluation](evaluation.md).

```bash
pip install doormat
```

Continue with [getting started](getting-started.md).
