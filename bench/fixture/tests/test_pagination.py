import unittest

from shop.pagination import page_count, paginate

ITEMS = list(range(1, 11))  # 1..10


class PaginationTest(unittest.TestCase):
    def test_first_page(self):
        self.assertEqual(paginate(ITEMS, 1, 3), [1, 2, 3])

    def test_middle_page(self):
        self.assertEqual(paginate(ITEMS, 2, 3), [4, 5, 6])

    def test_last_partial_page(self):
        self.assertEqual(paginate(ITEMS, 4, 3), [10])

    def test_past_the_end_is_empty(self):
        self.assertEqual(paginate(ITEMS, 5, 3), [])

    def test_page_count(self):
        self.assertEqual(page_count(10, 3), 4)
        self.assertEqual(page_count(0, 3), 1)

    def test_invalid_arguments(self):
        with self.assertRaises(ValueError):
            paginate(ITEMS, 0, 3)
        with self.assertRaises(ValueError):
            paginate(ITEMS, 1, 0)
