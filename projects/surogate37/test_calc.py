#!/usr/bin/env python3


def add(a, b):
    return a + b


def subtract(a, b):
    return a - b


def is_prime(n):
    if n < 2:
        return False
    for i in range(2, int(n**0.5) + 1):
        if n % i == 0:
            return False
    return True


if __name__ == "__main__":
    print(add(10, 5))
    print(subtract(10, 5))
