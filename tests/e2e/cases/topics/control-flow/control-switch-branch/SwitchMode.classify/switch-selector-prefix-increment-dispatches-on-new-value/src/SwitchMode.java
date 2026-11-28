class SwitchMode {
    static int classify(int code) {
        int x = 1;
        return switch (++x) { case 2 -> 10 + x; default -> 100 + x; };
    }
}
