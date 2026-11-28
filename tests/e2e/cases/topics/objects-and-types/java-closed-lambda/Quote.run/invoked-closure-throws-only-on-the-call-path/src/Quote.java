interface Calculation { int apply(int value); }

class Quote {
    static int run(boolean active, int input) {
        int zero = 0;
        Calculation compute = value -> value / zero;
        if (active) return compute.apply(input);
        return 7;
    }
}
