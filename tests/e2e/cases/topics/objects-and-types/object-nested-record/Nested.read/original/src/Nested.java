class Inner {
    int value;
}

class Outer {
    Inner inner;
}

class Nested {
    static int read(Outer outer) {
        return outer.inner.value;
    }
}
