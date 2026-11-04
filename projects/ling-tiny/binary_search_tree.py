class Node:
    """Represents a single node in the binary search tree."""
    __slots__ = ("key", "left", "right")

    def __init__(self, key):
        self.key = key
        self.left = None
        self.right = None


class BinarySearchTree:
    """A binary search tree supporting insert and search operations."""

    def __init__(self):
        self.root = None

    def insert(self, key):
        """Insert a key into the BST. If the key already exists, it is not added."""
        if self.root is None:
            self.root = Node(key)
        else:
            self._insert(self.root, key)

    def _insert(self, node, key):
        if key < node.key:
            if node.left is None:
                node.left = Node(key)
            else:
                self._insert(node.left, key)
        elif key > node.key:
            if node.right is None:
                node.right = Node(key)
            else:
                self._insert(node.right, key)

    def search(self, key):
        """Search for a key in the BST. Returns True if found, False otherwise."""
        return self._search(self.root, key)

    def _search(self, node, key):
        if node is None:
            return False
        if key == node.key:
            return True
        elif key < node.key:
            return self._search(node.left, key)
        else:
            return self._search(node.right, key)

    def __contains__(self, key):
        return self.search(key)

    def __str__(self):
        return " ".join(str(x) for x in self._inorder(self.root, []))

    def _inorder(self, node, result):
        if node is not None:
            self._inorder(node.left, result)
            result.append(node.key)
            self._inorder(node.right, result)
        return result