"""Order pricing: discounts, shipping and totals."""

FREE_SHIPPING_THRESHOLD = 50.0
STANDARD_SHIPPING = 4.99


def apply_discount(subtotal, percent):
    """Apply a percentage discount, given as 0-100."""
    if not 0 <= percent <= 100:
        raise ValueError("percent must be between 0 and 100")
    return subtotal - subtotal * percent / 100


def shipping_cost(subtotal):
    return 0.0 if subtotal >= FREE_SHIPPING_THRESHOLD else STANDARD_SHIPPING


def order_total(lines, discount_percent=0):
    """Total for (quantity, unit_price) lines after discount, plus shipping."""
    subtotal = sum(qty * price for qty, price in lines)
    discounted = apply_discount(subtotal, discount_percent)
    total = discounted + shipping_cost(discounted)
    if total < 0:
        raise ValueError("total cannot be negative")
    return round(total, 2)
