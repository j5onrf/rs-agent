class Node:
    def __init__(self, key):
        self.key = key
        self.left = None
        self.right = None


class BinarySearchTree:
    def __init__(self):
        self.root = None

    def insert(self, key):
        if self.root is None:
            self.root = Node(key)
        else:
            self._insert_recursive(self.root, key)

    def _insert_recursive(self, node, key):
        if key < node.key:
            if node.left is None:
                node.left = Node(key)
            else:
                self._insert_recursive(node.left, key)
        elif key > node.key:
            if node.right is None:
                node.right = Node(key)
            else:
                self._insert_recursive(node.right, key)
        # If key equals node.key, we do nothing (no duplicates)

    def search(self, key):
        return self._search_recursive(self.root, key)

    def delete(self, key):
        self.root = self._delete_recursive(self.root, key)

    def _delete_recursive(self, node, key):
        if node is None:
            return None
        if key < node.key:
            node.left = self._delete_recursive(node.left, key)
        elif key > node.key:
            node.right = self._delete_recursive(node.right, key)
        else:
            # node to delete found
            if node.left is None:
                return node.right
            elif node.right is None:
                return node.left
            else:
                # two children: get inorder successor (smallest in right subtree)
                min_node = self._find_min(node.right)
                node.key = min_node.key
                node.right = self._delete_recursive(node.right, min_node.key)
        return node

    def _find_min(self, node):
        current = node
        while current.left is not None:
            current = current.left
        return current

    def _search_recursive(self, node, key):
        if node is None:
            return False
        if key == node.key:
            return True
        elif key < node.key:
            return self._search_recursive(node.left, key)
        else:
            return self._search_recursive(node.right, key)


# Example usage:
if __name__ == "__main__":
    bst = BinarySearchTree()
    for val in [7, 3, 9, 1, 5, 8]:
        bst.insert(val)
    print(bst.search(5))   # True
    print(bst.search(4))   # False