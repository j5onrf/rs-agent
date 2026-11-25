def is_prime(n):
    """Return True if n is a prime number, False otherwise."""
    if n <= 1:
        return False
    if n <= 3:
        return True
    if n % 2 == 0 or n % 3 == 0:
        return False
    i = 5
    while i * i <= n:
        if n % i == 0 or n % (i + 2) == 0:
            return False
        i += 6
    return True


if __name__ == "__main__":
    # Inspect the script file itself
    import os
    script_path = os.path.abspath(__file__)
    print(f"Script path: {script_path}")
    with open(script_path, "r") as f:
        content = f.read()
    print("First 50 lines of the script:")
    for line in content.splitlines()[:50]:
        print(line)

    # Run some prime checks
    test_numbers = [1, 2, 3, 4, 5, 10, 17, 19, 20]
    for num in test_numbers:
        result = is_prime(num)
        print(f"{num} is prime? {result}")
