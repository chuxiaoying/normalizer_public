class SwitchMode {
    static int classify(int code) {
        switch (code) {
            case 3, 1 -> { return 7; }
            default -> { return 0; }
        }
    }
}
