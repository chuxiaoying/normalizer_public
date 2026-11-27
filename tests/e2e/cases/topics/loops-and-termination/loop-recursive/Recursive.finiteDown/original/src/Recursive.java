class Recursive {
    static int finiteDown(int value) {
        if (value <= 0) return 0;
        return finiteDown(value - 1) + 1;
    }
}
