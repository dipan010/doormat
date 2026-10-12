# API reference

doormat's public API is four names: `load()`, which reads and scans a file, and
the frozen dataclasses it returns. Everything else in the package is internal.

::: doormat.load

::: doormat.GridDoc
    options:
      members: [relationships, credentials, complete]

::: doormat.Relationship
    options:
      members: [coordinate, header_coordinate]

::: doormat.SheetError
