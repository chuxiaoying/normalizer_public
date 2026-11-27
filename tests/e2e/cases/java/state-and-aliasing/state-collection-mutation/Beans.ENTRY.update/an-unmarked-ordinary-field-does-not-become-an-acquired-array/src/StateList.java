import normalizer.intrinsics.N;

class StateList { int audit;
    @N.EntryState
    int[] values;

    int update(int next) {
        int[] alias = values;
        values[0] = next;
        return alias[0];
    }
}

class Beans {
    static final StateList ENTRY = new StateList();
    static {
        ENTRY.values = new int[0];
    }
}
