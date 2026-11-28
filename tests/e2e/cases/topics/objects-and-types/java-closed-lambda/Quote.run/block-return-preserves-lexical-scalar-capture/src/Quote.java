interface Calculation { int apply(int value); }

class Quote {
    static int run(int input, int delta) {
        int offset = delta + 1;
        Calculation addOffset = (value) -> { return value + offset; };
        return addOffset.apply(input);
    }
}
