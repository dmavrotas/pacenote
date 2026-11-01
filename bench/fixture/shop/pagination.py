"""Pagination helpers for listing endpoints."""

from math import ceil


def page_count(total, size):
    """Number of pages needed to show `total` items, `size` per page."""
    if size <= 0:
        raise ValueError("size must be positive")
    return max(1, ceil(total / size))


def paginate(items, page, size):
    """Return the items on 1-indexed `page`, `size` items per page."""
    if page < 1:
        raise ValueError("page must be >= 1")
    if size <= 0:
        raise ValueError("size must be positive")
    start = (page - 1) * size + 1
    return items[start:start + size]
