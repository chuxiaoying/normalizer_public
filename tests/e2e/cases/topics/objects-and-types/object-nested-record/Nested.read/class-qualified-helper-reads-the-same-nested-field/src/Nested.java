class Inner {
    int value;
}

class Outer {
    Inner inner;
}

class Nested {
    static int project(Outer row) { return row.inner.value; }
    static int read(Outer outer) {
        return Nested.project(outer);
    }
}
