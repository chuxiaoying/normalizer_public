class Recursive {
    static int down(int value) {
        if (value <= 0) return 0;
        return down(value) + 1;
    }
}
