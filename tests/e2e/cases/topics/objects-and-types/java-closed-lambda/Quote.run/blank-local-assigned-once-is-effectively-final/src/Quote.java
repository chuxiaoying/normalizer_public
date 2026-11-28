interface Calculation { int apply(int value); }

class Quote {
    static int run(int input, int delta) {
        int offset;
        int adjusted = delta + 1;
        offset = adjusted;
        Calculation add = value -> value + offset;
        return add.apply(input);
    }
}
