import unittest

from shop.users import display_name, normalize_email, validate_user


class UsersTest(unittest.TestCase):
    def test_valid_user(self):
        self.assertEqual(validate_user({"name": "Ada", "email": "ada@example.com"}), [])

    def test_invalid_user(self):
        errors = validate_user({"name": " ", "email": "nope", "age": 7})
        self.assertEqual(len(errors), 3)

    def test_normalize_email(self):
        self.assertEqual(normalize_email("  Ada@Example.COM "), "ada@example.com")

    def test_display_name(self):
        self.assertEqual(display_name({"name": "Ada Lovelace"}), "Ada L.")
        self.assertEqual(display_name({"name": "Plato"}), "Plato")
