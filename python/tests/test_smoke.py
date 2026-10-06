from doormat import version


def test_version_returns_string():
    v = version()
    assert isinstance(v, str)
    assert len(v) > 0
