class SwitchMode {
    static int classify(int code) {
        return switch (code) {
            case 1 -> 10;
            case 2 -> 20;
            default -> 0;
        };
    }
}
