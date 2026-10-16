# 0011: Compound credential labels by their final part

**Status:** accepted, 2026-10-16

**Context.** Labels were matched as whole keywords, so `login_password`,
`MYSQL_ROOT_PASSWORD` and `RABBITMQ_DEFAULT_PASS` were not credential labels
(#19). Set A's test split found 0 of 465 Bitwarden passwords.

**Options.** Add the names seen in exports and documentation to the keyword
list; match any label that contains a credential word; or recognise a label
by its final part and judge the value as well.

**Decision.** An identifier-style label is a credential label when its final
part is a credential word (`..._PASSWORD`, `..._PASS`, `...ADMINPASSWORD`,
`...COOKIESECRET`). A credential word followed by another part names
something else (`_FILE`, `_HASH`, `_PATH`). In lists of configuration
variables a label's value is the cell across from it, never a neighbour's.
`..._KEY`/`..._SALT` labels need key-material values. Switches and amounts
are never credentials. No specific variable names were added: the evaluation
protocol forbade it, because names recur across its splits.

**Evidence.** Evaluation set A2, split by image family before any result:
design 25 -> 407/407, holdout 219/219, test 81/81 with 98.8% precision
(12/81 before). Enron: no finding added or lost. An interim version cost
12-17% in `process_sheet`; doing the credential-word scan once per cell as a
flag brought that to about 3-4%.
