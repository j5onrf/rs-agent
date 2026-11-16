#!/usr/bin/env python3
"""Simple math utilities: factorial and fibonacci with tests."""

import unittest


def factorial(n: int) -> int:
    """Return n! (factorial of n)."""
    if n < 0:
        raise ValueError("factorial is not defined for negative integers")
    if n == 0 or n == 1:
        return 1
    result = 1
    for i in range(2, n + 1):
        result *= i
    return result


def fibonacci(n: int) -> int:
    """Return the n-th Fibonacci number (0-indexed)."""
    if n < 0:
        raise ValueError("fibonacci is not defined for negative indices")
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a


class TestMathUtils(unittest.TestCase):
    def test_factorial_zero(self):
        self.assertEqual(factorial(0), 1)

    def test_factorial_one(self):
        self.assertEqual(factorial(1), 1)

    def test_factorial_five(self):
        self.assertEqual(factorial(5), 120)

    def test_factorial_ten(self):
        self.assertEqual(factorial(10), 3628800)

    def test_factorial_negative(self):
        with self.assertRaises(ValueError):
            factorial(-1)

    def test_fibonacci_zero(self):
        self.assertEqual(fibonacci(0), 0)

    def test_fibonacci_one(self):
        self.assertEqual(fibonacci(1), 1)

    def test_fibonacci_five(self):
        self.assertEqual(fibonacci(5), 5)

    def test_fibonacci_ten(self):
        self.assertEqual(fibonacci(10), 55)

    def test_fibonacci_negative(self):
        with self.assertRaises(ValueError):
            fibonacci(-1)


if __name__ == "__main__":
    unittest.main()