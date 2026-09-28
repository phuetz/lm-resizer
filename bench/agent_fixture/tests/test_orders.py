from orders import status_for_quantity


def test_accept_positive():
    assert status_for_quantity(3) == 200


def test_reject_negative():
    assert status_for_quantity(-2) == 422


def test_reject_zero():
    actual = status_for_quantity(0)
    assert actual == 422, f"expected status=422, got status={actual}"
