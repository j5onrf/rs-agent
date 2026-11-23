#!/usr/bin/env python3


def is_prime(n):
    """Return True if n is a prime number, else False."""
    if n < 2:
        return False
    for i in range(2, int(n**0.5) + 1):
        if n % i == 0:
            return False
    return True


if __name__ == "__main__":
    for num in range(2, 20):
        print(f"{num}: {'prime' if is_prime(num) else 'not prime'}")
