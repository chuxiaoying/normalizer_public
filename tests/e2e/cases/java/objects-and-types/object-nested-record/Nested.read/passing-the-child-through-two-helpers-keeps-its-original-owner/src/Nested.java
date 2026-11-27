class Inner {
    int value;
}

class Outer {
    Inner inner;
}

class Nested {
    static int project(Inner row) { return row.value; }
    static int through(Outer row) { return Nested.project(row.inner); }
    static int read(Outer outer) {
        return Nested.through(outer);
    }
}
