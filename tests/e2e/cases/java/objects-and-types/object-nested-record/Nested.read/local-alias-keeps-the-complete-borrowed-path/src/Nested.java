class Inner {
    int value;
}

class Outer {
    Inner inner;
}

class Nested {
    static int read(Outer outer) {
        Inner borrowed = outer.inner; return borrowed.value;
    }
}
