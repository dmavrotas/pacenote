# shop (benchmark fixture)

A tiny Python package used by pacenote's end-to-end benchmark. It contains two
deliberate bugs (`shop/pagination.py`, `shop/pricing.py`); the other modules are
correct and exist so that a vague prompt has something to search through.

Run the tests with `python3 -m unittest` (standard library only).
