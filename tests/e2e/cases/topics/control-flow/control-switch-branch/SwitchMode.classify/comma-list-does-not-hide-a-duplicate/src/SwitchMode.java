class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 1, 3 -> { return 7; }
            case 2, 1 -> { return 8; }
            default -> { return 0; }
        }
    }
}
