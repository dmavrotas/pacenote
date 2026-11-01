"""Sales reporting."""

from collections import defaultdict


def revenue_by_day(orders):
    """Sum order totals per ISO date, sorted by date."""
    totals = defaultdict(float)
    for order in orders:
        totals[order["date"]] += order["total"]
    return dict(sorted(totals.items()))


def top_products(orders, n=3):
    counts = defaultdict(int)
    for order in orders:
        for sku, qty in order["items"]:
            counts[sku] += qty
    return [sku for sku, _ in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:n]]
