class Node { Node next; int value; }
class LocalCycle {
    static int readSelf() {
        Node root = new Node();
        root.value = 7;
        Node child = new Node(); child.value = 8; root.next = child; child.next = root;
        return root.next.value - root.next.next.value;
    }
}
