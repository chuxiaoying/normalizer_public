import normalizer.intrinsics.N;

class Box {
    int value;
}

class Alias {
    @N.EntryState
    Box box;

    int read() {
        Box alias = this.box;
        alias.value = alias.value + 2; alias.value = alias.value - 1; return this.box.value;
    }
}

class Beans {
    static final Alias ENTRY = new Alias();
    static {
        ENTRY.box = new Box();
    }
}
