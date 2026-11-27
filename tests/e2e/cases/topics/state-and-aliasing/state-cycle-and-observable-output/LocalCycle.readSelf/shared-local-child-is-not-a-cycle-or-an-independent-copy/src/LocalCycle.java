class Node { Node next; Node other; int value; }
class LocalCycle {
    static int readSelf() {
        Node root = new Node();
        root.value = 7;
        Node leaf = new Node(); root.next = leaf; root.other = leaf; Node throughNext = root.next; throughNext.value = 12;
        return root.other.value;
    }
}
