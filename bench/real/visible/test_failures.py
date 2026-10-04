def test_success():
    assert 2 + 2 == 4


def test_invoice_total():
    assert 12030 == 12031


def test_config_path():
    assert '~/.config/app' == '~/.config/other'
