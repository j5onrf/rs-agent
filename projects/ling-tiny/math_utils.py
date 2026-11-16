#!/usr/bin/env python3
"""math_utils.py — factorial and fibonacci implementations with tests."""

import math
import unittest


def factorial(n: int) -> int:
    """Return n! (the factorial of n)."""
    if n < 0:
        raise ValueError(f"n must be non-negative, got {n}")
    result = 1
    for i in range(2, n + 1):
        result *= i
    return result


def fibonacci(n: int) -> int:
    """Return the n-th Fibonacci number (0-indexed). F(0)=0, F(1)=1."""
    if n < 0:
        raise ValueError(f"n must be non-negative, got {n}")
    if n == 0:
        return 0
    if n == 1:
        return 1
    a, b = 0, 1
    for _ in range(2, n + 1):
        a, b = b, a + b
    return b


# ── Tests ──────────────────────────────────────────────────────
class TestFactorial(unittest.TestCase):
    def test_zero(self):
        self.assertEqual(factorial(0), 1)

    def test_one(self):
        self.assertEqual(factorial(1), 1)

    def test_small(self):
        self.assertEqual(factorial(5), 120)

    def test_large(self):
        self.assertEqual(factorial(10), 3628800)

    def test_negative_raises(self):
        with self.assertRaises(ValueError):
            factorial(-1)


class TestFibonacci(unittest.TestCase):
    def test_zero(self):
        self.assertEqual(fibonacci(0), 0)

    def test_one(self):
        self.assertEqual(fibonacci(1), 1)

    def test_two(self):
        self.assertEqual(fibonacci(2), 1)

    def test_five(self):
        self.assertEqual(fibonacci(5), 5)

    def test_ten(self):
        self.assertEqual(fibonacci(10), 55)

    def test_large(self):
        self.assertEqual(fibonacci(20), 6765)

    def test_negative_raises(self):
        with self.assertRaises(ValueError):
            fibonacci(-1)


if __name__ == "__main__":
    unittest.main()
