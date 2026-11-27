class Recursive {
    static int finiteDown(int value) {
        if (value <= 0) return 0;
        if (value == 1) return 1;
        return finiteDown(value - 2) + 2;
    }
}
