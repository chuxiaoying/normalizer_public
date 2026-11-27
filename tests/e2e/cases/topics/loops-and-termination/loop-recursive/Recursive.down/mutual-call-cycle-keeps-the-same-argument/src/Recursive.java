class Recursive {
    static int down(int value) {
        if (value <= 0) return 0;
        return step(value);
    }
    static int step(int value) {
        return down(value);
    }
}
