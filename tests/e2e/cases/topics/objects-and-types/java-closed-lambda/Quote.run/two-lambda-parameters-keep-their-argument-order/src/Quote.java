interface Calculation { int apply(int first, int second); }

class Quote {
    static int run(int input, int delta) {
        Calculation subtract = (first, second) -> second - first;
        return subtract.apply(input, delta);
    }
}
