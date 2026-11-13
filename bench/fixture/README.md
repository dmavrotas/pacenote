# shop (benchmark fixture)

A tiny Python package used by pacenote's end-to-end benchmark. All tests pass
as committed; each benchmark task injects its own bug (see
`examples/e2e/tasks.rs`) into a fresh copy, so tasks are independent and the
other modules give a vague prompt something to search through.

Run the tests with `python3 -m unittest` (standard library only).
