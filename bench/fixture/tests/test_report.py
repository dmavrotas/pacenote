import unittest

from shop.report import revenue_by_day, top_products

ORDERS = [
    {"date": "2026-10-02", "total": 20.0, "items": [("A", 1), ("B", 2)]},
    {"date": "2026-10-01", "total": 15.5, "items": [("B", 1)]},
    {"date": "2026-10-02", "total": 4.5, "items": [("C", 5)]},
]


class ReportTest(unittest.TestCase):
    def test_revenue_by_day(self):
        self.assertEqual(revenue_by_day(ORDERS), {"2026-10-01": 15.5, "2026-10-02": 24.5})

    def test_top_products(self):
        self.assertEqual(top_products(ORDERS, 2), ["C", "B"])
