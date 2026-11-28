class SwitchMode {
    static int classify(int code) {
        int x = 1;
        return switch (x++) { case 1 -> 100 + x; default -> 10 + x; };
    }
}
