import normalizer.intrinsics.N;

class StateList {
    @N.EntryState
    int[] values;

    int update(int next) {
        
        
        return values.length;
    }
}

class Beans {
    static final StateList ENTRY = new StateList();
    static {
        ENTRY.values = new int[0];
    }
}
