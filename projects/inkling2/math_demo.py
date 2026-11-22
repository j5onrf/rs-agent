def is_prime(n):
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    import math
    for i in range(3, int(math.sqrt(n)) + 1, 2):
        if n % i == 0:
            return False
    return True


if __name__ == "__main__":
    for num in [2, 3, 4, 7, 10, 17, 25, 29]:
        print(num, "is prime" if is_prime(num) else "is not prime")
