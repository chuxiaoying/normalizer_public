import normalizer.intrinsics.N;

class Node { Node next; int value; }
class CycleState {
    @N.EntryState
    Node root;
    int read() { return root.next.value; }
}

class Beans {
    static final CycleState ENTRY = new CycleState();
    static final Node ROOT = new Node();
    static {
        ROOT.next = ROOT;
        ENTRY.root = ROOT;
    }
}
