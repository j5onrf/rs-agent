"""Math demo script with a prime-checking function."""


def is_prime(n):
    """Check if a given integer is a prime number."""
    if n < 2:
        return False
    for i in range(2, int(n**0.5) + 1):
        if n % i == 0:
            return False
    return True


if __name__ == "__main__":
    test_numbers = [2, 3, 4, 5, 11, 12, 13, 17, 19, 20]
    for num in test_numbers:
        print(f"{num} -> {is_prime(num)}")
