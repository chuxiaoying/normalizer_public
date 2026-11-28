interface Calculation { int apply(int value); }

class Quote {
    static int run(int input) {
        Calculation addOffset = (first, second) -> first + second;
        return addOffset.apply(input);
    }
}
