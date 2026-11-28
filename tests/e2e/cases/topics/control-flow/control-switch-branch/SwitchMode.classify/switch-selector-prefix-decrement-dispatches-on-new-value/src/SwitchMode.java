class SwitchMode {
    static int classify(int code) {
        int x = 2;
        return switch (--x) { case 1 -> 10 + x; default -> 100 + x; };
    }
}
