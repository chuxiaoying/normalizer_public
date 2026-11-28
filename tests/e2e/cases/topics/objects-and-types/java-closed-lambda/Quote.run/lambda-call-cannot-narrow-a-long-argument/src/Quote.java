interface Calculation { int apply(int value); }

class Quote {
    static int run(long input) {
        Calculation calculate = value -> value + 1;
        return calculate.apply(input);
    }
}
