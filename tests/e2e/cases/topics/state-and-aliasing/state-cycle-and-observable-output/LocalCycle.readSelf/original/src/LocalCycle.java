class Node { Node next; int value; }
class LocalCycle {
    static int readSelf() {
        Node root = new Node();
        root.value = 7;
        root.next = root;
        return root.next.value;
    }
}
