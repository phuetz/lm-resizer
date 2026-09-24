require 'minitest/autorun'

class TestMath < Minitest::Test
  def test_addition
    assert_equal 3, 1 + 1
  end

  def test_subtraction
    assert_equal 2, 2 - 1
  end
end
