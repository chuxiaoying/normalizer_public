class SwitchMode {
    static int classify(int code) {
        return switch (code) {
            case 1 -> {
                int value = 9;
                value += 1;
                yield value;
            }
            case 2 -> 20;
            default -> { yield 0; }
        };
    }
}
