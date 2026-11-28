class SwitchMode {
    static int classify(int code) {
        return switch (code) {
            case 1 -> throw new IllegalArgumentException();
            default -> throw new IllegalArgumentException();
        };
    }
}
