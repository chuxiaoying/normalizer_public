class SwitchMode {
    static int classify(int code) {
        final int x = 1;
        return switch (++x) { case 2 -> 10; default -> 20; };
    }
}
