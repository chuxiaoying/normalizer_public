class SwitchMode {
    static int classify(int code) {
        int x = code;
        return switch (x++) { case 1 -> 10; default -> 20; };
    }
}
