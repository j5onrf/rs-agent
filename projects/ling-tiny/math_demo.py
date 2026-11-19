#!/usr/bin/env python3


def is_prime(n):
    """Check if a number is prime."""
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    for i in range(3, int(n ** 0.5) + 1, 2):
        if n % i == 0:
            return False
    return True


if __name__ == "__main__":
    test_numbers = [2, 3, 4, 5, 11, 12, 13, 17, 19, 20, 23, 24, 29]
    for num in test_numbers:
        print(f"{num} -> {'prime' if is_prime(num) else 'not prime'}")