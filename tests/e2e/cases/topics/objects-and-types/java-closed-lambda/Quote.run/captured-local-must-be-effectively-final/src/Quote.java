interface Calculation { int apply(int value); }

class Quote {
    static int run(int input, int delta) {
        int offset = delta + 1;
        Calculation addOffset = value -> value + offset;
        offset++;
        return addOffset.apply(input);
    }
}
