import normalizer.intrinsics.N;

class StatePair {
    int left;
    int right;
}

class StateSnapshot {
    int leftBefore;
    int leftAfter;
    int rightBefore;
    int rightAfter;

    StateSnapshot(int leftBefore, int leftAfter, int rightBefore, int rightAfter) {
        this.leftBefore = leftBefore;
        this.leftAfter = leftAfter;
        this.rightBefore = rightBefore;
        this.rightAfter = rightAfter;
    }
}

class SnapshotEntry {
    @N.EntryState StatePair pair;

    StateSnapshot run(int next, int later) {
        StatePair edit = this.pair;
        StatePair reader = edit;
        int leftBefore = reader.left;
        int rightBefore = reader.right;
        edit.left = next;
        StateSnapshot result = new StateSnapshot(leftBefore, reader.left, rightBefore, reader.right);
        edit.right = later;
        return result;
    }
}

class Beans {
    static final SnapshotEntry ENTRY = new SnapshotEntry();
    static {
        ENTRY.pair = new StatePair();
    }
}
