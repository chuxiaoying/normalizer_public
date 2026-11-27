class Node { Node next; int value; }
class LocalCycle {
    static Node exportSelf() {
        Node root = new Node();
        root.value = 7;
        root.next = root;
        return root;
    }
}
