import normalizer.intrinsics.N;

class Box {
    int value;
}

class Alias {
    @N.EntryState
    Box box;

    int read() {
        Box alias = this.box;
        return alias.value + 1;
    }
}

class Beans {
    static final Alias ENTRY = new Alias();
    static {
        ENTRY.box = new Box();
    }
}
