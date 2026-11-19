def is_prime(n):
    """Return True if n is a prime number, False otherwise."""
    if n <= 1:
        return False
    if n <= 3:
        return True
    if n % 2 == 0:
        return False
    i = 3
    while i * i <= n:
        if n % i == 0:
            return False
        i += 2
    return True


if __name__ == "__main__":
    # Inspect the file: print its path and content summary
    import os
    print(f"File: {os.path.abspath(__file__)}")
    with open(__file__, "r") as f:
        lines = f.readlines()
        print(f"Number of lines: {len(lines)}")
        # Print first 5 lines
        for line in lines[:5]:
            print(line.rstrip())

    # Run the prime check on a few numbers
    test_numbers = [1, 2, 3, 4, 17, 20, 97]
    for num in test_numbers:
        print(f"{num} is prime? {is_prime(num)}")
