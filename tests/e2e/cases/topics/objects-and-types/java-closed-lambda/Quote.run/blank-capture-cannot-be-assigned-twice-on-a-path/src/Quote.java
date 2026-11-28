interface Calculation { int apply(int value); }

class Quote {
    static int run(int input) {
        int offset;
        offset = 2;
        offset = 3;
        Calculation add = value -> value + offset;
        return add.apply(input);
    }
}
