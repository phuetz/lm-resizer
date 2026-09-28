"""A deliberately faulty order validator for the end-to-end task."""


def status_for_quantity(quantity: int) -> int:
    if quantity < 0:
        return 422
    return 200
