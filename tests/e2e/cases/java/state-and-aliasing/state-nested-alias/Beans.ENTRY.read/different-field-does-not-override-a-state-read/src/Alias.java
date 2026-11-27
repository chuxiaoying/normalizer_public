import normalizer.intrinsics.N;

class Box {
    int value; int other;
}

class Alias {
    @N.EntryState
    Box box;

    int read() {
        Box alias = this.box;
        alias.other = 9; return this.box.value;
    }
}

class Beans {
    static final Alias ENTRY = new Alias();
    static {
        ENTRY.box = new Box();
    }
}
