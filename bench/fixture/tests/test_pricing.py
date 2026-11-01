import unittest

from shop.pricing import apply_discount, order_total, shipping_cost


class PricingTest(unittest.TestCase):
    def test_no_discount(self):
        self.assertEqual(apply_discount(80.0, 0), 80.0)

    def test_percentage_discount(self):
        self.assertAlmostEqual(apply_discount(80.0, 25), 60.0)

    def test_order_total_with_discount(self):
        self.assertEqual(order_total([(2, 30.0)], discount_percent=10), 54.0)

    def test_small_order_pays_shipping(self):
        self.assertEqual(order_total([(1, 10.0)]), 14.99)

    def test_shipping_threshold(self):
        self.assertEqual(shipping_cost(50.0), 0.0)

    def test_invalid_percent(self):
        with self.assertRaises(ValueError):
            apply_discount(10.0, 120)
