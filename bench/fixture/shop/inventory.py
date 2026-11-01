"""In-memory stock tracking."""


class OutOfStock(Exception):
    pass


class Inventory:
    def __init__(self):
        self._stock = {}

    def add(self, sku, quantity):
        if quantity <= 0:
            raise ValueError("quantity must be positive")
        self._stock[sku] = self._stock.get(sku, 0) + quantity

    def remove(self, sku, quantity):
        available = self._stock.get(sku, 0)
        if quantity > available:
            raise OutOfStock(f"{sku}: requested {quantity}, have {available}")
        self._stock[sku] = available - quantity

    def available(self, sku):
        return self._stock.get(sku, 0)

    def low_stock(self, threshold=5):
        return sorted(sku for sku, qty in self._stock.items() if qty < threshold)
