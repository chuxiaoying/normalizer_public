class SwitchMode {
    static int classify(int code) {
        return switch (code) {
            case 1 -> { int value = 10; }
            default -> 0;
        };
    }
}
