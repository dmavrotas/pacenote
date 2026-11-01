import unittest

from shop.inventory import Inventory, OutOfStock


class InventoryTest(unittest.TestCase):
    def test_add_and_remove(self):
        inv = Inventory()
        inv.add("A", 5)
        inv.remove("A", 2)
        self.assertEqual(inv.available("A"), 3)

    def test_out_of_stock(self):
        inv = Inventory()
        inv.add("A", 1)
        with self.assertRaises(OutOfStock):
            inv.remove("A", 2)

    def test_low_stock(self):
        inv = Inventory()
        inv.add("B", 2)
        inv.add("A", 10)
        inv.add("C", 1)
        self.assertEqual(inv.low_stock(), ["B", "C"])
