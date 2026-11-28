class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1, 3 -> throw new IllegalArgumentException();
            default -> { return 0; }
        }
    }
}
