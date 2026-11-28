class SwitchMode {
    static int classify(int code) {
        int x;
        return switch (++x) { case 2 -> 10; default -> 20; };
    }
}
