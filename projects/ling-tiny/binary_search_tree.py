class Node:
    """Represents a single node in the binary search tree."""

    def __init__(self, key):
        self.key = key
        self.left = None
        self.right = None


class BinarySearchTree:
    """A binary search tree with insert and search methods."""

    def __init__(self):
        self.root = None

    def insert(self, key):
        """Insert a key into the BST. If the tree is empty, the key becomes the root."""
        if self.root is None:
            self.root = Node(key)
        else:
            self._insert_recursive(self.root, key)

    def search(self, key):
        """Search for a key in the BST. Returns True if found, False otherwise."""
        return self._search_recursive(self.root, key)

    def _insert_recursive(self, node, key):
        if key < node.key:
            if node.left is None:
                node.left = Node(key)
            else:
                node.left = Node(key)
        elif key > node.key:
            if node.right is None:
                node.right = Node(key)
            else:
                node.right = Node(key)

    def _search_recursive(self, node, key):
        if node is None:
            return False
        if key == node.key:
            return True
        elif key < node.key:
            return self._search_recursive(node.left, key)
        else:
            return self._search_recursive(node.right, key)

    def __repr__(self):
        return f"BinarySearchTree(root={self.root.key if self.root else None})"