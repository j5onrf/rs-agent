def is_prime(n: int) -> bool:
    """Check if a number is prime."""
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    for i in range(3, int(n**0.5) + 1, 2):
        if n % i == 0:
            return False
    return True


def main():
    test_numbers = [1, 2, 3, 4, 5, 17, 20, 23, 29, 30]
    for num in test_numbers:
        result = "prime" if is_prime(num) else "not prime"
        print(f"{num} is {result}")


if __name__ == "__main__":
    main()