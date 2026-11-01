"""User records and validation."""

import re

EMAIL_RE = re.compile(r"^[^@\s]+@[^@\s]+\.[a-z]{2,}$", re.IGNORECASE)


def normalize_email(email):
    return email.strip().lower()


def validate_user(user):
    """Return a list of validation errors for a user dict."""
    errors = []
    if not user.get("name", "").strip():
        errors.append("name is required")
    if not EMAIL_RE.match(user.get("email", "")):
        errors.append("email is invalid")
    if user.get("age") is not None and not 13 <= user["age"] <= 130:
        errors.append("age is out of range")
    return errors


def display_name(user):
    first, _, last = user["name"].strip().partition(" ")
    return f"{first} {last[:1]}." if last else first
