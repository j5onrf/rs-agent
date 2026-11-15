import unittest

from calc import add, divide


class TestAdd(unittest.TestCase):
    def test_positive(self):
        self.assertEqual(add(2, 3), 5)

    def test_negative(self):
        self.assertEqual(add(-2, -3), -5)

    def test_floats(self):
        self.assertAlmostEqual(add(0.1, 0.2), 0.3)

    def test_zero(self):
        self.assertEqual(add(0, 0), 0)


class TestDivide(unittest.TestCase):
    def test_exact(self):
        self.assertEqual(divide(6, 3), 2)

    def test_float_result(self):
        self.assertAlmostEqual(divide(1, 4), 0.25)

    def test_negative(self):
        self.assertEqual(divide(-6, 3), -2)

    def test_zero_denominator_raises(self):
        with self.assertRaises(ValueError):
            divide(1, 0)

    def test_zero_numerator(self):
        self.assertEqual(divide(0, 5), 0)


if __name__ == "__main__":
    unittest.main()
