class Inner {
    int value;
}

class Outer {
    Inner inner; Inner alternate;
}

class Nested {
    static int read(Outer outer) {
        Inner a = outer.inner; Inner b = outer.alternate; return a.value - b.value;
    }
}
