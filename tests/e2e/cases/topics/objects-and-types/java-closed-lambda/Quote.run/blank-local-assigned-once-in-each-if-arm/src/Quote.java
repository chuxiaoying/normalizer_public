interface Calculation { int apply(int value); }

class Quote {
    static int run(boolean active) {
        int offset;
        if (active) { offset = 2; } else { offset = 3; }
        Calculation choose = ignored -> offset;
        return choose.apply(0);
    }
}
