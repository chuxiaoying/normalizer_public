interface Calculation { int apply(int value); }

class Quote {
    static int run(int input) {
        int zero = 0;
        Calculation compute = value -> value / zero;
        return 7;
    }
}
